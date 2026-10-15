//! Local mode: the authority on the session thread and the evaluator on
//! its worker, both through the executor, talking to clients over
//! channels (docs/CONCURRENCY-WASM.md §Roles and threads). The kernel never
//! runs on the caller's thread. A preview (ADR-0019) is the worker's too,
//! behind the newest commit.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use super::core::Core;
use super::plugins::{PluginLoader, Plugins};
use super::preview::Job;
use super::wire::{self, Heard};
use super::worker::EvalWorker;
use super::{EventStream, Request, Session, SessionEvent};
use crate::authority::{Authority, Outcome, Rejected};
use arrix_core::FeatureId;
use arrix_kernel::Interrupt;

use crate::document::Document;
use crate::eval::Evaluator;
use crate::executor::{self, Worker};
use crate::frozen::Frozen;
use crate::open::MemorySource;
use crate::registry::Registry;

type Reply = Sender<Result<Outcome, Rejected>>;

/// What the session thread reads, from clients and from its worker.
pub(super) enum Msg {
    Request(Request, Reply),
    Subscribe(Sender<SessionEvent>),
    /// An evaluation event from the worker, to publish.
    Publish(SessionEvent),
    /// The plugin features' results of an evaluation, from the worker,
    /// sent before that evaluation's `Finished`.
    Freeze(BTreeMap<FeatureId, Arc<Frozen>>),
    /// The document as it saves, with the current frozen results.
    Save(Sender<MemorySource>),
    Stop,
}

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
        Self::open_with(document, registry, None)
    }

    /// As [`open`](Self::open), with the loader that serves
    /// `Request::LoadPlugin` and `ReloadPlugin` (ADR-0024). Without one,
    /// both are `Rejected::NoPluginLoader`.
    pub fn open_with(
        document: Document,
        registry: Registry,
        loader: Option<Box<dyn PluginLoader>>,
    ) -> std::io::Result<Self> {
        let plugins = Plugins::new(registry.clone(), loader);
        let (inbox, rx) = channel();
        let (jobs, job_rx) = channel();
        let (handle, interrupt_rx) = channel();
        let publish = inbox.clone();
        let worker = executor::spawn("arrix-evaluator", move || {
            let evaluator = Evaluator::new(registry);
            let _ = handle.send(evaluator.interrupt());
            EvalWorker::new(evaluator).run(&job_rx, &publish);
        })?;
        let interrupt = interrupt_rx
            .recv()
            .expect("the evaluator thread hands out its interrupt first");
        let wired = wire::loopback();
        let thread = executor::spawn("arrix-session", move || {
            serve(
                Authority::new(document),
                plugins,
                &rx,
                jobs,
                worker,
                &interrupt,
                wired,
            );
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

    fn save(&self) -> MemorySource {
        let (reply, answer) = channel();
        self.inbox
            .send(Msg::Save(reply))
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

/// The session thread: steps the core with each message, posting the jobs
/// it makes to the worker in order. With `wired` (a test's loopback) every
/// job and every answer crosses through its serialised form.
fn serve(
    authority: Authority,
    plugins: Plugins,
    inbox: &Receiver<Msg>,
    jobs: Sender<Job>,
    worker: Worker,
    interrupt: &Interrupt,
    wired: bool,
) {
    let (mut core, first) = Core::new(authority, plugins);
    // The flag goes up before a snapshot is queued, so a worker that takes
    // the snapshot and lowers the flag never lowers a raise that is still
    // to be followed by its job.
    let post = |list: Vec<Job>| {
        for job in list {
            if matches!(job, Job::Commit(_)) {
                interrupt.raise();
            }
            let job = if wired {
                let text = wire::encode_job(&job).expect("a loopback session loads no plugin");
                wire::decode_job(&text).expect("a job decodes as it was encoded")
            } else {
                job
            };
            let _ = jobs.send(job);
        }
    };
    post(first);
    while let Ok(msg) = inbox.recv() {
        let msg = match msg {
            Msg::Publish(event) if wired => match decode(&wire::encode_publish(&event)) {
                Heard::Publish(event) => Msg::Publish(*event),
                Heard::Freeze(_) => unreachable!("a publish decodes as a publish"),
            },
            Msg::Freeze(results) if wired => match decode(&wire::encode_freeze(&results)) {
                Heard::Freeze(results) => Msg::Freeze(results),
                Heard::Publish(_) => unreachable!("a freeze decodes as a freeze"),
            },
            other => other,
        };
        match msg {
            Msg::Request(request, reply) => {
                let (result, list) = core.request(request);
                post(list);
                let _ = reply.send(result);
            }
            Msg::Subscribe(tx) => core.subscribe(tx),
            Msg::Publish(event) => core.publish(event),
            Msg::Freeze(results) => core.freeze(results),
            Msg::Save(reply) => {
                let _ = reply.send(core.save());
            }
            Msg::Stop => break,
        }
    }
    // The worker stops once its job channel is closed.
    drop(jobs);
    drop(worker);
}

fn decode(text: &str) -> Heard {
    wire::decode_heard(text).expect("an answer decodes as it was encoded")
}
