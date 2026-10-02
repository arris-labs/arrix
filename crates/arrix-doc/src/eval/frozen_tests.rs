//! A plugin feature evaluated from its stored result (docs/DATA-MODEL.md
//! §Frozen results): each state, each transition, and a frozen feature
//! with nothing stored. `test.block` stands in for a plugin's feature.

use std::sync::Arc;

use arrix_core::{Id, ParamId, SweepPartName};

use super::tests::{Block, PART, doc, face, failure, fid, plane_slot, record};
use super::*;
use crate::document::FeatureTypeId;
use crate::registry::TypeOutput;

/// `test.block` as a plugin of version `.1` contributes it.
struct Plugin(Block, &'static str);

impl FeatureType for Plugin {
    fn spec(&self) -> &arrix_plugin_api::FeatureTypeSpec {
        self.0.spec()
    }

    fn evaluate(
        &self,
        kernel: &mut dyn arrix_plugin_api::Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        self.0.evaluate(kernel, args)
    }

    fn plugin_version(&self) -> Option<&str> {
        Some(self.1)
    }
}

fn with_plugin(version: &'static str) -> Registry {
    let mut r = Registry::with_core_types();
    r.register(Arc::new(Plugin(Block::new(), version))).unwrap();
    r
}

const BLOCK: u64 = 2;
const ABOVE: u64 = 3;

/// A plane, a block on it, and a plane 2 mm above the block's top face.
fn recipe() -> Document {
    let top = Ref::Topo(face(BLOCK, SweepPartName::EndCap));
    doc(
        &[(10, "w", "20 mm")],
        vec![
            record(1, "core.datum-plane", &[("offset", "10 mm")], &[]),
            record(
                BLOCK,
                "test.block",
                &[("width", "w")],
                &[("plane", plane_slot(1))],
            ),
            record(
                ABOVE,
                "core.datum-plane",
                &[("offset", "2 mm")],
                &[("plane", top)],
            ),
        ],
    )
}

/// `recipe()` evaluated live, with the block's result stored as a save
/// writes it.
fn stored() -> (Document, Evaluation) {
    let mut d = recipe();
    let mut ev = Evaluator::new(with_plugin("0.1.0"));
    let live = ev.evaluate(&d);
    let frozen = ev
        .frozen()
        .get(&fid(BLOCK))
        .expect("a live plugin feature")
        .clone();
    assert!(d.freeze(fid(BLOCK), &frozen));
    (d, live)
}

fn set_width(d: &mut Document, expr: &str) {
    d.params.get_mut(&ParamId(Id(10))).unwrap().expr = Expr::parse(expr).unwrap();
}

fn info(e: &Evaluation, feature: u64) -> Option<FrozenInfo> {
    match e.outcome(fid(feature)) {
        Some(FeatureOutcome::Ok { frozen, .. }) => *frozen,
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn codes(e: &Evaluation, feature: u64) -> Vec<String> {
    match e.outcome(fid(feature)) {
        Some(FeatureOutcome::Ok { diagnostics, .. }) => {
            diagnostics.iter().map(|d| d.code.to_string()).collect()
        }
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn hash(e: &Evaluation, feature: u64) -> InputHash {
    match e.outcome(fid(feature)) {
        Some(FeatureOutcome::Ok { hash, .. }) => *hash,
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn volume(e: &Evaluation, feature: u64) -> f64 {
    match e.slot(fid(feature), "body") {
        Some(SlotView::Body(m)) => m.volume,
        other => panic!("feature {feature}: {other:?}"),
    }
}

#[test]
fn a_live_plugin_feature_is_not_frozen() {
    let (_, live) = stored();
    assert_eq!(info(&live, BLOCK), None);
    assert!(codes(&live, BLOCK).is_empty() && codes(&live, ABOVE).is_empty());
}

#[test]
fn without_its_plugin_a_feature_is_frozen_missing_and_its_readers_do_not_notice() {
    let (d, live) = stored();
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&d);
    let expected = FrozenInfo {
        reason: FrozenReason::Missing,
        stale: false,
    };
    assert_eq!(info(&e, BLOCK), Some(expected));
    assert_eq!(codes(&e, BLOCK), ["plugin.frozen.missing"]);
    assert_eq!(volume(&e, BLOCK), volume(&live, BLOCK));
    assert_eq!(e.slot(fid(BLOCK), "body"), live.slot(fid(BLOCK), "body"));
    // Same hash as live, so what reads it is the same evaluation.
    assert_eq!(hash(&e, BLOCK), hash(&live, BLOCK));
    assert_eq!(hash(&e, ABOVE), hash(&live, ABOVE));
    // The reference into its names resolves through the stored table.
    assert_eq!(info(&e, ABOVE), None);
    assert!(codes(&e, ABOVE).is_empty());
    assert_eq!(e.slot(fid(ABOVE), "plane"), live.slot(fid(ABOVE), "plane"));
}

#[test]
fn an_upstream_edit_makes_it_stale_and_the_warning_goes_downstream() {
    let (mut d, live) = stored();
    set_width(&mut d, "30 mm");
    let mut ev = Evaluator::new(Registry::with_core_types());
    let e = ev.evaluate(&d);
    let expected = FrozenInfo {
        reason: FrozenReason::Missing,
        stale: true,
    };
    assert_eq!(info(&e, BLOCK), Some(expected));
    assert_eq!(
        codes(&e, BLOCK),
        ["plugin.frozen.missing", "plugin.frozen.stale"]
    );
    // The stored result stands in, so what is below it does not move.
    assert_eq!(volume(&e, BLOCK), volume(&live, BLOCK));
    assert_eq!(e.slot(fid(ABOVE), "plane"), live.slot(fid(ABOVE), "plane"));
    assert_eq!(codes(&e, ABOVE), ["plugin.frozen.stale"]);
    let Some(FeatureOutcome::Ok { diagnostics, .. }) = e.outcome(fid(ABOVE)) else {
        panic!()
    };
    assert_eq!(diagnostics[0].severity, Severity::Warning);
    assert_eq!(diagnostics[0].refs, [Ref::Feature(fid(BLOCK))]);
    // Nothing is stored for a result that was not computed live.
    assert!(ev.frozen().is_empty());
    // Edited back, it is not stale.
    set_width(&mut d, "20 mm");
    let back = ev.evaluate(&d);
    assert_eq!(info(&back, BLOCK).map(|f| f.stale), Some(false));
    assert!(codes(&back, ABOVE).is_empty());
}

#[test]
fn with_the_plugin_back_it_is_live_and_re_frozen() {
    let (mut d, live) = stored();
    set_width(&mut d, "30 mm");
    let mut ev = Evaluator::new(with_plugin("0.1.0"));
    let e = ev.evaluate(&d);
    assert_eq!(info(&e, BLOCK), None);
    assert!(codes(&e, BLOCK).is_empty() && codes(&e, ABOVE).is_empty());
    assert!(volume(&e, BLOCK) > volume(&live, BLOCK));
    let now = &ev.frozen()[&fid(BLOCK)];
    assert_eq!(now.result.input_hash, hash(&e, BLOCK));
    assert_ne!(
        now.result.input_hash,
        d.parts[&PART].features[&fid(BLOCK)]
            .frozen
            .as_ref()
            .unwrap()
            .input_hash
    );
}

#[test]
fn a_plugin_of_another_major_version_leaves_it_frozen() {
    let (d, live) = stored();
    let e = Evaluator::new(with_plugin("1.0.0")).evaluate(&d);
    let expected = FrozenInfo {
        reason: FrozenReason::Version,
        stale: false,
    };
    assert_eq!(info(&e, BLOCK), Some(expected));
    assert_eq!(codes(&e, BLOCK), ["plugin.frozen.version"]);
    assert_eq!(volume(&e, BLOCK), volume(&live, BLOCK));
    // A compatible release evaluates live, under its own hash.
    let e = Evaluator::new(with_plugin("0.1.7")).evaluate(&d);
    assert_eq!(info(&e, BLOCK), None);
    // An older plugin than the one that stored it is not compatible.
    let e = Evaluator::new(with_plugin("0.0.9")).evaluate(&d);
    assert_eq!(
        info(&e, BLOCK).map(|f| f.reason),
        Some(FrozenReason::Version)
    );
}

#[test]
fn a_record_newer_than_its_type_evaluates_frozen_with_the_version_reason() {
    let (mut d, _) = stored();
    d.parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(BLOCK))
        .unwrap()
        .type_version = 2;
    let e = Evaluator::new(with_plugin("0.1.0")).evaluate(&d);
    assert_eq!(
        info(&e, BLOCK).map(|f| f.reason),
        Some(FrozenReason::Version)
    );
    // Its hash is not the stored one's, which was made under type version 1.
    assert_eq!(info(&e, BLOCK).map(|f| f.stale), Some(true));
}

#[test]
fn a_frozen_feature_with_no_stored_result_fails_with_no_outputs() {
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&recipe());
    let d = failure(&e, BLOCK);
    assert_eq!(d.code.as_str(), "plugin.frozen.missing");
    assert!(e.slot(fid(BLOCK), "body").is_none());
    // Its reader is unavailable, not given something else.
    assert_eq!(failure(&e, ABOVE).code.as_str(), "input.unavailable");
    let e = Evaluator::new(with_plugin("0.1.0")).evaluate(&recipe());
    assert_eq!(info(&e, BLOCK), None);
}

#[test]
fn a_core_feature_of_an_unknown_type_is_not_frozen() {
    let d = doc(&[], vec![record(1, "core.nothing", &[], &[])]);
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&d);
    assert_eq!(failure(&e, 1).code.as_str(), "feature.unknown-type");
}

#[test]
fn a_missing_or_corrupt_blob_fails_the_feature() {
    let (d, _) = stored();
    let result = d.parts[&PART].features[&fid(BLOCK)].frozen.clone().unwrap();
    let body = result.slots.values().next().unwrap().clone();

    let mut missing = d.clone();
    missing.blobs.remove(&body);
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&missing);
    assert_eq!(failure(&e, BLOCK).code.as_str(), "blob.missing");

    let mut corrupt = d.clone();
    corrupt.blobs.get_mut(&body).unwrap()[40] ^= 1;
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&corrupt);
    assert_eq!(failure(&e, BLOCK).code.as_str(), "blob.corrupt");
}

#[test]
fn removing_a_plugin_leaves_the_others() {
    let mut r = with_plugin("0.1.0");
    let plugin = arrix_core::PluginId::new("test").unwrap();
    assert!(r.remove_plugin(&plugin));
    assert!(!r.remove_plugin(&plugin));
    assert!(r.get(&FeatureTypeId::new("test.block").unwrap()).is_none());
    assert!(
        r.get(&FeatureTypeId::new("core.extrude").unwrap())
            .is_some()
    );
}
