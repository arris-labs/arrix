//! A client of `arrix serve --stdio` as an agent would be: the spawned
//! binary, a line a request, every notification checked against the
//! schemas `serve --schema` prints.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command as Process, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use std::collections::BTreeMap;

use arrix_doc::{AuthorId, Command, CommandEnvelope, Generation, MemorySource, Part, open};
use serde_json::{Value, json};

pub struct Serve {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    /// Every line heard so far, in order: responses and notifications.
    pub heard: Vec<Value>,
    next_id: i64,
    event_schema: jsonschema::Validator,
}

impl Serve {
    /// Spawned, not yet initialized.
    pub fn spawn() -> Self {
        Self::spawn_with(&[])
    }

    /// Spawned with `args` after `serve --stdio`, not yet initialized.
    pub fn spawn_with(args: &[&str]) -> Self {
        let mut child = Process::new(env!("CARGO_BIN_EXE_arrix"))
            .args(["serve", "--stdio"])
            .args(args)
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
    pub fn start() -> Self {
        let mut s = Self::spawn();
        let hello = s.call("initialize", json!({"protocol": 1}));
        assert_eq!(hello["result"]["protocol"], 1, "{hello}");
        s
    }

    pub fn write(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").unwrap();
        self.stdin.flush().unwrap();
    }

    pub fn read(&mut self) -> Value {
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
    pub fn call(&mut self, method: &str, params: Value) -> Value {
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
    pub fn session(&mut self, method: &str, params: Value) -> Value {
        let response = self.call(&format!("session.{method}"), params);
        assert!(response.get("error").is_none(), "{response}");
        response["result"].clone()
    }

    pub fn submit(&mut self, author: AuthorId, base: u64, command: Command) -> Value {
        let envelope = CommandEnvelope {
            author,
            base: Generation(base),
            command,
        };
        self.session("submit", serde_json::to_value(envelope).unwrap())
    }

    /// Hears until some notification satisfies `done`.
    pub fn hear_event(&mut self, done: impl Fn(&Value) -> bool) -> Value {
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

    pub fn events(&self) -> Vec<Value> {
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

/// `tests/docs/standoff` as the commands that build it: its parameters, its
/// part, then its first `features` features (the plate's sketch and
/// extrusion, the standoff, the fillet).
pub fn standoff_commands(features: usize) -> Vec<Command> {
    let mut files = BTreeMap::new();
    crate::common::read_tree(
        &crate::common::workspace().join("tests/docs/standoff"),
        "",
        &mut files,
    );
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
    for (at, id) in part.history.iter().take(features).enumerate() {
        commands.push(Command::AddFeature {
            part: part.id,
            at,
            record: part.features[id].clone(),
        });
    }
    commands
}
