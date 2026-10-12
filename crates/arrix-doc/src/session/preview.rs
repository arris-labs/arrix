//! Previews (ADR-0019): the document evaluated with a command applied that
//! is never applied to it. A preview is session state, not a command: it
//! makes no generation and no undo entry, and its events carry the
//! client's own counter so a superseded preview's late events are told
//! apart. Local mode evaluates it on the evaluator's worker, behind the
//! newest commit and never in its way (docs/CONCURRENCY-WASM.md §The
//! evaluator).

use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use arrix_core::{ExportFormat, MeshDetail, PartId};
use serde::{Deserialize, Serialize};

use super::SessionEvent;
use crate::authority::{AuthorId, Generation};
use crate::document::Document;
use crate::document::FeatureTypeId;
use crate::eval::{BodySlot, Evaluator, FinalBody, interrupted};
use crate::registry::Registry;

/// A client's name for one of its previews, counted by the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(transparent)]
pub struct PreviewId(pub u64);

/// Whose preview an event is of, and which.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PreviewTag {
    pub author: AuthorId,
    pub preview: PreviewId,
}

/// A final body of a preview: `changed` when its version is not the one
/// the generation it was taken on ends with, so a `PreviewMeshed` follows.
/// A body of the generation missing from the list is one the preview
/// consumes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PreviewBody {
    pub part: PartId,
    pub slot: BodySlot,
    pub changed: bool,
}

pub(super) type Snapshot = (Generation, Arc<Document>);

/// A preview as the worker takes it: the speculated document.
pub(super) struct PreviewJob {
    pub tag: PreviewTag,
    pub generation: Generation,
    pub doc: Arc<Document>,
}

/// An export as the worker takes it.
pub(super) struct ExportJob {
    pub bodies: Vec<BodySlot>,
    pub format: ExportFormat,
}

/// A STEP file's tree to read, as the worker takes it.
pub(super) struct StructureJob {
    pub tag: u64,
    pub bytes: Vec<u8>,
}

/// What the session thread hands its worker.
pub(super) enum Job {
    Commit(Snapshot),
    /// The registry after a plugin load or reload (ADR-0024), with the
    /// types whose cache entries to drop. Always followed by the commit
    /// of the generation it made.
    Registry(Box<Registry>, BTreeSet<FeatureTypeId>),
    Preview(PreviewJob),
    ClearPreview,
    Export(ExportJob),
    Structure(StructureJob),
}

/// The worker's queue: the newest commit and the newest preview not yet
/// taken. A commit drops a waiting preview, which was taken on an older
/// document; a newer preview or a clear replaces it.
pub(super) struct Queue<'a> {
    rx: &'a Receiver<Job>,
    pub commit: Option<Snapshot>,
    /// The newest registry not yet taken, and the types stale in any of
    /// those it replaced.
    pub registry: Option<(Box<Registry>, BTreeSet<FeatureTypeId>)>,
    pub preview: Option<PreviewJob>,
    /// A preview or a clear arrived since the flag was last reset.
    pub preview_moved: bool,
    /// The exports asked for, oldest first: every one is answered, so a
    /// commit drops none.
    pub exports: VecDeque<ExportJob>,
    /// The files whose structure is asked for, oldest first.
    pub structures: VecDeque<StructureJob>,
    pub gone: bool,
}

impl<'a> Queue<'a> {
    pub fn new(rx: &'a Receiver<Job>) -> Self {
        Self {
            rx,
            commit: None,
            registry: None,
            preview: None,
            preview_moved: false,
            exports: VecDeque::new(),
            structures: VecDeque::new(),
            gone: false,
        }
    }

    fn take(&mut self, job: Job) {
        match job {
            Job::Commit(s) => {
                self.commit = Some(s);
                self.preview = None;
            }
            Job::Registry(registry, stale) => {
                let mut all = self.registry.take().map(|(_, s)| s).unwrap_or_default();
                all.extend(stale);
                self.registry = Some((registry, all));
            }
            Job::Preview(p) => {
                self.preview = Some(p);
                self.preview_moved = true;
            }
            Job::ClearPreview => {
                self.preview = None;
                self.preview_moved = true;
            }
            Job::Export(job) => self.exports.push_back(job),
            Job::Structure(job) => self.structures.push_back(job),
        }
    }

    /// Takes every job queued, without waiting.
    pub fn poll(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(job) => self.take(job),
                Err(TryRecvError::Empty) => return,
                Err(TryRecvError::Disconnected) => {
                    self.gone = true;
                    return;
                }
            }
        }
    }

    /// Waits until there is work, then takes everything queued; `false`
    /// once the session thread has gone. Exports are work only when
    /// `exports_ready`: a commit has been evaluated for them to write.
    pub fn wait(&mut self, exports_ready: bool) -> bool {
        let exports = exports_ready && !self.exports.is_empty();
        if self.commit.is_none() && self.preview.is_none() && !exports && self.structures.is_empty()
        {
            match self.rx.recv() {
                Ok(job) => self.take(job),
                Err(_) => self.gone = true,
            }
        }
        self.poll();
        !self.gone
    }

    /// Whether a running preview is superseded: by a commit, a newer
    /// preview or a clear.
    fn stops_preview(&mut self) -> bool {
        self.poll();
        self.gone || self.commit.is_some() || self.preview_moved
    }
}

/// Evaluates `job` on `evaluator` and publishes its `Previewed` and the
/// meshes of the bodies it changed against `committed`, the final bodies
/// of the newest commit's evaluation. Stops between features and between
/// meshes once anything newer is queued; what it computed stays cached.
pub(super) fn run(
    evaluator: &mut Evaluator,
    job: PreviewJob,
    committed: &[FinalBody],
    queue: &mut Queue,
    publish: &mut impl FnMut(SessionEvent),
) {
    queue.preview_moved = false;
    let Some(evaluation) = evaluator.evaluate_while(&job.doc, |_| !queue.stops_preview()) else {
        return;
    };
    let finals = evaluator.final_bodies().to_vec();
    let bodies = finals
        .iter()
        .map(|b| PreviewBody {
            part: b.part,
            slot: b.slot.clone(),
            changed: !committed.contains(b),
        })
        .collect();
    publish(SessionEvent::Previewed {
        tag: job.tag,
        generation: job.generation,
        events: evaluation.events,
        params: evaluation.params,
        bodies,
    });
    let changed: Vec<&FinalBody> = finals.iter().filter(|b| !committed.contains(b)).collect();
    for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
        for body in &changed {
            if queue.stops_preview() {
                return;
            }
            let mesh = evaluator.mesh(body, detail);
            if matches!(&mesh, Err(d) if interrupted(d)) {
                return;
            }
            publish(SessionEvent::PreviewMeshed {
                tag: job.tag,
                part: body.part,
                slot: body.slot.clone(),
                detail,
                mesh,
            });
        }
    }
}
