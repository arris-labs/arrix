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
mod export_tests;
#[cfg(test)]
mod interrupt_tests;
mod local;
#[cfg(test)]
mod plugin_tests;
mod plugins;
mod preview;
#[cfg(test)]
mod preview_tests;
#[cfg(feature = "schema")]
mod schema;
#[cfg(test)]
mod step_tree_tests;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::fmt;
use std::sync::mpsc::Receiver;

use arrix_core::{
    AssemblyId, Diagnostic, ExportFormat, FeatureId, MeshDetail, ParamId, PartId, PluginId,
    Quantity, RenderMesh,
};
use arrix_plugin_api::FeatureTypeSpec;
use serde::{Deserialize, Serialize};

pub use local::LocalSession;
pub use plugins::PluginLoader;

use crate::authority::{AuthorId, Change, CommandEnvelope, Generation, Outcome, Rejected};
use crate::command::{CommandError, apply};
use crate::document::{Document, FeatureTypeId};
use crate::eval::{AssemblyOutcome, BodySlot, EvalEvent, FeatureOutcome, Masses};
use crate::open::{MemorySource, open};
use crate::save::save;
use crate::step_tree::StepTree;
pub use preview::{PreviewBody, PreviewId, PreviewTag};
#[cfg(feature = "schema")]
pub use schema::protocol_schemas;

/// What an exporter (ADR-0037) is run on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ExportSubject {
    Part(PartId),
    Assembly(AssemblyId),
}

/// An export by an exporter: its id (`<plugin>.<name>`), the part or
/// assembly it is of and its options as a feature's: `params` are
/// expressions, `choices` words; what is not given is the spec's default.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExporterRequest {
    pub id: String,
    pub subject: ExportSubject,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub choices: BTreeMap<String, String>,
}

/// A file an exporter wrote: a relative `/`-separated path, the main file
/// first (ADR-0037 §7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExportedFileEvent {
    pub name: String,
    #[serde(with = "crate::command::bytes")]
    #[cfg_attr(feature = "schema", schemars(with = "String"))]
    pub bytes: Vec<u8>,
}

/// The session protocol's version (ADR-0023 §6), raised with any change to
/// the shapes of its requests, outcomes and events; separate from the
/// plugin API's.
pub const PROTOCOL_VERSION: u32 = 1;

/// The round-trip check every protocol test case goes through: `value`
/// serialised, read back equal, and, with the `schema` feature (which the
/// workspace's test run turns on), valid against the schema of `kind`
/// (`request`, `outcome`, `rejected` or `event`). Returns the JSON.
#[cfg(test)]
pub(crate) fn crosses<T>(value: &T, kind: &str) -> String
where
    T: Serialize + serde::de::DeserializeOwned + PartialEq + fmt::Debug,
{
    let json = serde_json::to_string(value).expect("a protocol value serialises");
    assert_eq!(
        &serde_json::from_str::<T>(&json).expect("it reads back"),
        value,
        "{json}"
    );
    #[cfg(feature = "schema")]
    schema::validate(kind, &json);
    #[cfg(not(feature = "schema"))]
    let _ = kind;
    json
}

/// What a client asks of the authority.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
    /// Writes the final bodies `bodies` names (every one when empty) as
    /// `format`, once the newest commit is evaluated; answered `Exporting`,
    /// the file following as `Exported`. Not a command: the document does
    /// not change, no generation is made and nothing is undone.
    Export {
        bodies: Vec<BodySlot>,
        format: ExportFormat,
        /// Write this assembly with its structure (ADR-0032 §5) instead
        /// of bodies: STEP only, `bodies` empty.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        assembly: Option<AssemblyId>,
        /// Run this exporter (ADR-0037) instead: `bodies`, `format` and
        /// `assembly` are then ignored, and the files follow as
        /// `Exported::files`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        exporter: Option<ExporterRequest>,
    },
    /// Reads the product tree of the STEP file `bytes` on the worker
    /// (ADR-0032), answered `Reading`, the tree following as
    /// `StepTree` under `tag`. Not a command: the document does not
    /// change; the client turns the tree into one with `import_products`.
    ReadStepTree {
        tag: u64,
        #[serde(with = "crate::command::bytes")]
        #[cfg_attr(feature = "schema", schemars(with = "String"))]
        bytes: Vec<u8>,
    },
    /// Loads the plugin directory `dir` on the authority's machine
    /// (ADR-0024): its feature types join the registry in a generation of
    /// their own, announced by `SessionEvent::Registry`, and answered
    /// `Loaded`. Not a command: nothing is undone and no document changes.
    LoadPlugin {
        dir: String,
    },
    /// Restarts the loaded `plugin` and takes its feature types again, in
    /// a generation even when no spec changed, since the code did.
    ReloadPlugin {
        plugin: PluginId,
    },
}

/// BLAKE3 over a document's saved files, as 64 hex digits: equal
/// documents, equal hashes.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
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
        /// The feature types loaded since the session opened (ADR-0024
        /// §5), which the document's files do not say.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        types: Vec<FeatureTypeSpec>,
    },
    /// A change the authority applied, with the document's hash after it
    /// in a debug build, for the replicas to check theirs against.
    Applied {
        change: Change,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        hash: Option<DocHash>,
    },
    /// The registry changed (ADR-0024): `generation` is the one it made,
    /// evaluated like any other. `added` are the specs of the types
    /// loaded, new or replaced; `removed` the ids a reload no longer has.
    /// Not part of the document: a replica takes the generation only.
    Registry {
        generation: Generation,
        added: Vec<FeatureTypeSpec>,
        removed: Vec<FeatureTypeId>,
    },
    /// One feature's outcome in the evaluation of `generation`.
    Evaluated {
        generation: Generation,
        event: EvalEvent,
    },
    /// One assembly's solve in the evaluation of `generation`: where each
    /// component is and how each joint ended (docs/DATA-MODEL.md
    /// §Evaluation), sent for every assembly before `Finished`.
    Assembled {
        generation: Generation,
        assembly: AssemblyId,
        outcome: AssemblyOutcome,
    },
    /// The mass, centroid and inertia of every body, part and assembly in
    /// the evaluation of `generation` (ADR-0031), sent after the
    /// `Assembled` events and before `Finished`.
    Massed {
        generation: Generation,
        masses: Masses,
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
    /// The file a `Request::Export` asked for, written from the bodies of
    /// `generation`'s evaluation, or why it could not be. Not part of the
    /// document: a replica ignores it.
    Exported {
        generation: Generation,
        format: ExportFormat,
        /// The file; empty when an exporter wrote (see `files`).
        bytes: Result<Vec<u8>, Diagnostic>,
        /// The files an exporter wrote, the main one first (ADR-0037);
        /// empty for a `format` export.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        files: Vec<ExportedFileEvent>,
    },
    /// The product tree a `Request::ReadStepTree` asked for, or why the
    /// file could not be read. Not part of the document: a replica
    /// ignores it.
    StepTree {
        tag: u64,
        tree: Result<StepTree, Diagnostic>,
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

    fn export(&self, bodies: Vec<BodySlot>, format: ExportFormat) -> Result<Outcome, Rejected> {
        self.request(Request::Export {
            bodies,
            format,
            assembly: None,
            exporter: None,
        })
    }

    /// The exporter `exporter` (ADR-0037) of a part or assembly, its files
    /// following as `Exported::files`.
    fn export_with(&self, exporter: ExporterRequest) -> Result<Outcome, Rejected> {
        self.request(Request::Export {
            bodies: Vec::new(),
            format: ExportFormat::Step,
            assembly: None,
            exporter: Some(exporter),
        })
    }

    /// The assembly `assembly` as a STEP file with its product structure.
    fn export_assembly(&self, assembly: AssemblyId) -> Result<Outcome, Rejected> {
        self.request(Request::Export {
            bodies: Vec::new(),
            format: ExportFormat::Step,
            assembly: Some(assembly),
            exporter: None,
        })
    }

    fn read_step_tree(&self, tag: u64, bytes: Vec<u8>) -> Result<Outcome, Rejected> {
        self.request(Request::ReadStepTree { tag, bytes })
    }

    fn clear_preview(&self, author: AuthorId) -> Result<Outcome, Rejected> {
        self.request(Request::ClearPreview { author })
    }

    fn load_plugin(&self, dir: &str) -> Result<Outcome, Rejected> {
        self.request(Request::LoadPlugin { dir: dir.into() })
    }

    fn reload_plugin(&self, plugin: PluginId) -> Result<Outcome, Rejected> {
        self.request(Request::ReloadPlugin { plugin })
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
    assemblies: BTreeMap<AssemblyId, (Generation, AssemblyOutcome)>,
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

    /// The assembly's newest solve and the generation it is of.
    pub fn assembly(&self, assembly: AssemblyId) -> Option<(Generation, &AssemblyOutcome)> {
        self.assemblies.get(&assembly).map(|(g, o)| (*g, o))
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
                ..
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
            SessionEvent::Registry { generation, .. } => {
                let expected = Generation(self.generation.0 + 1);
                if *generation != expected {
                    return Err(ReplicaError::Gap {
                        expected,
                        got: *generation,
                    });
                }
                self.generation = *generation;
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
            SessionEvent::Assembled {
                generation,
                assembly,
                outcome,
            } => {
                let newer = self
                    .assemblies
                    .get(assembly)
                    .is_none_or(|(g, _)| g <= generation);
                if newer {
                    self.assemblies
                        .insert(*assembly, (*generation, outcome.clone()));
                }
            }
            SessionEvent::Finished { generation, params } => {
                if self.finished.as_ref().is_none_or(|(g, _)| g <= generation) {
                    self.assemblies.retain(|_, (g, _)| *g >= *generation);
                    // Every feature of that generation has reported; an
                    // older outcome is of a feature no longer there.
                    self.outcomes.retain(|_, (g, _)| *g >= *generation);
                    self.finished = Some((*generation, params.clone()));
                }
            }
            SessionEvent::Massed { .. }
            | SessionEvent::StepTree { .. }
            | SessionEvent::Meshed { .. }
            | SessionEvent::Previewed { .. }
            | SessionEvent::PreviewMeshed { .. }
            | SessionEvent::Exported { .. } => {}
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
