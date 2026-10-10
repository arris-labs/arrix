//! Local mode: the authority on the session thread and the evaluator on
//! its worker, both through the executor, talking to clients over
//! channels (docs/CONCURRENCY-WASM.md §Roles and threads). The kernel never
//! runs on the caller's thread. A preview (ADR-0019) is the worker's too,
//! behind the newest commit.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use super::plugins::{PluginLoader, Plugins, RegistryChange};
use super::preview::{self, ExportJob, Job, PreviewJob, Queue};
use super::{DocHash, EventStream, PreviewTag, Request, Session, SessionEvent, files};
use crate::authority::{Authority, Generation, Outcome, Rejected};
use crate::eval::{EvalEvent, interrupted};
use arrix_core::{FeatureId, MeshDetail};
use arrix_kernel::Interrupt;

use crate::document::Document;
use crate::eval::{Evaluator, FeatureOutcome, InputHash};
use crate::executor::{self, Worker};
use crate::frozen::Frozen;
use crate::open::MemorySource;
use crate::registry::Registry;
use crate::save::save;

type Reply = Sender<Result<Outcome, Rejected>>;

/// What the session thread reads, from clients and from its worker.
enum Msg {
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

/// What the evaluation of the newest generation has said so far, which a
/// late subscriber is told after `Opened`: each feature's outcome, the
/// `Finished`, and the latest mesh of each body (refined replacing coarse).
#[derive(Default)]
struct Replay {
    generation: Option<Generation>,
    events: Vec<SessionEvent>,
}

impl Replay {
    fn record(&mut self, event: &SessionEvent) {
        let (SessionEvent::Evaluated { generation, .. }
        | SessionEvent::Finished { generation, .. }
        | SessionEvent::Meshed { generation, .. }) = event
        else {
            return;
        };
        match self.generation {
            Some(held) if held > *generation => return,
            Some(held) if held == *generation => {}
            _ => {
                self.generation = Some(*generation);
                self.events.clear();
            }
        }
        let same = |old: &SessionEvent| match (old, event) {
            (
                SessionEvent::Evaluated { event: a, .. },
                SessionEvent::Evaluated { event: b, .. },
            ) => a.feature == b.feature,
            (SessionEvent::Finished { .. }, SessionEvent::Finished { .. }) => true,
            (
                SessionEvent::Meshed { part, slot, .. },
                SessionEvent::Meshed {
                    part: p, slot: s, ..
                },
            ) => part == p && slot == s,
            _ => false,
        };
        match self.events.iter().position(same) {
            Some(at) => self.events[at] = event.clone(),
            None => self.events.push(event.clone()),
        }
    }

    /// The events of `generation`, in the order they were first heard.
    fn of(&self, generation: Generation) -> &[SessionEvent] {
        if self.generation == Some(generation) {
            &self.events
        } else {
            &[]
        }
    }
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
            evaluate(evaluator, &job_rx, &publish);
        })?;
        let interrupt = interrupt_rx
            .recv()
            .expect("the evaluator thread hands out its interrupt first");
        let thread = executor::spawn("arrix-session", move || {
            serve(
                Authority::new(document),
                plugins,
                &rx,
                jobs,
                worker,
                &interrupt,
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

/// The session thread: applies requests in arrival order, publishes each
/// change, then hands the evaluator the new snapshot. A preview's events
/// reach subscribers only while it is the session's preview: a newer
/// preview, a clear or an applied change ends it.
fn serve(
    mut authority: Authority,
    mut plugins: Plugins,
    inbox: &Receiver<Msg>,
    jobs: Sender<Job>,
    worker: Worker,
    interrupt: &Interrupt,
) {
    let mut subscribers: Vec<Sender<SessionEvent>> = Vec::new();
    // ADR-0009: the latest result of each plugin feature, and the input
    // hash each feature last evaluated under, with its generation.
    let mut kept: BTreeMap<FeatureId, Arc<Frozen>> = BTreeMap::new();
    let mut hashes: BTreeMap<FeatureId, (Generation, InputHash)> = BTreeMap::new();
    let mut previewing: Option<PreviewTag> = None;
    let mut replay = Replay::default();
    // The flag goes up before the snapshot is queued, so a worker that
    // takes the snapshot and lowers the flag never lowers a raise that
    // is still to be followed by its job.
    let commit = |a: &Authority| {
        interrupt.raise();
        let _ = jobs.send(Job::Commit((
            a.generation(),
            Arc::new(a.document().clone()),
        )));
    };
    commit(&authority);
    while let Ok(msg) = inbox.recv() {
        match msg {
            Msg::Request(request, reply) => {
                let result = match request {
                    Request::Submit(envelope) => authority.submit(&envelope),
                    Request::Undo { author } => authority.undo(author),
                    Request::Redo { author } => authority.redo(author),
                    Request::Preview { preview, envelope } => {
                        authority.speculate(&envelope).map(|doc| {
                            let tag = PreviewTag {
                                author: envelope.author,
                                preview,
                            };
                            previewing = Some(tag);
                            let _ = jobs.send(Job::Preview(PreviewJob {
                                tag,
                                generation: authority.generation(),
                                doc: Arc::new(doc),
                            }));
                            Outcome::Previewing
                        })
                    }
                    Request::Export { bodies, format } => {
                        let _ = jobs.send(Job::Export(ExportJob { bodies, format }));
                        Ok(Outcome::Exporting)
                    }
                    Request::LoadPlugin { dir } => plugins.load(&dir).map(|change| {
                        let out = announce(change, &mut authority, &plugins, &jobs);
                        previewing = None;
                        subscribers.retain(|s| s.send(out.1.clone()).is_ok());
                        commit(&authority);
                        out.0
                    }),
                    Request::ReloadPlugin { plugin } => plugins.reload(&plugin).map(|change| {
                        let out = announce(change, &mut authority, &plugins, &jobs);
                        previewing = None;
                        subscribers.retain(|s| s.send(out.1.clone()).is_ok());
                        commit(&authority);
                        out.0
                    }),
                    Request::ClearPreview { author } => {
                        if previewing.is_some_and(|t| t.author == author) {
                            previewing = None;
                            let _ = jobs.send(Job::ClearPreview);
                        }
                        Ok(Outcome::NoOp)
                    }
                };
                if let Ok(Outcome::Applied(change)) = &result {
                    let hash = cfg!(debug_assertions).then(|| DocHash::of(authority.document()));
                    let event = SessionEvent::Applied {
                        change: (**change).clone(),
                        hash,
                    };
                    subscribers.retain(|s| s.send(event.clone()).is_ok());
                    previewing = None;
                    commit(&authority);
                }
                let _ = reply.send(result);
            }
            Msg::Subscribe(tx) => {
                let (files, blobs) = files(authority.document());
                let opened = SessionEvent::Opened {
                    generation: authority.generation(),
                    files,
                    blobs,
                    types: plugins.loaded_specs(),
                };
                // The newest generation's outcomes follow, so that a client
                // that joins after the evaluation has spoken still hears it.
                let told = tx.send(opened).is_ok()
                    && replay
                        .of(authority.generation())
                        .iter()
                        .all(|e| tx.send(e.clone()).is_ok());
                if told {
                    subscribers.push(tx);
                }
            }
            Msg::Publish(event) => {
                match &event {
                    SessionEvent::Evaluated {
                        generation,
                        event:
                            EvalEvent {
                                feature,
                                outcome: FeatureOutcome::Ok { hash, .. },
                            },
                    } => {
                        hashes.insert(*feature, (*generation, *hash));
                    }
                    SessionEvent::Previewed { tag, .. }
                    | SessionEvent::PreviewMeshed { tag, .. }
                        if previewing != Some(*tag) =>
                    {
                        continue;
                    }
                    _ => {}
                }
                replay.record(&event);
                subscribers.retain(|s| s.send(event.clone()).is_ok());
            }
            Msg::Freeze(results) => kept.extend(results),
            Msg::Save(reply) => {
                // A kept result is written when it is of the input hash the
                // feature has now; otherwise the document keeps what it
                // was opened with.
                let mut doc = authority.document().clone();
                let now = authority.generation();
                for (id, frozen) in &kept {
                    if hashes.get(id) == Some(&(now, frozen.result.input_hash)) {
                        doc.freeze(*id, frozen);
                    }
                }
                let _ = reply.send(save(&doc));
            }
            Msg::Stop => break,
        }
    }
    // The worker stops once its job channel is closed.
    drop(jobs);
    drop(worker);
}

/// Makes the generation of a registry change (ADR-0024) and hands the
/// worker its registry, ahead of the commit that follows: the outcome to
/// reply and the `Registry` event to publish.
fn announce(
    change: RegistryChange,
    authority: &mut Authority,
    plugins: &Plugins,
    jobs: &Sender<Job>,
) -> (Outcome, SessionEvent) {
    let generation = authority.advance();
    let event = SessionEvent::Registry {
        generation,
        added: change.added,
        removed: change.removed,
    };
    let _ = jobs.send(Job::Registry(
        Box::new(plugins.registry().clone()),
        change.stale,
    ));
    (Outcome::Loaded { generation }, event)
}

/// The evaluator worker: evaluates the newest commit's snapshot,
/// publishing each feature's event as it is made, and starts over on a
/// newer one between features; the cache makes starting over cheap. A
/// preview waits until the newest commit is evaluated and meshed, and
/// gives way to anything newer (ADR-0019).
fn evaluate(mut evaluator: Evaluator, jobs: &Receiver<Job>, publish: &Sender<Msg>) {
    let mut queue = Queue::new(jobs);
    // The final bodies of the newest commit's finished evaluation, which a
    // preview's bodies are told changed or not against.
    let mut committed = Vec::new();
    let interrupt = evaluator.interrupt();
    // The newest commit evaluated to its end, which an export writes, and
    // whether the evaluator still holds that evaluation (a preview ran
    // since if not).
    let mut finished: Option<(Generation, Arc<Document>)> = None;
    let mut held = false;
    while queue.wait(finished.is_some()) {
        // Whatever raised the flag is the job taken now, or an older one.
        interrupt.lower();
        if let Some((registry, stale)) = queue.registry.take() {
            evaluator.set_registry(*registry, &stale);
            // What the evaluator held was of the old registry.
            held = false;
        }
        if let Some((generation, doc)) = queue.commit.take() {
            let done = evaluator.evaluate_while(&doc, |event| {
                let event = SessionEvent::Evaluated {
                    generation,
                    event: event.clone(),
                };
                let _ = publish.send(Msg::Publish(event));
                queue.poll();
                queue.commit.is_none() && !queue.gone
            });
            if queue.gone {
                return;
            }
            let _ = publish.send(Msg::Freeze(evaluator.frozen().clone()));
            held = done.is_some();
            if let Some(evaluation) = done {
                finished = Some((generation, doc));
                committed = evaluator.final_bodies().to_vec();
                let finished = SessionEvent::Finished {
                    generation,
                    params: evaluation.params,
                };
                let _ = publish.send(Msg::Publish(finished));
                mesh_finals(&mut evaluator, generation, &mut queue, publish);
            }
        } else if let (false, Some((generation, doc))) = (queue.exports.is_empty(), &finished) {
            export_queued(
                &mut evaluator,
                (*generation, doc),
                &mut held,
                &mut queue,
                publish,
            );
        } else if let Some(job) = queue.preview.take() {
            held = false;
            let mut send = |event| {
                let _ = publish.send(Msg::Publish(event));
            };
            preview::run(&mut evaluator, job, &committed, &mut queue, &mut send);
        }
    }
}

/// Answers the exports queued, oldest first, from the newest finished
/// commit `(generation, doc)`: evaluated again first (the cache makes it
/// cheap) when a preview has run since and `held` is false. An export
/// stopped by a newer commit stays queued, for that commit's evaluation.
fn export_queued(
    evaluator: &mut Evaluator,
    (generation, doc): (Generation, &Arc<Document>),
    held: &mut bool,
    queue: &mut Queue,
    publish: &Sender<Msg>,
) {
    while let Some(job) = queue.exports.front() {
        if !*held {
            if evaluator.evaluate_while(doc, |_| true).is_none() {
                return;
            }
            *held = true;
        }
        let bytes = evaluator.export(&job.bodies, job.format);
        if matches!(&bytes, Err(d) if interrupted(d)) {
            return;
        }
        let event = SessionEvent::Exported {
            generation,
            format: job.format,
            bytes,
        };
        let _ = publish.send(Msg::Publish(event));
        queue.exports.pop_front();
    }
}

/// Meshes the final bodies of the evaluation of `generation`, every body
/// coarse and then every body refined, publishing each as it is made.
/// Stops between meshes when a newer commit is queued, or the session
/// thread has gone.
fn mesh_finals(
    evaluator: &mut Evaluator,
    generation: Generation,
    queue: &mut Queue,
    publish: &Sender<Msg>,
) {
    let finals = evaluator.final_bodies().to_vec();
    for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
        for body in &finals {
            queue.poll();
            if queue.commit.is_some() || queue.gone {
                return;
            }
            let mesh = evaluator.mesh(body, detail);
            if matches!(&mesh, Err(d) if interrupted(d)) {
                return;
            }
            let event = SessionEvent::Meshed {
                generation,
                part: body.part,
                slot: body.slot.clone(),
                detail,
                mesh,
            };
            let _ = publish.send(Msg::Publish(event));
        }
    }
}
