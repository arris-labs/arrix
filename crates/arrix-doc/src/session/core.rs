//! The session's authority side, kernel-free: the requests it applies, the
//! events it publishes, the results it keeps to save, and the jobs it hands
//! the evaluator. It is stepped one message at a time and owns no thread
//! and no channel to the worker, so the same core serves the session
//! thread natively and the main thread in a browser (ADR-0042).

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::mpsc::Sender;

use super::plugins::{Plugins, RegistryChange};
use super::preview::{ExpandJob, ExportJob, Job, PreviewJob, StructureJob};
use super::{DocHash, PreviewTag, Request, SessionEvent, files};
use crate::authority::{Authority, Generation, Outcome, Rejected};
use crate::eval::{EvalEvent, FeatureOutcome, InputHash};
use crate::frozen::Frozen;
use crate::open::MemorySource;
use crate::save::save;
use arrix_core::FeatureId;

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
        | SessionEvent::Assembled { generation, .. }
        | SessionEvent::Massed { generation, .. }
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
            (
                SessionEvent::Assembled { assembly: a, .. },
                SessionEvent::Assembled { assembly: b, .. },
            ) => a == b,
            (SessionEvent::Massed { .. }, SessionEvent::Massed { .. }) => true,
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

/// The authority, its plugins and what it has heard. A preview's events
/// reach subscribers only while it is the session's preview: a newer
/// preview, a clear or an applied change ends it.
pub(super) struct Core {
    authority: Authority,
    plugins: Plugins,
    subscribers: Vec<Sender<SessionEvent>>,
    /// ADR-0009: the latest result of each plugin feature, and the input
    /// hash each feature last evaluated under, with its generation.
    kept: BTreeMap<FeatureId, Arc<Frozen>>,
    hashes: BTreeMap<FeatureId, (Generation, InputHash)>,
    previewing: Option<PreviewTag>,
    replay: Replay,
}

impl Core {
    /// The core at `authority`'s generation, with the commit job of that
    /// generation for the evaluator.
    pub fn new(authority: Authority, plugins: Plugins) -> (Self, Vec<Job>) {
        let core = Self {
            authority,
            plugins,
            subscribers: Vec::new(),
            kept: BTreeMap::new(),
            hashes: BTreeMap::new(),
            previewing: None,
            replay: Replay::default(),
        };
        let first = core.commit();
        (core, vec![first])
    }

    /// The snapshot job of the current generation. The embedder raises its
    /// interrupt before it posts one, so a worker that takes the snapshot
    /// never lowers a raise still to be followed by its job.
    fn commit(&self) -> Job {
        Job::Commit((
            self.authority.generation(),
            Arc::new(self.authority.document().clone()),
        ))
    }

    /// Applies `request`: the answer, and the jobs for the evaluator in
    /// the order to post them.
    pub fn request(&mut self, request: Request) -> (Result<Outcome, Rejected>, Vec<Job>) {
        let mut jobs = Vec::new();
        let result = match request {
            Request::Submit(envelope) => self.authority.submit(&envelope),
            Request::Undo { author } => self.authority.undo(author),
            Request::Redo { author } => self.authority.redo(author),
            Request::Preview { preview, envelope } => {
                self.authority.speculate(&envelope).map(|doc| {
                    let tag = PreviewTag {
                        author: envelope.author,
                        preview,
                    };
                    self.previewing = Some(tag);
                    jobs.push(Job::Preview(PreviewJob {
                        tag,
                        generation: self.authority.generation(),
                        doc: Arc::new(doc),
                    }));
                    Outcome::Previewing
                })
            }
            Request::Export {
                bodies,
                format,
                assembly,
                exporter,
            } => {
                jobs.push(Job::Export(ExportJob {
                    bodies,
                    format,
                    assembly,
                    exporter,
                }));
                Ok(Outcome::Exporting)
            }
            Request::Expand(request) => {
                if request.base < self.authority.generation() {
                    Err(Rejected::Stale {
                        base: request.base,
                        nodes: Vec::new(),
                    })
                } else {
                    jobs.push(Job::Expand(ExpandJob { request }));
                    Ok(Outcome::Expanding)
                }
            }
            Request::ReadStepTree { tag, bytes } => {
                jobs.push(Job::Structure(StructureJob { tag, bytes }));
                Ok(Outcome::Reading)
            }
            Request::LoadPlugin { dir } => self
                .plugins
                .load(&dir)
                .map(|change| self.announce(change, &mut jobs)),
            Request::ReloadPlugin { plugin } => self
                .plugins
                .reload(&plugin)
                .map(|change| self.announce(change, &mut jobs)),
            Request::ClearPreview { author } => {
                if self.previewing.is_some_and(|t| t.author == author) {
                    self.previewing = None;
                    jobs.push(Job::ClearPreview);
                }
                Ok(Outcome::NoOp)
            }
        };
        if let Ok(Outcome::Applied(change)) = &result {
            let hash = cfg!(debug_assertions).then(|| DocHash::of(self.authority.document()));
            let event = SessionEvent::Applied {
                change: (**change).clone(),
                hash,
            };
            self.send(&event);
            self.previewing = None;
            jobs.push(self.commit());
        }
        (result, jobs)
    }

    /// Makes the generation of a registry change (ADR-0024), hands the
    /// worker its registry ahead of the commit that follows, and publishes
    /// the `Registry` event: the outcome to reply.
    fn announce(&mut self, change: RegistryChange, jobs: &mut Vec<Job>) -> Outcome {
        let generation = self.authority.advance();
        let event = SessionEvent::Registry {
            generation,
            added: change.added,
            removed: change.removed,
        };
        jobs.push(Job::Registry(
            Box::new(self.plugins.registry().clone()),
            change.stale,
        ));
        self.previewing = None;
        self.send(&event);
        jobs.push(self.commit());
        Outcome::Loaded { generation }
    }

    fn send(&mut self, event: &SessionEvent) {
        self.subscribers.retain(|s| s.send(event.clone()).is_ok());
    }

    pub fn subscribe(&mut self, tx: Sender<SessionEvent>) {
        let (files, blobs) = files(self.authority.document());
        let opened = SessionEvent::Opened {
            generation: self.authority.generation(),
            files,
            blobs,
            types: self.plugins.loaded_specs(),
        };
        // The newest generation's outcomes follow, so that a client that
        // joins after the evaluation has spoken still hears it.
        let told = tx.send(opened).is_ok()
            && self
                .replay
                .of(self.authority.generation())
                .iter()
                .all(|e| tx.send(e.clone()).is_ok());
        if told {
            self.subscribers.push(tx);
        }
    }

    /// An event from the evaluator, to publish.
    pub fn publish(&mut self, event: SessionEvent) {
        match &event {
            SessionEvent::Evaluated {
                generation,
                event:
                    EvalEvent {
                        feature,
                        outcome: FeatureOutcome::Ok { hash, .. },
                    },
            } => {
                self.hashes.insert(*feature, (*generation, *hash));
            }
            SessionEvent::Previewed { tag, .. } | SessionEvent::PreviewMeshed { tag, .. }
                if self.previewing != Some(*tag) =>
            {
                return;
            }
            _ => {}
        }
        self.replay.record(&event);
        self.send(&event);
    }

    /// The plugin features' results of an evaluation, sent before that
    /// evaluation's `Finished`.
    pub fn freeze(&mut self, results: BTreeMap<FeatureId, Arc<Frozen>>) {
        self.kept.extend(results);
    }

    /// The document as it saves. A kept result is written when it is of the
    /// input hash the feature has now; otherwise the document keeps what it
    /// was opened with.
    pub fn save(&self) -> MemorySource {
        let mut doc = self.authority.document().clone();
        let now = self.authority.generation();
        for (id, frozen) in &self.kept {
            if self.hashes.get(id) == Some(&(now, frozen.result.input_hash)) {
                doc.freeze(*id, frozen);
            }
        }
        save(&doc)
    }
}
