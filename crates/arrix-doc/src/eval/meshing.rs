//! The bodies a part ends with and their meshes (docs/CONCURRENCY-WASM.md
//! §Tessellation): each body slot's last version that no later feature
//! consumed, which is what the user sees. Intermediate versions are never
//! meshed, and a version the cache holds keeps its meshes.

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{Diagnostic, FeatureId, MeshDetail, PartId, RenderMesh, SlotName};
use arrix_kernel::KernelBody;
use arrix_plugin_api::SlotKind;
use serde::{Deserialize, Serialize};

use super::{Evaluator, InputHash, SlotOut, State, host};
use crate::bodies::{self, Effect, SlotKey};
use crate::document::Document;

/// A body slot, named by the feature that made it: the name a mesh of its
/// final version goes by, whatever feature made that version.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BodySlot {
    pub feature: FeatureId,
    pub slot: SlotName,
}

/// A part's body as it ends: the slot, and the kernel's handle for its
/// last version.
#[derive(Clone, Debug, PartialEq)]
pub struct FinalBody {
    pub part: PartId,
    pub slot: BodySlot,
    pub(super) body: KernelBody,
    /// The input hash of the feature that made the last version, which
    /// identifies it (an anchor's input, ADR-0029 §2).
    pub(super) hash: InputHash,
}

impl Evaluator {
    /// The final bodies of the latest evaluation, in the document's order;
    /// none when it was stopped before it finished.
    pub fn final_bodies(&self) -> &[FinalBody] {
        &self.finals
    }

    /// `body` as a render mesh at `detail`, from the cache when its
    /// version was meshed before. A kernel failure is a diagnostic.
    pub fn mesh(&mut self, body: &FinalBody, detail: MeshDetail) -> Result<RenderMesh, Diagnostic> {
        if let Some(mesh) = self.meshes.get(&(body.body, detail)) {
            return Ok(mesh.clone());
        }
        let mesh = self
            .kernel
            .tessellate(body.body, detail)
            .map_err(|e| host::kernel_diagnostic(&e))?;
        self.meshes.insert((body.body, detail), mesh.clone());
        Ok(mesh)
    }

    /// Finds each part's final bodies once every feature has its state.
    pub(super) fn find_finals(
        &self,
        doc: &Document,
        states: &BTreeMap<FeatureId, State>,
    ) -> Vec<FinalBody> {
        let mut out = Vec::new();
        for part in doc.parts.values() {
            let end = part.rollback.unwrap_or(part.history.len());
            for id in &part.history[..end.min(part.history.len())] {
                let Some(State::Ok { slots, .. }) = states.get(id) else {
                    continue;
                };
                let record = &part.features[id];
                let ty = self.registry.get(&record.type_id);
                for (name, slot) in slots {
                    let SlotOut::Body { .. } = slot else { continue };
                    // A version is another slot's, and is found from it.
                    let version = ty.as_ref().is_some_and(|ty| {
                        let spec = ty.spec();
                        spec.outputs.iter().any(|s| {
                            s.name == *name
                                && s.kind == SlotKind::Body
                                && bodies::is_version(s, record)
                        })
                    });
                    if version {
                        continue;
                    }
                    let key = SlotKey {
                        feature: *id,
                        slot: name.clone(),
                    };
                    if let Some((body, hash)) = last_version(self, part, end, states, &key) {
                        out.push(FinalBody {
                            part: part.id,
                            slot: BodySlot {
                                feature: key.feature,
                                slot: key.slot,
                            },
                            body,
                            hash,
                        });
                    }
                }
            }
        }
        out
    }

    /// Drops the meshes of bodies the cache no longer holds.
    pub(super) fn keep_meshes(&mut self, keep: &BTreeSet<KernelBody>) {
        self.meshes.retain(|(body, _), _| keep.contains(body));
    }
}

/// The body of `key`'s last version before `end` and the input hash of
/// the feature that made it, or `None` when a later
/// feature consumed it or the feature that made its last version has no
/// outputs.
fn last_version(
    e: &Evaluator,
    part: &crate::document::Part,
    end: usize,
    states: &BTreeMap<FeatureId, State>,
    key: &SlotKey,
) -> Option<(KernelBody, InputHash)> {
    let mut current = key.clone();
    for (record, effect) in bodies::touching(&e.registry, part, end, key) {
        if record.suppressed {
            continue;
        }
        match effect {
            Effect::Consumes => return None,
            Effect::Modifies(out) => {
                current = SlotKey {
                    feature: record.id,
                    slot: out,
                };
            }
        }
    }
    match states.get(&current.feature)? {
        State::Ok { slots, hash, .. } => match slots.get(&current.slot)? {
            SlotOut::Body { body, .. } => Some((*body, *hash)),
            _ => None,
        },
        State::Unavailable(_) => None,
    }
}
