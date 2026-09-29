//! Local mode: the authority on the session thread and the evaluator on
//! its worker, both through the executor, talking to clients over
//! channels (docs/CONCURRENCY-WASM.md §Roles and threads). The kernel never
//! runs on the caller's thread.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use super::{DocHash, EventStream, Request, Session, SessionEvent, files};
use crate::authority::{Authority, Generation, Outcome, Rejected};
use crate::document::Document;
use crate::eval::Evaluator;
use crate::executor::{self, Worker};
use crate::registry::Registry;

type Reply = Sender<Result<Outcome, Rejected>>;

/// What the session thread reads, from clients and from its worker.
enum Msg {
    Request(Request, Reply),
    Subscribe(Sender<SessionEvent>),
    /// An evaluation event from the worker, to publish.
    Publish(SessionEvent),
    Stop,
}

type Snapshot = (Generation, Arc<Document>);

/// A session in this process. Dropping it stops both threads, the worker
/// after the feature it is evaluating.
pub struct LocalSession {
    inbox: Sender<Msg>,
    thread: Option<Worker>,
}

impl LocalSession {
    /// Starts the authority over `document`, at generation 0, and its
    /// evaluator with `registry`'s feature types. Fails where no thread
    /// can be spawned (wasm32 until C4).
    pub fn open(document: Document, registry: Registry) -> std::io::Result<Self> {
        let (inbox, rx) = channel();
        let (snapshots, snapshot_rx) = channel();
        let publish = inbox.clone();
        let worker = executor::spawn("arrix-evaluator", move || {
            evaluate(Evaluator::new(registry), &snapshot_rx, &publish);
        })?;
        let thread = executor::spawn("arrix-session", move || {
            serve(Authority::new(document), &rx, snapshots, worker);
        })?;
        Ok(Self {
            inbox,
            thread: Some(thread),
        })
    }
}

impl Session for LocalSession {
    fn request(&self, request: Request) -> Result<Outcome, Rejected> {
        let (reply, answer) = channel();
        self.inbox
            .send(Msg::Request(request, reply))
            .expect("the session thread runs until the session is dropped");
        answer
            .recv()
            .expect("the session thread answers every request")
    }

    fn subscribe(&self) -> EventStream {
        let (tx, rx) = channel();
        self.inbox
            .send(Msg::Subscribe(tx))
            .expect("the session thread runs until the session is dropped");
        rx
    }
}

impl Drop for LocalSession {
    fn drop(&mut self) {
        let _ = self.inbox.send(Msg::Stop);
        drop(self.thread.take());
    }
}

/// The session thread: applies requests in arrival order, publishes each
/// change, then hands the evaluator the new snapshot.
fn serve(
    mut authority: Authority,
    inbox: &Receiver<Msg>,
    snapshots: Sender<Snapshot>,
    worker: Worker,
) {
    let mut subscribers: Vec<Sender<SessionEvent>> = Vec::new();
    let snapshot = |a: &Authority| (a.generation(), Arc::new(a.document().clone()));
    let _ = snapshots.send(snapshot(&authority));
    while let Ok(msg) = inbox.recv() {
        match msg {
            Msg::Request(request, reply) => {
                let result = match request {
                    Request::Submit(envelope) => authority.submit(&envelope),
                    Request::Undo { author } => authority.undo(author),
                    Request::Redo { author } => authority.redo(author),
                };
                if let Ok(Outcome::Applied(change)) = &result {
                    let hash = cfg!(debug_assertions).then(|| DocHash::of(authority.document()));
                    let event = SessionEvent::Applied {
                        change: (**change).clone(),
                        hash,
                    };
                    subscribers.retain(|s| s.send(event.clone()).is_ok());
                    let _ = snapshots.send(snapshot(&authority));
                }
                let _ = reply.send(result);
            }
            Msg::Subscribe(tx) => {
                let opened = SessionEvent::Opened {
                    generation: authority.generation(),
                    files: files(authority.document()),
                };
                if tx.send(opened).is_ok() {
                    subscribers.push(tx);
                }
            }
            Msg::Publish(event) => subscribers.retain(|s| s.send(event.clone()).is_ok()),
            Msg::Stop => break,
        }
    }
    // The worker stops once its snapshot channel is closed.
    drop(snapshots);
    drop(worker);
}

/// The newest snapshot queued after `current`, or `current`; `None` once
/// the session thread has gone.
fn newest(snapshots: &Receiver<Snapshot>, mut current: Snapshot) -> Option<Snapshot> {
    loop {
        match snapshots.try_recv() {
            Ok(s) => current = s,
            Err(TryRecvError::Empty) => return Some(current),
            Err(TryRecvError::Disconnected) => return None,
        }
    }
}

/// The evaluator worker: evaluates the newest snapshot, publishing each
/// feature's event as it is made, and starts over on a newer one between
/// features. The cache makes starting over cheap.
fn evaluate(mut evaluator: Evaluator, snapshots: &Receiver<Snapshot>, publish: &Sender<Msg>) {
    let mut next = None;
    loop {
        let queued = match next.take() {
            Some(s) => s,
            None => match snapshots.recv() {
                Ok(s) => s,
                Err(_) => return,
            },
        };
        let Some((generation, doc)) = newest(snapshots, queued) else {
            return;
        };
        let mut gone = false;
        let done = evaluator.evaluate_while(&doc, |event| {
            let event = SessionEvent::Evaluated {
                generation,
                event: event.clone(),
            };
            let _ = publish.send(Msg::Publish(event));
            match snapshots.try_recv() {
                Ok(s) => next = Some(s),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => gone = true,
            }
            next.is_none() && !gone
        });
        if gone {
            return;
        }
        if let Some(evaluation) = done {
            let finished = SessionEvent::Finished {
                generation,
                params: evaluation.params,
            };
            let _ = publish.send(Msg::Publish(finished));
        }
    }
}
