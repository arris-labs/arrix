//! Persistent naming through a plugin feature (docs/DATA-MODEL.md
//! §Persistent naming): datum planes on the faces `gears.spur` builds,
//! referenced by name, through parameter edits. Beside the registration
//! list that names the plugin, as tests/gears.rs is.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_core::{
    CurveKey, DVec3, Diagnostic, FeatureId, Id, NameRoot, ParamId, PartId, PersistentName,
    QuantityKind, Ref, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Authority, Command, CommandEnvelope, Evaluation, Evaluator, FeatureOutcome,
    FeatureRecord, FeatureTypeId, Outcome, Param, Part, Registry, SlotView, expr::Expr,
};
use arrix_plugin_host::register_tier0;

const MM: f64 = 1e-3;
const ME: AuthorId = AuthorId(Id(1));
const TEETH: ParamId = ParamId(Id(10));
const W: ParamId = ParamId(Id(12));
const BASE: FeatureId = FeatureId(Id(1));
const GEAR: FeatureId = FeatureId(Id(2));
const TOP: FeatureId = FeatureId(Id(3));
const FLANK: FeatureId = FeatureId(Id(4));

fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears)).unwrap();
    r
}

fn gear_face(part: SweepPartName) -> PersistentName {
    PersistentName {
        kind: TopoKind::Face,
        root: NameRoot::Sweep {
            feature: GEAR,
            part,
        },
        chain: vec![],
    }
}

/// Tooth 20's rising radial flank: a plane, below the base circle.
fn tooth_20_flank() -> PersistentName {
    gear_face(SweepPartName::Side(CurveKey(Id(20_001))))
}

fn record(id: FeatureId, ty: &str, params: &[(&str, &str)], input: Option<Ref>) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new(ty).unwrap(),
        type_version: 1,
        name: format!("f{id}"),
        params: params
            .iter()
            .map(|(k, v)| (k.to_string(), Expr::parse(v).unwrap()))
            .collect(),
        choices: BTreeMap::new(),
        inputs: input.into_iter().map(|r| ("plane".into(), r)).collect(),
        suppressed: false,
        sketch: None,
        frozen: None,
    }
}

fn submit(a: &mut Authority, command: Command) {
    let envelope = CommandEnvelope {
        author: ME,
        base: a.generation(),
        command,
    };
    assert!(matches!(a.submit(&envelope), Ok(Outcome::Applied(_))));
}

fn set(a: &mut Authority, param: ParamId, expr: &str) {
    let expr = Expr::parse(expr).unwrap();
    submit(a, Command::SetParam { param, expr });
}

/// `teeth = 20`, `m = 1 mm`, `w = 8 mm`; a datum plane 10 mm above world
/// XY; a spur gear on it; a datum plane 2 mm above the gear's top face and
/// one on tooth 20's flank, each by persistent name.
fn build() -> Authority {
    let mut a = Authority::default();
    let params = [
        (TEETH, "teeth", QuantityKind::Count, "20"),
        (ParamId(Id(11)), "m", QuantityKind::Length, "1 mm"),
        (W, "w", QuantityKind::Length, "8 mm"),
    ];
    for (param, name, kind, expr) in params {
        let record = Param {
            name: name.into(),
            kind,
            expr: Expr::parse(expr).unwrap(),
        };
        submit(&mut a, Command::AddParam { param, record });
    }
    let part = PartId(Id(100));
    let empty = Part {
        id: part,
        name: "Part".into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
    };
    submit(&mut a, Command::AddPart { part: empty });
    let on_base = Ref::Slot {
        feature: BASE,
        slot: SlotName::new("plane").unwrap(),
    };
    let top = Ref::Topo(gear_face(SweepPartName::EndCap));
    let features = [
        record(BASE, "core.datum-plane", &[("offset", "10 mm")], None),
        record(
            GEAR,
            "gears.spur",
            &[("teeth", "teeth"), ("module", "m"), ("width", "w")],
            Some(on_base),
        ),
        record(TOP, "core.datum-plane", &[("offset", "2 mm")], Some(top)),
        record(
            FLANK,
            "core.datum-plane",
            &[("offset", "1 mm")],
            Some(Ref::Topo(tooth_20_flank())),
        ),
    ];
    for (at, record) in features.into_iter().enumerate() {
        submit(&mut a, Command::AddFeature { part, at, record });
    }
    a
}

fn plane(e: &Evaluation, feature: FeatureId) -> arrix_core::Frame {
    match e.slot(feature, "plane") {
        Some(SlotView::Plane(f)) => *f,
        _ => panic!("feature {feature}: {:?}", e.outcome(feature)),
    }
}

fn cached(e: &Evaluation, feature: FeatureId) -> bool {
    match e.outcome(feature) {
        Some(FeatureOutcome::Ok { cached, .. }) => *cached,
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn failure(e: &Evaluation, feature: FeatureId) -> &Diagnostic {
    match e.outcome(feature) {
        Some(FeatureOutcome::Failed { diagnostic, .. }) => diagnostic,
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-12
}

#[test]
fn a_plane_on_the_gears_top_face_follows_width_edits_and_survives_a_teeth_edit() {
    let mut a = build();
    let mut ev = Evaluator::new(registry());
    let e = ev.evaluate(a.document());
    let top = plane(&e, TOP);
    assert!(close(top.origin().z, 20.0 * MM), "{top:?}");
    assert!((top.z_axis() - DVec3::Z).length() < 1e-12, "{top:?}");

    set(&mut a, W, "12 mm");
    let e = ev.evaluate(a.document());
    assert!(
        cached(&e, BASE),
        "the lower plane reads nothing that changed"
    );
    assert!(!cached(&e, GEAR) && !cached(&e, TOP));
    let moved = plane(&e, TOP).origin() - top.origin();
    assert!((moved - DVec3::new(0.0, 0.0, 4.0 * MM)).length() < 1e-12);

    set(&mut a, TEETH, "24");
    let e = ev.evaluate(a.document());
    assert!(!cached(&e, GEAR));
    // Resolved to the same frame, so the plane's input hash is unchanged.
    assert!(close(plane(&e, TOP).origin().z, 24.0 * MM));
    assert!(!cached(&e, FLANK), "tooth 20 is still there at 24 teeth");
}

#[test]
fn a_flank_whose_tooth_is_gone_is_a_lost_reference_never_a_rebind() {
    let mut a = build();
    let mut ev = Evaluator::new(registry());
    let before = plane(&ev.evaluate(a.document()), FLANK);

    set(&mut a, TEETH, "18");
    let e = ev.evaluate(a.document());
    let lost = failure(&e, FLANK);
    assert_eq!(lost.code.as_str(), "ref.lost");
    assert_eq!(lost.refs, [Ref::Topo(tooth_20_flank())]);
    assert!(!lost.candidates.is_empty());
    for c in &lost.candidates {
        let Ref::Topo(name) = c else {
            panic!("a candidate is a name: {c:?}")
        };
        assert!(
            matches!(
                name.root,
                NameRoot::Sweep {
                    feature: GEAR,
                    part: SweepPartName::Side(_)
                }
            ),
            "the same feature's side faces rank first: {name}"
        );
    }
    // Candidates are offered, never applied: the record still names tooth 20.
    let (_, _, record) = a.document().feature(FLANK).unwrap();
    assert_eq!(record.inputs["plane"], Ref::Topo(tooth_20_flank()));
    for f in [BASE, GEAR, TOP] {
        evaluated(&e, f);
    }

    // Back to 20 teeth, the same name resolves to the same place again.
    assert!(matches!(a.undo(ME), Ok(Outcome::Applied(_))));
    assert_eq!(plane(&ev.evaluate(a.document()), FLANK), before);
}

/// The features that do not read tooth 20 evaluate.
fn evaluated(e: &Evaluation, feature: FeatureId) {
    assert!(
        matches!(e.outcome(feature), Some(FeatureOutcome::Ok { .. })),
        "feature {feature}: {:?}",
        e.outcome(feature)
    );
}
