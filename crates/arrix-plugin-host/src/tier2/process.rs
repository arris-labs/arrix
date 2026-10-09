//! A Tier 2 plugin's child process: its pipes, the reader thread that turns
//! its stdout into lines, and one call to it with the plugin's own calls
//! served while it is open (ADR-0022 §2).

use std::io::{BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::thread;
use std::time::Duration;

use arrix_core::Diagnostic;
use serde_json::Value;

use crate::wire::rpc::{LineError, LineReader, Message, Request, RequestId, Response, RpcError};

/// How a call to the plugin ended other than with its answer.
#[derive(Debug)]
pub(crate) enum CallError {
    /// The process ended, or its stdout closed, before answering.
    Crash(String),
    /// Nothing came from it for the call's timeout.
    Timeout(Duration),
    /// A line that is no message of the protocol, an answer to no call, or
    /// a protocol error in the answer.
    Protocol(String),
    /// The plugin asked for something its manifest does not allow.
    Capability(String),
    /// The host's own kernel was stopped by the evaluator's interrupt while
    /// serving the plugin: the call ends there, and is the evaluator's
    /// stop, not the plugin's failure.
    Interrupted(Diagnostic),
}

/// What the host does with a request the plugin made during a call.
pub(crate) enum Served {
    /// Answers it, with a result or the protocol's error.
    Answer(Result<Value, RpcError>),
    /// Ends the call, the plugin's process with it.
    End(CallError),
}

pub(crate) struct Process {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<Result<Option<String>, LineError>>,
    next_id: i64,
}

impl Process {
    /// Starts `command` in `dir`, its stderr the host's.
    pub(crate) fn spawn(command: &[String], dir: &Path) -> std::io::Result<Process> {
        let (program, args) = command
            .split_first()
            .expect("a parsed tier 2 manifest has a command");
        let mut child = Command::new(program)
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, lines) = channel();
        // The one thread the plugin host spawns, native only
        // (docs/CONCURRENCY-WASM.md §Roles and threads): a blocking read
        // that a timeout can stop waiting on. It ends with the pipe.
        thread::Builder::new()
            .name(format!("tier2 {program}"))
            .spawn(move || {
                let mut reader = LineReader::new(BufReader::new(stdout));
                loop {
                    let line = reader.next_line();
                    let last = matches!(line, Ok(None) | Err(LineError::Io(_)));
                    if tx.send(line).is_err() || last {
                        break;
                    }
                }
            })?;
        Ok(Process {
            child,
            stdin,
            lines,
            next_id: 1,
        })
    }

    fn send(&mut self, message: &Message) -> Result<(), CallError> {
        let mut line = message.to_line();
        line.push('\n');
        let stdin = self
            .stdin
            .as_mut()
            .expect("stdin is open while the process is");
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| self.crashed(&format!("its stdin closed ({e})")))
    }

    /// Why the process is gone, with its exit status when it has one.
    fn crashed(&mut self, what: &str) -> CallError {
        // The pipe closes as the process exits; its status follows shortly.
        let status = (0..50).find_map(|_| match self.child.try_wait() {
            Ok(Some(status)) => Some(status.to_string()),
            Ok(None) => {
                thread::sleep(Duration::from_millis(2));
                None
            }
            Err(_) => Some("its status unknown".into()),
        });
        match status {
            Some(status) => CallError::Crash(format!("{what}, {status}")),
            None => CallError::Crash(what.to_owned()),
        }
    }

    /// Calls `method` and returns its result, answering each request the
    /// plugin makes meanwhile with `serve`. `timeout` bounds each wait for
    /// the plugin's next line, not the call: the kernel's time is the
    /// host's.
    pub(crate) fn call(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
        serve: &mut dyn FnMut(&Request) -> Served,
    ) -> Result<Value, CallError> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&Message::request(id, method, params))?;
        loop {
            let line = match self.lines.recv_timeout(timeout) {
                Ok(Ok(Some(line))) => line,
                Ok(Ok(None)) | Ok(Err(LineError::Io(_))) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(self.crashed(&format!("it ended during `{method}`")));
                }
                Ok(Err(e)) => {
                    let m = e.malformed().expect("only an io error has no answer");
                    return Err(CallError::Protocol(m.error.message));
                }
                Err(RecvTimeoutError::Timeout) => return Err(CallError::Timeout(timeout)),
            };
            match Message::parse(&line) {
                Err(m) => return Err(CallError::Protocol(m.error.message)),
                Ok(Message::Request(request)) => {
                    let outcome = match serve(&request) {
                        Served::Answer(outcome) => outcome,
                        Served::End(e) => return Err(e),
                    };
                    // A notification gets no answer.
                    if let Some(id) = request.id {
                        self.send(&Message::Response(Response {
                            id: Some(id),
                            outcome,
                        }))?;
                    }
                }
                Ok(Message::Response(r)) if r.id == Some(RequestId::Number(id)) => {
                    return r
                        .outcome
                        .map_err(|e| CallError::Protocol(format!("`{method}` was refused: {e}")));
                }
                Ok(Message::Response(r)) => {
                    return Err(CallError::Protocol(format!(
                        "an answer to {:?}, which was not asked",
                        r.id
                    )));
                }
            }
        }
    }
}

impl Drop for Process {
    /// Closes its stdin, which ends a well-behaved plugin, and kills it
    /// anyway: it holds nothing worth a graceful exit (its features are
    /// pure functions).
    fn drop(&mut self) {
        drop(self.stdin.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
