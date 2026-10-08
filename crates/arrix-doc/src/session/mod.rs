//! The protocol boundary (docs/ARCHITECTURE.md §The protocol boundary):
//! what a client asks of the authority and what it hears back, as plain,
//! serialisable data even in one process, so a server with thin clients
//! stays possible. A client keeps a [`Replica`] of the document from the
//! applied stream; it never reads the authority's.
//!
//! A client mints the ids of the records its commands create
//! (docs/DATA-MODEL.md §Identifiers) with an `IdMinter` seeded by its
//! caller; nothing here mints, so nothing here needs entropy.

#[cfg(test)]
mod interrupt_tests;
mod local;
mod preview;
#[cfg(test)]
mod preview_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::mpsc::Receiver;

use arrix_core::{Diagnostic, FeatureId, MeshDetail, ParamId, PartId, Quantity, RenderMesh};
use serde::{Deserialize, Serialize};

pub use local::LocalSession;

use crate::authority::{AuthorId, Change, CommandEnvelope, Generation, Outcome, Rejected};
use crate::command::{CommandError, apply};
use crate::document::Document;
use crate::eval::{BodySlot, EvalEvent, FeatureOutcome};
use crate::open::{MemorySource, open};
use crate::save::save;
pub use preview::{PreviewBody, PreviewId, PreviewTag};

/// What a client asks of the authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    Submit(CommandEnvelope),
    Undo {
        author: AuthorId,
    },
    Redo {
        author: AuthorId,
    },
    /// Evaluates the document with the envelope's command applied, never
    /// applying it (ADR-0019): refused or stale as its submit would be, and
    /// otherwise answered `Previewing`, its events following. It supersedes
    /// the session's previous preview, whoever took it.
    Preview {
        preview: PreviewId,
        envelope: CommandEnvelope,
    },
    /// Drops `author`'s preview, if it is the session's: its events stop.
    ClearPreview {
        author: AuthorId,
    },
}

/// BLAKE3 over a document's saved files, as 64 hex digits: equal
/// documents, equal hashes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocHash(String);

impl DocHash {
    pub fn of(doc: &Document) -> Self {
        let mut h = blake3::Hasher::new();
        for (path, bytes) in &save(doc).0 {
            for field in [path.as_bytes(), bytes] {
                h.update(&(field.len() as u64).to_le_bytes());
                h.update(field);
            }
        }
        Self(h.finalize().to_hex().to_string())
    }
}

impl fmt::Debug for DocHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "DocHash({})", self.0)
    }
}

/// Every parameter's value in SI, or why it has none.
pub type ParamValues = BTreeMap<ParamId, Result<Quantity, Diagnostic>>;

/// What a subscriber hears, in the order the authority made it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionEvent {
    /// The first event of every subscription: the document at
    /// `generation`, as its directory form's files: the JSON ones as text,
    /// and the frozen results' blobs (`blobs/<name>`) as bytes.
    Opened {
        generation: Generation,
        files: BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        blobs: BTreeMap<String, Vec<u8>>,
    },
    /// A change the authority applied, with the document's hash after it
    /// in a debug build, for the replicas to check theirs against.
    Applied {
        change: Change,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hash: Option<DocHash>,
    },
    /// One feature's outcome in the evaluation of `generation`.
    Evaluated {
        generation: Generation,
        event: EvalEvent,
    },
    /// The evaluation of `generation` finished, with every parameter's
    /// value. An evaluation a newer generation stopped has no `Finished`.
    Finished {
        generation: Generation,
        params: ParamValues,
    },
    /// A final body of `generation`'s evaluation, meshed (docs/
    /// CONCURRENCY-WASM.md §Tessellation): every body coarse first, then
    /// every body refined. A mesh of a superseded generation is never
    /// drawn, and a body that could not be meshed says why. Not part of
    /// the document: a replica ignores it.
    Meshed {
        generation: Generation,
        part: PartId,
        slot: BodySlot,
        detail: MeshDetail,
        mesh: Result<RenderMesh, Diagnostic>,
    },
    /// A preview's evaluation (ADR-0019): the document at `generation` with
    /// the preview's command applied. Every feature's outcome, every
    /// parameter's value, and the final bodies, each saying whether it
    /// differs from `generation`'s own; a `PreviewMeshed` follows for each
    /// that does. Not part of the document: a replica ignores it.
    Previewed {
        tag: PreviewTag,
        generation: Generation,
        events: Vec<EvalEvent>,
        params: ParamValues,
        bodies: Vec<PreviewBody>,
    },
    /// A body a preview changed, meshed: every one coarse first, then every
    /// one refined, as `Meshed` does for a generation.
    PreviewMeshed {
        tag: PreviewTag,
        part: PartId,
        slot: BodySlot,
        detail: MeshDetail,
        mesh: Result<RenderMesh, Diagnostic>,
    },
}

/// The events a subscription receives, until the session ends.
pub type EventStream = Receiver<SessionEvent>;

/// The authority, as a client reaches it. Local mode is [`LocalSession`].
pub trait Session {
    /// Applies a request, or says why nothing was applied.
    fn request(&self, request: Request) -> Result<Outcome, Rejected>;

    /// The document as it saves: its directory form, carrying each
    /// plugin feature's latest frozen result that is current for the
    /// generation (docs/DATA-MODEL.md §Frozen results, ADR-0009).
    fn save(&self) -> MemorySource;

    /// A new subscription: `Opened` first, then everything after it.
    fn subscribe(&self) -> EventStream;

    fn submit(&self, envelope: CommandEnvelope) -> Result<Outcome, Rejected> {
        self.request(Request::Submit(envelope))
    }

    fn undo(&self, author: AuthorId) -> Result<Outcome, Rejected> {
        self.request(Request::Undo { author })
    }

    fn redo(&self, author: AuthorId) -> Result<Outcome, Rejected> {
        self.request(Request::Redo { author })
    }

    fn preview(&self, preview: PreviewId, envelope: CommandEnvelope) -> Result<Outcome, Rejected> {
        self.request(Request::Preview { preview, envelope })
    }

    fn clear_preview(&self, author: AuthorId) -> Result<Outcome, Rejected> {
        self.request(Request::ClearPreview { author })
    }
}

/// Why a replica could not follow the stream: each is a bug in the
/// authority or the transport, never the user's.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ReplicaError {
    #[error("the opened document does not open: {0}")]
    Open(String),
    #[error("expected generation {expected:?}, got {got:?}")]
    Gap {
        expected: Generation,
        got: Generation,
    },
    #[error("a change the authority applied is refused here: {0}")]
    Refused(#[from] CommandError),
    #[error("the replica differs from the authority at generation {0:?}")]
    Diverged(Generation),
}

/// A client's copy of the document, kept by applying the applied stream,
/// and the newest outcome it has heard for each feature.
#[derive(Clone, Debug, Default)]
pub struct Replica {
    document: Document,
    generation: Generation,
    outcomes: BTreeMap<FeatureId, (Generation, FeatureOutcome)>,
    finished: Option<(Generation, ParamValues)>,
}

impl Replica {
    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn generation(&self) -> Generation {
        self.generation
    }

    /// The feature's newest outcome and the generation it is of.
    pub fn outcome(&self, feature: FeatureId) -> Option<(Generation, &FeatureOutcome)> {
        self.outcomes.get(&feature).map(|(g, o)| (*g, o))
    }

    /// The newest finished evaluation's generation and parameter values.
    pub fn finished(&self) -> Option<(Generation, &ParamValues)> {
        self.finished.as_ref().map(|(g, p)| (*g, p))
    }

    /// Follows one event. An evaluation event older than the one already
    /// held for its feature is dropped: the newest generation wins.
    pub fn receive(&mut self, event: &SessionEvent) -> Result<(), ReplicaError> {
        match event {
            SessionEvent::Opened {
                generation,
                files,
                blobs,
            } => {
                let source = MemorySource(
                    files
                        .iter()
                        .map(|(p, text)| (p.clone(), text.as_bytes().to_vec()))
                        .chain(blobs.iter().map(|(p, bytes)| (p.clone(), bytes.clone())))
                        .collect(),
                );
                let document = open(&source).map_err(|e| ReplicaError::Open(e.to_string()))?;
                *self = Replica {
                    document,
                    generation: *generation,
                    ..Replica::default()
                };
            }
            SessionEvent::Applied { change, hash } => {
                let expected = Generation(self.generation.0 + 1);
                if change.generation != expected {
                    return Err(ReplicaError::Gap {
                        expected,
                        got: change.generation,
                    });
                }
                let applied = apply(&self.document, &change.command)?;
                if cfg!(debug_assertions)
                    && hash
                        .as_ref()
                        .is_some_and(|h| *h != DocHash::of(&applied.document))
                {
                    return Err(ReplicaError::Diverged(change.generation));
                }
                self.document = applied.document;
                self.generation = change.generation;
            }
            SessionEvent::Evaluated { generation, event } => {
                let newer = self
                    .outcomes
                    .get(&event.feature)
                    .is_none_or(|(g, _)| g <= generation);
                if newer {
                    self.outcomes
                        .insert(event.feature, (*generation, event.outcome.clone()));
                }
            }
            SessionEvent::Finished { generation, params } => {
                if self.finished.as_ref().is_none_or(|(g, _)| g <= generation) {
                    // Every feature of that generation has reported; an
                    // older outcome is of a feature no longer there.
                    self.outcomes.retain(|_, (g, _)| *g >= *generation);
                    self.finished = Some((*generation, params.clone()));
                }
            }
            SessionEvent::Meshed { .. }
            | SessionEvent::Previewed { .. }
            | SessionEvent::PreviewMeshed { .. } => {}
        }
        Ok(())
    }
}

/// The directory form's files: the JSON ones as text, and the blobs, the
/// only files that are not, as bytes.
fn files(doc: &Document) -> (BTreeMap<String, String>, BTreeMap<String, Vec<u8>>) {
    let (mut files, mut blobs) = (BTreeMap::new(), BTreeMap::new());
    for (path, bytes) in save(doc).0 {
        if path.starts_with("blobs/") {
            blobs.insert(path, bytes);
        } else {
            let text = String::from_utf8(bytes).expect("a file but a blob is JSON, so UTF-8");
            files.insert(path, text);
        }
    }
    (files, blobs)
}
