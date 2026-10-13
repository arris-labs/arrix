//! `arrix serve`: the session protocol on JSON-RPC (ADR-0023). `--stdio`
//! serves one client one document over stdin and stdout, with the framing
//! the Tier 2 host speaks to a plugin (ADR-0022): a line per message.
//! `--schema` prints the protocol's JSON Schemas as `serve` speaks them
//! (`codec::wire_schemas`), kept fresh against
//! `tests/golden/protocol.schema.json`. `--plugin <dir>` starts a Tier 2
//! plugin with the session, and `session.load_plugin` and
//! `session.reload_plugin` change the registry while it runs (ADR-0024).

mod codec;

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};

use arrix_doc::{
    Document, EventStream, LocalSession, PROTOCOL_VERSION, Registry, Request, Session as _,
};
use arrix_plugin_host::DirLoader;
use arrix_plugin_host::wire::rpc::{
    INVALID_PARAMS, INVALID_REQUEST, LineError, LineReader, METHOD_NOT_FOUND, Message, Response,
    RpcError,
};
use serde_json::{Value, json};

use crate::load::Loaded;
use crate::plugins;

/// `initialize` asked for a protocol version this `serve` does not speak.
const VERSION_MISMATCH: i64 = -32002;

pub struct Args<'a> {
    pub schema: bool,
    pub stdio: bool,
    pub doc: Option<&'a Path>,
    pub plugins: &'a [PathBuf],
}

pub fn run(args: &Args) -> ExitCode {
    if args.schema {
        let text =
            serde_json::to_string_pretty(&codec::wire_schemas()).expect("a schema is plain data");
        return match writeln!(std::io::stdout(), "{text}") {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::from(2),
        };
    }
    if !args.stdio {
        eprintln!("arrix serve: say how: --stdio, or --schema");
        return ExitCode::from(2);
    }
    let session = match open(args.doc, args.plugins) {
        Ok(session) => session,
        Err(message) => {
            eprintln!("arrix serve: {message}");
            return ExitCode::from(2);
        }
    };
    serve_stdio(session);
    ExitCode::SUCCESS
}

fn open(doc: Option<&Path>, dirs: &[PathBuf]) -> Result<LocalSession, String> {
    let document = match doc {
        None => Document::default(),
        Some(path) => {
            let name = path.display();
            let source = Loaded::open(path).map_err(|e| format!("{name}: {e}"))?;
            arrix_doc::open(&source).map_err(|e| format!("{name}: {e}"))?
        }
    };
    let mut registry: Registry = plugins::registry();
    let started = plugins::register_dirs(&mut registry, dirs)?;
    let known = started
        .into_iter()
        .zip(dirs)
        .map(|(m, d)| (m.id, d.clone()));
    let loader =
        DirLoader::with_plugins(known).replacing(plugins::manifests().into_iter().map(|m| m.id));
    LocalSession::open_with(document, registry, Some(Box::new(loader))).map_err(|e| e.to_string())
}

/// The one writer's queue: every line `serve` prints goes through it.
type Out = Sender<String>;

/// Held while a request is served and while an event is queued, so a
/// request's response is queued before any event it caused (ADR-0023 §3).
type Gate = Arc<Mutex<()>>;

fn serve_stdio(session: LocalSession) {
    let (out, lines) = channel::<String>();
    let writer = std::thread::spawn(move || {
        let mut stdout = std::io::stdout().lock();
        for line in lines {
            if writeln!(stdout, "{line}")
                .and_then(|()| stdout.flush())
                .is_err()
            {
                break;
            }
        }
    });
    let gate = Gate::default();
    let mut forwarder = None;
    let mut input = LineReader::new(std::io::stdin().lock());
    loop {
        let line = match input.next_line() {
            Ok(Some(line)) => line,
            Ok(None) | Err(LineError::Io(_)) => break,
            Err(e) => {
                if let Some(malformed) = e.malformed() {
                    send(&out, &malformed.into_response());
                }
                continue;
            }
        };
        let request = match Message::parse(&line) {
            Ok(Message::Request(request)) => request,
            Ok(Message::Response(_)) => continue,
            Err(malformed) => {
                send(&out, &malformed.into_response());
                continue;
            }
        };
        if request.method == "exit" {
            break;
        }
        let guard = gate.lock().unwrap_or_else(|p| p.into_inner());
        let answer = if request.method == "initialize" {
            initialize(&request.params, forwarder.is_some())
        } else if forwarder.is_none() {
            Err(RpcError::new(INVALID_REQUEST, "send `initialize` first"))
        } else {
            call(&session, &request.method, request.params)
        };
        let handshake = request.method == "initialize" && answer.is_ok();
        if let Some(id) = request.id {
            send(
                &out,
                &Response {
                    id: Some(id),
                    outcome: answer,
                },
            );
        }
        if handshake {
            forwarder = Some(forward(session.subscribe(), &out, &gate));
        }
        drop(guard);
    }
    drop(session);
    if let Some(forwarder) = forwarder {
        let _ = forwarder.join();
    }
    drop(out);
    let _ = writer.join();
}

fn send(out: &Out, response: &Response) {
    let _ = out.send(Message::Response(response.clone()).to_line());
}

/// The events as notifications, in the order the session made them.
fn forward(events: EventStream, out: &Out, gate: &Gate) -> std::thread::JoinHandle<()> {
    let (out, gate) = (out.clone(), Arc::clone(gate));
    std::thread::spawn(move || {
        for event in events {
            let mut params = serde_json::to_value(&event).expect("an event is plain data");
            codec::encode_event(&mut params);
            let notification = Message::Request(arrix_plugin_host::wire::rpc::Request {
                id: None,
                method: "session.event".into(),
                params,
            });
            let _guard = gate.lock().unwrap_or_else(|p| p.into_inner());
            if out.send(notification.to_line()).is_err() {
                break;
            }
        }
    })
}

fn initialize(params: &Value, again: bool) -> Result<Value, RpcError> {
    if again {
        return Err(RpcError::new(INVALID_REQUEST, "already initialized"));
    }
    match params.get("protocol").and_then(Value::as_u64) {
        Some(v) if v == u64::from(PROTOCOL_VERSION) => Ok(json!({
            "protocol": PROTOCOL_VERSION,
            "name": "arrix",
            "version": env!("CARGO_PKG_VERSION"),
            "methods": codec::methods(),
        })),
        Some(v) => Err(RpcError::new(
            VERSION_MISMATCH,
            format!("this serve speaks session protocol {PROTOCOL_VERSION}, not {v}"),
        )),
        None => Err(RpcError::new(
            INVALID_PARAMS,
            "`initialize` takes {\"protocol\": <the session protocol's version>}",
        )),
    }
}

/// One `session.*` method: its params are its request's fields, its result
/// `{"ok": outcome}` or `{"err": rejected}` (ADR-0023 §1).
fn call(session: &LocalSession, method: &str, params: Value) -> Result<Value, RpcError> {
    if method == "session.save" {
        return Ok(codec::saved_files(&session.save().0));
    }
    let Some((_, variant)) = codec::REQUESTS.iter().find(|(m, _)| *m == method) else {
        return Err(RpcError::new(
            METHOD_NOT_FOUND,
            format!("no method `{method}` here"),
        ));
    };
    let params = if params.is_null() { json!({}) } else { params };
    let request: Request = serde_json::from_value(json!({ *variant: params }))
        .map_err(|e| RpcError::new(INVALID_PARAMS, format!("{method}: {e}")))?;
    Ok(match session.request(request) {
        Ok(outcome) => json!({"ok": outcome}),
        Err(rejected) => json!({"err": rejected}),
    })
}
