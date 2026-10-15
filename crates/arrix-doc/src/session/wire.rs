//! The worker's messages as serialised data, for a worker that is not on
//! a channel of this process: a browser's `postMessage` (ADR-0042). They
//! are internal messages between the authority's core and the evaluator,
//! not the public protocol, so `protocol.schema.json` does not list them.
//! A snapshot is the document's own serde form, the files `save` writes
//! (ADR-0023); a frozen result's bodies are its blob bytes.
//!
//! Natively nothing crosses this way, except in tests, where a thread-local
//! switch makes `LocalSession` pass every job and answer through it, so the
//! session suite runs against the wire form.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_core::{AssemblyId, ExportFormat, FeatureId};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::preview::{ExpandJob, ExportJob, Job, PreviewJob, PreviewTag, StructureJob};
use super::{ExpandRequest, ExporterRequest, SessionEvent};
use crate::authority::Generation;
use crate::document::Document;
use crate::eval::BodySlot;
use crate::frozen::{BlobRef, Frozen, FrozenResult};
use crate::open::{MemorySource, OpenError, open};
use crate::save::save;

#[derive(Debug, thiserror::Error)]
pub(super) enum WireError {
    #[error("a registry change has no wire form: the browser has no plugin loader")]
    Registry,
    #[error("not a message: {0}")]
    Malformed(#[from] serde_json::Error),
    #[error("the snapshot does not open: {0}")]
    Snapshot(#[from] OpenError),
}

/// Bytes as base64 text, which JSON carries at a third of the size of a
/// list of numbers.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Bytes(Vec<u8>);

impl Serialize for Bytes {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&STANDARD.encode(&self.0))
    }
}

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        STANDARD
            .decode(text)
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}

/// A document as its files.
#[derive(Serialize, Deserialize)]
struct Files(BTreeMap<String, Bytes>);

impl Files {
    fn of(doc: &Document) -> Self {
        Self(
            save(doc)
                .0
                .into_iter()
                .map(|(path, bytes)| (path, Bytes(bytes)))
                .collect(),
        )
    }

    fn open(self) -> Result<Arc<Document>, OpenError> {
        let files = self.0.into_iter().map(|(p, b)| (p, b.0)).collect();
        open(&MemorySource(files)).map(Arc::new)
    }
}

/// What the authority posts to the evaluator. There is no registry
/// variant: only a plugin load makes one (ADR-0024), and the browser has
/// no plugin loader.
#[derive(Serialize, Deserialize)]
enum ToWorker {
    Commit {
        generation: Generation,
        doc: Files,
    },
    Preview {
        tag: PreviewTag,
        generation: Generation,
        doc: Files,
    },
    ClearPreview,
    Export {
        bodies: Vec<BodySlot>,
        format: ExportFormat,
        assembly: Option<AssemblyId>,
        exporter: Option<ExporterRequest>,
    },
    Expand {
        request: ExpandRequest,
    },
    Structure {
        tag: u64,
        bytes: Bytes,
    },
}

pub(super) fn encode_job(job: &Job) -> Result<String, WireError> {
    let message = match job {
        Job::Commit((generation, doc)) => ToWorker::Commit {
            generation: *generation,
            doc: Files::of(doc),
        },
        Job::Registry(..) => return Err(WireError::Registry),
        Job::Preview(p) => ToWorker::Preview {
            tag: p.tag,
            generation: p.generation,
            doc: Files::of(&p.doc),
        },
        Job::ClearPreview => ToWorker::ClearPreview,
        Job::Export(e) => ToWorker::Export {
            bodies: e.bodies.clone(),
            format: e.format,
            assembly: e.assembly,
            exporter: e.exporter.clone(),
        },
        Job::Expand(e) => ToWorker::Expand {
            request: e.request.clone(),
        },
        Job::Structure(s) => ToWorker::Structure {
            tag: s.tag,
            bytes: Bytes(s.bytes.clone()),
        },
    };
    Ok(serde_json::to_string(&message)?)
}

pub(super) fn decode_job(text: &str) -> Result<Job, WireError> {
    Ok(match serde_json::from_str(text)? {
        ToWorker::Commit { generation, doc } => Job::Commit((generation, doc.open()?)),
        ToWorker::Preview {
            tag,
            generation,
            doc,
        } => Job::Preview(PreviewJob {
            tag,
            generation,
            doc: doc.open()?,
        }),
        ToWorker::ClearPreview => Job::ClearPreview,
        ToWorker::Export {
            bodies,
            format,
            assembly,
            exporter,
        } => Job::Export(ExportJob {
            bodies,
            format,
            assembly,
            exporter,
        }),
        ToWorker::Expand { request } => Job::Expand(ExpandJob { request }),
        ToWorker::Structure { tag, bytes } => Job::Structure(StructureJob {
            tag,
            bytes: bytes.0,
        }),
    })
}

/// A frozen result, its blobs as bytes.
#[derive(Serialize, Deserialize)]
struct FrozenWire {
    feature: FeatureId,
    result: FrozenResult,
    blobs: Vec<(BlobRef, Bytes)>,
}

/// What the evaluator says back: an event to publish, or the plugin
/// features' results of an evaluation, sent before its `Finished`.
#[derive(Serialize, Deserialize)]
enum FromWorker {
    Publish(Box<SessionEvent>),
    Freeze(Vec<FrozenWire>),
}

/// What the evaluator said, decoded.
pub(super) enum Heard {
    Publish(Box<SessionEvent>),
    Freeze(BTreeMap<FeatureId, Arc<Frozen>>),
}

pub(super) fn encode_publish(event: &SessionEvent) -> String {
    serde_json::to_string(&FromWorker::Publish(Box::new(event.clone())))
        .expect("session events serialise to JSON")
}

pub(super) fn encode_freeze(results: &BTreeMap<FeatureId, Arc<Frozen>>) -> String {
    let list = results
        .iter()
        .map(|(feature, frozen)| FrozenWire {
            feature: *feature,
            result: frozen.result.clone(),
            blobs: frozen
                .blobs
                .iter()
                .map(|(r, b)| (r.clone(), Bytes(b.clone())))
                .collect(),
        })
        .collect();
    serde_json::to_string(&FromWorker::Freeze(list)).expect("frozen results serialise to JSON")
}

pub(super) fn decode_heard(text: &str) -> Result<Heard, WireError> {
    Ok(match serde_json::from_str(text)? {
        FromWorker::Publish(event) => Heard::Publish(event),
        FromWorker::Freeze(list) => Heard::Freeze(
            list.into_iter()
                .map(|w| {
                    let blobs = w.blobs.into_iter().map(|(r, b)| (r, b.0)).collect();
                    (
                        w.feature,
                        Arc::new(Frozen {
                            result: w.result,
                            blobs,
                        }),
                    )
                })
                .collect(),
        ),
    })
}

#[cfg(test)]
thread_local! {
    static LOOPBACK: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether sessions opened on this thread pass their messages through the
/// wire form. Only a test turns it on.
pub(super) fn loopback() -> bool {
    #[cfg(test)]
    {
        LOOPBACK.get()
    }
    #[cfg(not(test))]
    {
        false
    }
}

/// Runs `scenario` with every session it opens on this thread in loopback.
#[cfg(test)]
pub(super) fn in_loopback<R>(scenario: impl FnOnce() -> R) -> R {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            LOOPBACK.set(false);
        }
    }
    LOOPBACK.set(true);
    let _reset = Reset;
    scenario()
}
