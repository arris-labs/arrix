//! `arrix serve --stdio` (ADR-0023), driven as an agent would, on the
//! spawned binary: the handshake, the plate built by commands with its
//! `Finished` and `Meshed` read, a stale and a refused submit as a result's
//! `err`, `session.save` equal to the same commands' in-process save, a
//! response ahead of the events it caused, and each protocol error. Every
//! notification is checked against the schemas `serve --schema` prints.

mod common;

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command as Process, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use arrix_core::Id;
use arrix_doc::{
    AuthorId, Command, CommandEnvelope, Document, Generation, MemorySource, Part, apply, open, save,
};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use common::{read_tree, workspace};
use serde_json::{Value, json};

const ME: AuthorId = AuthorId(Id(1));
const THEM: AuthorId = AuthorId(Id(2));

struct Serve {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    /// Every line heard so far, in order: responses and notifications.
    heard: Vec<Value>,
    next_id: i64,
    event_schema: jsonschema::Validator,
}

impl Serve {
    /// Spawned, not yet initialized.
    fn spawn() -> Self {
        let mut child = Process::new(env!("CARGO_BIN_EXE_arrix"))
            .args(["serve", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("run arrix");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let schemas = Process::new(env!("CARGO_BIN_EXE_arrix"))
            .args(["serve", "--schema"])
            .output()
            .expect("run arrix");
        let schemas: Value = serde_json::from_slice(&schemas.stdout).expect("the schemas");
        Serve {
            child,
            stdin,
            lines,
            heard: Vec::new(),
            next_id: 0,
            event_schema: jsonschema::validator_for(&schemas["event"]).expect("it compiles"),
        }
    }

    /// Spawned and past the handshake.
    fn start() -> Self {
        let mut s = Self::spawn();
        let hello = s.call("initialize", json!({"protocol": 1}));
        assert_eq!(hello["result"]["protocol"], 1, "{hello}");
        s
    }

    fn write(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let line = self
            .lines
            .recv_timeout(Duration::from_secs(60))
            .expect("serve writes within the timeout");
        let value: Value = serde_json::from_str(&line).expect("a line is JSON");
        if value["method"] == "session.event" {
            let errors: Vec<String> = self
                .event_schema
                .iter_errors(&value["params"])
                .map(|e| format!("{e} at {}", e.instance_path()))
                .collect();
            assert!(errors.is_empty(), "{line:.600}\n{}", errors.join("\n"));
        }
        self.heard.push(value.clone());
        value
    }

    /// A request and its response, hearing whatever comes before it.
    fn call(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.write(
            &json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string(),
        );
        loop {
            let message = self.read();
            if message["id"] == id {
                return message;
            }
        }
    }

    /// `session.<method>`'s result.
    fn session(&mut self, method: &str, params: Value) -> Value {
        let response = self.call(&format!("session.{method}"), params);
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }

    fn submit(&mut self, author: AuthorId, base: u64, command: Command) -> Value {
        let envelope = CommandEnvelope {
            author,
            base: Generation(base),
            command,
        };
        self.session("submit", serde_json::to_value(envelope).unwrap())
    }

    /// Hears until some notification satisfies `done`.
    fn hear_event(&mut self, done: impl Fn(&Value) -> bool) -> Value {
        if let Some(e) = self.events().into_iter().find(|e| done(e)) {
            return e;
        }
        loop {
            let message = self.read();
            if message["method"] == "session.event" && done(&message["params"]) {
                return message["params"].clone();
            }
        }
    }

    fn events(&self) -> Vec<Value> {
        self.heard
            .iter()
            .filter(|m| m["method"] == "session.event")
            .map(|m| m["params"].clone())
            .collect()
    }
}

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.write_exit();
        let _ = self.child.wait();
    }
}

impl Serve {
    fn write_exit(&mut self) -> std::io::Result<()> {
        writeln!(self.stdin, r#"{{"jsonrpc":"2.0","method":"exit"}}"#)?;
        self.stdin.flush()
    }
}

/// The standoff scenario's plate as the commands an agent sends: its
/// parameters, its part, then the sketch and the extrusion that make it.
fn plate() -> Vec<Command> {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/standoff"), "", &mut files);
    let doc = open(&MemorySource(files)).unwrap();
    let mut commands: Vec<Command> = doc
        .params()
        .iter()
        .map(|(id, record)| Command::AddParam {
            param: *id,
            record: record.clone(),
        })
        .collect();
    let part = doc.parts().values().next().unwrap();
    commands.push(Command::AddPart {
        part: Part {
            history: vec![],
            features: BTreeMap::new(),
            rollback: None,
            ..part.clone()
        },
    });
    for (at, id) in part.history.iter().take(2).enumerate() {
        commands.push(Command::AddFeature {
            part: part.id,
            at,
            record: part.features[id].clone(),
        });
    }
    commands
}

fn is(kind: &'static str, generation: u64) -> impl Fn(&Value) -> bool {
    move |e| e["kind"] == kind && e["generation"] == generation
}

#[test]
fn the_plate_is_built_by_commands_and_read_back() {
    let mut s = Serve::start();
    let commands = plate();
    let n = commands.len() as u64;
    for (base, command) in commands.into_iter().enumerate() {
        let result = s.submit(ME, base as u64, command);
        assert_eq!(
            result["ok"]["applied"]["generation"],
            base as u64 + 1,
            "{result}"
        );
    }
    s.hear_event(is("finished", n));

    // `Opened` was the first notification, the handshake's response ahead.
    let kinds: Vec<String> = s
        .events()
        .iter()
        .map(|e| e["kind"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(kinds[0], "opened");
    assert_eq!(s.heard[0]["id"], 1, "the response to initialize is first");

    // Coarse, then refined, as base64 runs that decode to whole elements.
    let refined = s.hear_event(|e| e["kind"] == "meshed" && e["detail"] == "refined");
    let mesh = &refined["mesh"]["Ok"];
    let decode = |key: &str| B64.decode(mesh[key].as_str().unwrap()).unwrap();
    assert!(!decode("positions").is_empty());
    assert_eq!(decode("positions").len(), decode("normals").len());
    assert_eq!(decode("positions").len() % 12, 0);
    assert_eq!(decode("triangles").len() % 12, 0);

    // An export's bytes cross as base64 too: a binary STL of the plate.
    let queued = s.session("export", json!({"bodies": [], "format": "stl"}));
    assert_eq!(queued["ok"], "exporting");
    let exported = s.hear_event(|e| e["kind"] == "exported");
    let stl = B64
        .decode(exported["bytes"]["Ok"].as_str().unwrap())
        .unwrap();
    assert!(
        stl.len() > 84 && (stl.len() - 84).is_multiple_of(50),
        "{} bytes",
        stl.len()
    );

    // The final bodies are in the evaluation of the last generation.
    let feature_events = s
        .events()
        .into_iter()
        .filter(|e| e["kind"] == "evaluated" && e["generation"] == n)
        .count();
    assert_eq!(feature_events, 2);
}

#[test]
fn a_stale_and_a_refused_submit_come_back_as_a_results_err() {
    let mut s = Serve::start();
    let commands = plate();
    let last = commands.len() as u64;
    for (base, command) in commands.iter().cloned().enumerate() {
        s.submit(ME, base as u64, command);
    }
    let Command::AddParam { param, record } = commands[0].clone() else {
        panic!("the plate starts with its parameter");
    };
    // Another author, still at generation 0, edits what ME has since touched.
    let stale = s.submit(
        THEM,
        0,
        Command::SetParam {
            param,
            expr: arrix_doc::expr::Expr::parse("70 mm").unwrap(),
        },
    );
    assert!(stale["err"]["stale"].is_object(), "{stale}");
    assert_eq!(stale["err"]["stale"]["base"], 0);

    let refused = s.submit(ME, last, Command::AddParam { param, record });
    assert!(refused["err"]["refused"].is_object(), "{refused}");
    let nothing = s.session("redo", json!({"author": THEM}));
    assert_eq!(nothing["err"], "nothing_to_redo");

    // The refusals changed nothing: the next fresh command lands at `last + 1`.
    let ok = s.submit(
        ME,
        last,
        Command::SetParam {
            param,
            expr: arrix_doc::expr::Expr::parse("70 mm").unwrap(),
        },
    );
    assert_eq!(ok["ok"]["applied"]["generation"], last + 1);
}

#[test]
fn session_save_equals_the_same_commands_saved_in_process() {
    let mut s = Serve::start();
    let mut doc = Document::default();
    for (base, command) in plate().into_iter().enumerate() {
        doc = apply(&doc, &command).unwrap().document;
        s.submit(ME, base as u64, command);
    }
    let saved = s.session("save", json!({}));
    let files: BTreeMap<String, String> = saved["files"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(p, t)| (p.clone(), t.as_str().unwrap().to_owned()))
        .collect();
    assert!(saved["blobs"].as_object().unwrap().is_empty());
    let want: BTreeMap<String, String> = save(&doc)
        .0
        .into_iter()
        .map(|(p, b)| (p, String::from_utf8(b).unwrap()))
        .collect();
    assert_eq!(files, want);
}

#[test]
fn a_response_comes_before_the_events_its_request_caused() {
    let mut s = Serve::start();
    for (base, command) in plate().into_iter().enumerate() {
        s.submit(ME, base as u64, command);
    }
    s.hear_event(is("finished", 4));
    let position = |matches: &dyn Fn(&Value) -> bool| s.heard.iter().position(matches);
    for generation in 1..=4u64 {
        let response = position(&|m| m["id"] == generation as i64 + 1).unwrap();
        let applied = position(&|m| {
            m["params"]["kind"] == "applied" && m["params"]["change"]["generation"] == generation
        })
        .unwrap();
        assert!(response < applied, "generation {generation}");
    }
}

#[test]
fn each_protocol_error_is_a_json_rpc_error() {
    let mut s = Serve::spawn();
    let code = |response: &Value| response["error"]["code"].as_i64().unwrap();

    // Before the handshake, a session method is refused.
    assert_eq!(
        code(&s.call("session.undo", json!({"author": "0000000000001"}))),
        -32600
    );
    // The wrong version.
    assert_eq!(code(&s.call("initialize", json!({"protocol": 99}))), -32002);
    assert_eq!(code(&s.call("initialize", json!({}))), -32602);
    assert_eq!(
        s.call("initialize", json!({"protocol": 1}))["result"]["protocol"],
        1
    );
    assert_eq!(code(&s.call("initialize", json!({"protocol": 1}))), -32600);

    assert_eq!(code(&s.call("session.explode", json!({}))), -32601);
    assert_eq!(code(&s.call("session.undo", json!({}))), -32602);
    assert_eq!(
        code(&s.call("session.undo", json!({"author": "not an id"}))),
        -32602
    );
    assert_eq!(
        code(&s.call("session.submit", json!({"author": "0000000000001"}))),
        -32602
    );

    // A line that is not JSON, and one that is not a request.
    s.write("{not json");
    assert_eq!(code(&s.read()), -32700);
    s.write("[1, 2]");
    assert_eq!(code(&s.read()), -32600);

    // A line past the limit is skipped, and the next is served.
    let long = "x".repeat(16 * 1024 * 1024 + 1);
    s.write(&long);
    assert_eq!(code(&s.read()), -32001);
    let after = s.call("session.undo", json!({"author": "0000000000001"}));
    assert_eq!(after["result"]["err"], "nothing_to_undo");
}

#[test]
fn a_document_opens_from_a_path_and_a_bad_path_exits_2() {
    let out = Process::new(env!("CARGO_BIN_EXE_arrix"))
        .args(["serve", "--stdio", "/no/such/document"])
        .stdin(Stdio::null())
        .output()
        .expect("run arrix");
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());

    let doc = workspace().join("tests/docs/slice");
    let mut child = Process::new(env!("CARGO_BIN_EXE_arrix"))
        .arg("serve")
        .arg("--stdio")
        .arg(&doc)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocol":1}}}}"#
    )
    .unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let lines: Vec<Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["result"]["protocol"], 1);
    assert_eq!(lines[1]["params"]["kind"], "opened");
    assert!(lines[1]["params"]["files"].as_object().unwrap().len() > 2);
}
