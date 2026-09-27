use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_core::{
    CurveKey, DVec2, DVec3, Diagnostic, FeatureId, Id, NameRoot, ParamId, PartId, PersistentName,
    Profile, ProfileLoop, ProfileSegment, QuantityKind, Ref, SlotName, SweepPartName, TopoKind,
};
use arrix_kernel::KernelOp;
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec,
};

use super::*;
use crate::document::{FeatureTypeId, Param};
use crate::registry::TypeOutput;

const PART: PartId = PartId(Id(1));

fn fid(n: u64) -> FeatureId {
    FeatureId(Id(n))
}

fn slot(s: &str) -> SlotName {
    SlotName::new(s).unwrap()
}

/// `test.block`: a `width` square on its `plane`, extruded `height`. Its
/// sides are keyed 1–4 from the plane's origin anticlockwise.
struct Block(FeatureTypeSpec);

impl Block {
    fn new() -> Self {
        let param = |name: &str, default: &str| ParamSpec {
            name: name.into(),
            title: name.into(),
            kind: QuantityKind::Length,
            default: default.into(),
        };
        Block(FeatureTypeSpec {
            id: "test.block".into(),
            version: 1,
            title: "Block".into(),
            params: vec![param("width", "10 mm"), param("height", "5 mm")],
            inputs: vec![InputSpec {
                name: "plane".into(),
                title: "Plane".into(),
                kind: InputKind::Plane,
            }],
            outputs: vec![SlotSpec {
                name: slot("body"),
                kind: SlotKind::Body,
                modifies: None,
            }],
        })
    }
}

impl FeatureType for Block {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.0
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let w = args.param("width").unwrap();
        let plane = match args.input("plane").map(|i| &i.value) {
            Some(InputValue::Plane(f)) => *f,
            _ => Frame::WORLD_XY,
        };
        let line = |k: u64, x: f64, y: f64| ProfileSegment::Line {
            key: CurveKey(Id(k)),
            to: DVec2::new(x, y),
        };
        let outer = ProfileLoop::Path {
            start: DVec2::ZERO,
            segments: vec![
                line(1, w, 0.0),
                line(2, w, w),
                line(3, 0.0, w),
                line(4, 0.0, 0.0),
            ],
        };
        let profile = Profile::new(plane, outer, vec![]).unwrap();
        let body = kernel.extrude(&profile, args.param("height").unwrap())?;
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: slot("body"),
                value: OutputValue::Body(body),
            }],
        }
        .into())
    }
}

fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    r.register(Arc::new(Block::new())).unwrap();
    r
}

fn record(
    id: u64,
    type_id: &str,
    params: &[(&str, &str)],
    inputs: &[(&str, Ref)],
) -> FeatureRecord {
    FeatureRecord {
        id: fid(id),
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: format!("f{id}"),
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), Expr::parse(v).unwrap()))
            .collect(),
        choices: BTreeMap::new(),
        inputs: inputs
            .iter()
            .map(|(k, r)| (k.to_string(), r.clone()))
            .collect(),
        suppressed: false,
        sketch: None,
    }
}

fn plane_slot(id: u64) -> Ref {
    Ref::Slot {
        feature: fid(id),
        slot: slot("plane"),
    }
}

fn face(feature: u64, part: SweepPartName) -> PersistentName {
    PersistentName {
        kind: TopoKind::Face,
        root: NameRoot::Sweep {
            feature: fid(feature),
            part,
        },
        chain: vec![],
    }
}

fn doc(params: &[(u64, &str, &str)], features: Vec<FeatureRecord>) -> Document {
    Document {
        params: params
            .iter()
            .map(|(id, name, e)| {
                let p = Param {
                    name: name.to_string(),
                    kind: QuantityKind::Length,
                    expr: Expr::parse(e).unwrap(),
                };
                (ParamId(Id(*id)), p)
            })
            .collect(),
        parts: BTreeMap::from([(
            PART,
            Part {
                id: PART,
                name: "Part".into(),
                history: features.iter().map(|f| f.id).collect(),
                features: features.into_iter().map(|f| (f.id, f)).collect(),
                rollback: None,
            },
        )]),
    }
}

/// A plane 10 mm above world XY, a block of side `w` on it, a plane 2 mm
/// above the block's top face, and a plane on world YZ.
fn scenario() -> Document {
    let top = Ref::Topo(face(2, SweepPartName::EndCap));
    let mut side = record(4, "core.datum-plane", &[("offset", "1 mm")], &[]);
    side.choices.insert("world".into(), "yz".into());
    doc(
        &[(10, "w", "20 mm"), (11, "h", "8 mm")],
        vec![
            record(1, "core.datum-plane", &[("offset", "10 mm")], &[]),
            record(
                2,
                "test.block",
                &[("width", "w"), ("height", "h")],
                &[("plane", plane_slot(1))],
            ),
            record(
                3,
                "core.datum-plane",
                &[("offset", "2 mm")],
                &[("plane", top)],
            ),
            side,
        ],
    )
}

fn origin(e: &Evaluation, feature: u64) -> DVec3 {
    match e.slot(fid(feature), "plane") {
        Some(SlotView::Plane(f)) => f.origin(),
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn failure(e: &Evaluation, feature: u64) -> &Diagnostic {
    match e.outcome(fid(feature)) {
        Some(FeatureOutcome::Failed { diagnostic, .. }) => diagnostic,
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn cached(e: &Evaluation, feature: u64) -> bool {
    match e.outcome(fid(feature)) {
        Some(FeatureOutcome::Ok { cached, .. }) => *cached,
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn close(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-12
}

#[test]
fn a_datum_plane_stands_on_a_world_plane_or_its_input() {
    let e = Evaluator::new(registry()).evaluate(&scenario());
    assert!(close(origin(&e, 1), DVec3::new(0.0, 0.0, 0.010)));
    assert!(close(origin(&e, 3), DVec3::new(0.0, 0.0, 0.020)));
    assert!(close(origin(&e, 4), DVec3::new(0.001, 0.0, 0.0)));
    match e.slot(fid(3), "plane") {
        Some(SlotView::Plane(f)) => {
            assert_eq!(f.z_axis(), DVec3::Z, "the top face's outward normal")
        }
        other => panic!("{other:?}"),
    }
    let Some(SlotView::Body(m)) = e.slot(fid(2), "body") else {
        panic!("no body")
    };
    assert!((m.volume - 0.020 * 0.020 * 0.008).abs() < 1e-15);
    assert_eq!((m.faces, m.edges, m.vertices), (6, 12, 8));
}

#[test]
fn a_datum_plane_refuses_two_bases_or_an_unknown_world_plane() {
    let mut both = record(1, "core.datum-plane", &[], &[("plane", plane_slot(9))]);
    both.choices.insert("world".into(), "xy".into());
    let mut odd = record(2, "core.datum-plane", &[], &[]);
    odd.choices.insert("world".into(), "xz".into());
    let mut extra = record(3, "core.datum-plane", &[], &[]);
    extra.choices.insert("flip".into(), "yes".into());
    let e = Evaluator::new(registry()).evaluate(&doc(&[], vec![both, odd, extra]));
    // Input resolution comes first: the missing feature 9 is a lost reference.
    assert_eq!(failure(&e, 1).code.as_str(), "ref.lost");
    assert_eq!(failure(&e, 2).code.as_str(), "datum-plane.world");
    assert_eq!(failure(&e, 3).code.as_str(), "feature.unknown-choice");
}

#[test]
fn an_unchanged_hash_is_a_cache_hit() {
    let mut ev = Evaluator::new(registry());
    let first = ev.evaluate(&scenario());
    assert!((1..=4).all(|f| !cached(&first, f)));
    let calls = ev.kernel_calls().len();
    let again = ev.evaluate(&scenario());
    assert!((1..=4).all(|f| cached(&again, f)));
    // A cache hit makes no extrude; resolving the top face queries it.
    let ops: Vec<_> = ev.kernel_calls()[calls..].iter().map(|c| &c.op).collect();
    assert!(
        ops.iter()
            .all(|op| matches!(op, KernelOp::FaceFrame { .. })),
        "{ops:?}"
    );

    let mut taller = scenario();
    taller.params.get_mut(&ParamId(Id(11))).unwrap().expr = Expr::parse("12 mm").unwrap();
    let edited = ev.evaluate(&taller);
    assert!(
        cached(&edited, 1),
        "the lower plane reads nothing that changed"
    );
    assert!(!cached(&edited, 2));
    assert!(!cached(&edited, 3), "the top face moved");
    assert!(cached(&edited, 4));
    assert!(close(origin(&edited, 3), DVec3::new(0.0, 0.0, 0.024)));
    assert_eq!(again.events[0], edited.events[0], "same hash, same outputs");
}

#[test]
fn the_hash_covers_what_the_outputs_depend_on() {
    let hash = |d: &Document, f: u64| match Evaluator::new(registry()).evaluate(d).outcome(fid(f)) {
        Some(FeatureOutcome::Ok { hash, .. }) => *hash,
        other => panic!("{other:?}"),
    };
    let base = scenario();
    let h = hash(&base, 1);
    assert_eq!(h, hash(&base, 1), "deterministic across evaluators");
    let mut renamed = base.clone();
    renamed
        .parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(1))
        .unwrap()
        .name = "Base".into();
    assert_eq!(h, hash(&renamed, 1), "a name is not an input");
    let mut same_value = base.clone();
    let f1 = same_value
        .parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(1))
        .unwrap();
    f1.params
        .insert("offset".into(), Expr::parse("1 cm").unwrap());
    assert_eq!(h, hash(&same_value, 1), "the value is hashed, not the text");
    let mut world = base.clone();
    let f1 = world
        .parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(1))
        .unwrap();
    f1.choices.insert("world".into(), "zx".into());
    assert_ne!(h, hash(&world, 1));
    let json = serde_json::to_string(&h).unwrap();
    assert_eq!(json.len(), 66);
    assert_eq!(serde_json::from_str::<InputHash>(&json).unwrap(), h);
}

#[test]
fn a_failed_feature_fails_its_dependents_and_the_rest_evaluate() {
    let mut d = scenario();
    // An angle where a length is wanted: the block fails.
    let block = d
        .parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(2))
        .unwrap();
    block
        .params
        .insert("width".into(), Expr::parse("5 deg").unwrap());
    let e = Evaluator::new(registry()).evaluate(&d);
    let block = failure(&e, 2);
    assert_eq!(block.code.as_str(), "expr.unit-mismatch");
    assert!(block.message.starts_with("`width`: "), "{}", block.message);
    assert_eq!(block.refs, [Ref::Feature(fid(2))]);
    let above = failure(&e, 3);
    assert_eq!(above.code.as_str(), "input.unavailable");
    assert_eq!(above.refs, [Ref::Feature(fid(2))]);
    assert!(close(origin(&e, 1), DVec3::new(0.0, 0.0, 0.010)));
    assert!(close(origin(&e, 4), DVec3::new(0.001, 0.0, 0.0)));
    let line = e.line("d", &d);
    assert_eq!(line.status, EvalStatus::Failed);
    let statuses: Vec<_> = line.features.iter().map(|f| f.status.as_str()).collect();
    assert_eq!(statuses, ["ok", "failed", "failed", "ok"]);
    assert_eq!(
        line.features[2].category.as_deref(),
        Some("input.unavailable")
    );
    assert!(line.bodies.is_empty());
}

#[test]
fn a_failed_parameter_makes_its_readers_unavailable() {
    let mut d = scenario();
    d.params.get_mut(&ParamId(Id(11))).unwrap().expr = Expr::parse("h2 + 1 mm").unwrap();
    let e = Evaluator::new(registry()).evaluate(&d);
    let h = e.params[&ParamId(Id(11))].as_ref().unwrap_err();
    assert_eq!(h.code.as_str(), "expr.unknown-name");
    assert_eq!(h.refs, [Ref::Param(ParamId(Id(11)))]);
    let block = failure(&e, 2);
    assert_eq!(block.code.as_str(), "input.unavailable");
    assert_eq!(block.refs, [Ref::Param(ParamId(Id(11)))]);
    let line = e.line("d", &d);
    assert_eq!(line.params[1].name, "h");
    assert!(line.params[1].value.is_none());
    assert_eq!(line.params[0].value, Some(0.020));
}

#[test]
fn a_suppressed_or_rolled_back_feature_has_no_outputs() {
    let mut d = scenario();
    d.parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(1))
        .unwrap()
        .suppressed = true;
    let e = Evaluator::new(registry()).evaluate(&d);
    assert_eq!(e.outcome(fid(1)), Some(&FeatureOutcome::Suppressed));
    assert!(failure(&e, 2).message.contains("is suppressed"));
    assert_eq!(e.line("d", &d).status, EvalStatus::Failed);

    let mut d = scenario();
    d.parts.get_mut(&PART).unwrap().rollback = Some(2);
    let e = Evaluator::new(registry()).evaluate(&d);
    assert!(cached_or_fresh(&e, 2));
    assert_eq!(e.outcome(fid(3)), Some(&FeatureOutcome::RolledBack));
    assert_eq!(e.outcome(fid(4)), Some(&FeatureOutcome::RolledBack));
    assert_eq!(e.line("d", &d).status, EvalStatus::Ok);
}

fn cached_or_fresh(e: &Evaluation, feature: u64) -> bool {
    matches!(e.outcome(fid(feature)), Some(FeatureOutcome::Ok { .. }))
}

#[test]
fn a_face_that_is_gone_is_a_lost_reference_with_ranked_candidates() {
    let mut d = scenario();
    let gone = face(2, SweepPartName::Side(CurveKey(Id(7))));
    let f3 = d
        .parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(3))
        .unwrap();
    f3.inputs.insert("plane".into(), Ref::Topo(gone.clone()));
    let e = Evaluator::new(registry()).evaluate(&d);
    let lost = failure(&e, 3);
    assert_eq!(lost.code.as_str(), "ref.lost");
    assert_eq!(lost.refs, [Ref::Topo(gone)]);
    let sides: Vec<Ref> = (1..=4)
        .map(|k| Ref::Topo(face(2, SweepPartName::Side(CurveKey(Id(k))))))
        .collect();
    assert_eq!(lost.candidates[..4], sides[..], "same kind of part first");
    assert_eq!(lost.candidates.len(), 5);
    assert!(
        cached_or_fresh(&e, 4),
        "a lost reference fails only its reader"
    );
}

#[test]
fn unknown_types_fields_and_versions_are_refused_not_ignored() {
    let mut newer = record(3, "core.datum-plane", &[], &[]);
    newer.type_version = 2;
    let e = Evaluator::new(registry()).evaluate(&doc(
        &[],
        vec![
            record(1, "gears.spur", &[], &[]),
            record(2, "core.datum-plane", &[("angle", "5 deg")], &[]),
            newer,
        ],
    ));
    assert_eq!(failure(&e, 1).code.as_str(), "feature.unknown-type");
    assert_eq!(failure(&e, 2).code.as_str(), "feature.unknown-field");
    assert_eq!(failure(&e, 3).code.as_str(), "feature.type-version");
}

#[test]
fn a_kernel_failure_keeps_its_call_record() {
    let d = doc(
        &[],
        vec![record(1, "test.block", &[("height", "0 mm")], &[])],
    );
    let e = Evaluator::new(registry()).evaluate(&d);
    let Some(FeatureOutcome::Failed { diagnostic, call }) = e.outcome(fid(1)) else {
        panic!("{:?}", e.outcome(fid(1)))
    };
    assert!(
        diagnostic.code.as_str().starts_with("kernel."),
        "{diagnostic:?}"
    );
    let call = call.as_ref().expect("the failing call is kept");
    assert!(matches!(call.op, KernelOp::Extrude { distance, .. } if distance == 0.0));
}

#[test]
fn events_cross_as_json() {
    let mut d = scenario();
    let f3 = d
        .parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(3))
        .unwrap();
    f3.inputs.insert(
        "plane".into(),
        Ref::Topo(face(2, SweepPartName::Side(CurveKey(Id(9))))),
    );
    d.parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&fid(4))
        .unwrap()
        .suppressed = true;
    let e = Evaluator::new(registry()).evaluate(&d);
    for event in &e.events {
        let json = serde_json::to_string(event).unwrap();
        assert_eq!(
            &serde_json::from_str::<EvalEvent>(&json).unwrap(),
            event,
            "{json}"
        );
    }
    let json = serde_json::to_string(&e.events[3]).unwrap();
    assert_eq!(
        json,
        r#"{"feature":"0000000000004","outcome":{"status":"suppressed"}}"#
    );
}

#[test]
fn the_cache_keeps_what_the_latest_evaluation_used_past_its_capacity() {
    let mut ev = Evaluator::new(registry());
    ev.capacity = 0;
    let base = scenario();
    ev.evaluate(&base);
    let mut taller = base.clone();
    taller.params.get_mut(&ParamId(Id(11))).unwrap().expr = Expr::parse("12 mm").unwrap();
    ev.evaluate(&taller);
    assert_eq!(ev.cache.len(), 4, "only what the latest evaluation used");
    let back = ev.evaluate(&base);
    assert!(!cached(&back, 2), "the evicted block is rebuilt");
    assert!(cached(&back, 1));
    assert!(close(origin(&back, 3), DVec3::new(0.0, 0.0, 0.020)));

    let mut roomy = Evaluator::new(registry());
    roomy.evaluate(&base);
    roomy.evaluate(&taller);
    assert!(
        cached(&roomy.evaluate(&base), 2),
        "within capacity, undo is a hit"
    );
}

#[test]
fn a_stopped_evaluation_keeps_what_it_computed_cached() {
    let mut ev = Evaluator::new(registry());
    let mut seen = 0;
    let stopped = ev.evaluate_while(&scenario(), |_| {
        seen += 1;
        seen < 2
    });
    assert!(stopped.is_none());
    assert_eq!(seen, 2, "it stops right after the event that said so");
    let e = ev.evaluate(&scenario());
    assert!(cached(&e, 1) && cached(&e, 2));
    assert!(!cached(&e, 3));
}
