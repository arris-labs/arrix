//! Which body slots a feature reads, modifies or consumes (ADR-0007). The
//! DAG derives its implicit edges from this and the evaluator resolves a
//! reader's body version by it, so both agree on what a record does.

use std::collections::BTreeSet;

use arrix_core::{FeatureId, Ref, SlotName};
use arrix_plugin_api::{InputKind, SlotKind, SlotSpec};

use crate::dag::ref_features;
use crate::document::{Document, FeatureRecord, Part};
use crate::registry::Registry;

/// A body slot, named by the feature that made it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SlotKey {
    pub(crate) feature: FeatureId,
    pub(crate) slot: SlotName,
}

/// What a feature does to a body slot it names by a body input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Effect {
    /// Its output slot of this name is the slot's next version.
    Modifies(SlotName),
    /// It reads the body as a tool, which ends the slot.
    Consumes,
}

/// The body slots `record` names by its body inputs, each with what the
/// record does to it. A type the registry lacks does nothing to any.
pub(crate) fn effects(registry: &Registry, record: &FeatureRecord) -> Vec<(SlotKey, Effect)> {
    let Some(ty) = registry.get(&record.type_id) else {
        return Vec::new();
    };
    let spec = ty.spec();
    let mut out = Vec::new();
    for input in spec.inputs.iter().filter(|i| i.kind == InputKind::Body) {
        let Some(Ref::Slot { feature, slot }) = record.inputs.get(&input.name) else {
            continue;
        };
        let key = SlotKey {
            feature: *feature,
            slot: slot.clone(),
        };
        let modifying = spec
            .outputs
            .iter()
            .find(|s| s.modifies.as_deref() == Some(&input.name));
        let effect = match modifying {
            Some(s) => Effect::Modifies(s.name.clone()),
            None => Effect::Consumes,
        };
        out.push((key, effect));
    }
    out
}

/// Whether `slot` of a `record`'s type is a version of another feature's
/// slot rather than one of its own: it names an input it modifies, and the
/// record sets that input. With the input unset (a `core.extrude` making a
/// new body) it is the feature's own slot.
pub(crate) fn is_version(slot: &SlotSpec, record: &FeatureRecord) -> bool {
    slot.modifies
        .as_ref()
        .is_some_and(|input| record.inputs.contains_key(input))
}

/// The body slots a persistent name into `feature` may be a face of: its
/// own body slots when it made them, and the slots it modifies.
pub(crate) fn named_slots(
    registry: &Registry,
    doc: &Document,
    feature: FeatureId,
) -> BTreeSet<SlotKey> {
    let mut out = BTreeSet::new();
    let Some((_, _, record)) = doc.feature(feature) else {
        return out;
    };
    for (key, effect) in effects(registry, record) {
        if matches!(effect, Effect::Modifies(_)) {
            out.insert(key);
        }
    }
    if let Some(ty) = registry.get(&record.type_id) {
        for s in &ty.spec().outputs {
            if s.kind == SlotKind::Body && !is_version(s, record) {
                out.insert(SlotKey {
                    feature,
                    slot: s.name.clone(),
                });
            }
        }
    }
    out
}

/// Every body slot `record` reads: by a body input, or by a persistent
/// name into a feature whose body is a version of it.
pub(crate) fn reads(
    registry: &Registry,
    doc: &Document,
    record: &FeatureRecord,
) -> BTreeSet<SlotKey> {
    let mut out: BTreeSet<SlotKey> = effects(registry, record)
        .into_iter()
        .map(|(key, _)| key)
        .collect();
    for r in record.inputs.values() {
        if matches!(r, Ref::Topo(_)) {
            for f in ref_features(r) {
                out.extend(named_slots(registry, doc, f));
            }
        }
    }
    out
}

/// The features of `part` at history positions before `before` that
/// modify or consume `key`, in history order.
pub(crate) fn touching<'d>(
    registry: &Registry,
    part: &'d Part,
    before: usize,
    key: &SlotKey,
) -> Vec<(&'d FeatureRecord, Effect)> {
    part.history[..before.min(part.history.len())]
        .iter()
        .filter_map(|id| part.features.get(id))
        .filter_map(|record| {
            effects(registry, record)
                .into_iter()
                .find(|(k, _)| k == key)
                .map(|(_, effect)| (record, effect))
        })
        .collect()
}
