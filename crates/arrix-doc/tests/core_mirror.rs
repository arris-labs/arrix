//! `core.mirror` through the evaluator: a boss on a plate mirrored about a
//! datum plane and joined, a hole cut by its mirror, and the plane moved.

use std::collections::BTreeMap;

use arrix_core::{FeatureId, Id, ParamId, PartId, QuantityKind, Ref, RegionKey, SlotName};
use arrix_doc::{
    Command, Document, Evaluation, Evaluator, FeatureEdit, FeatureOutcome, FeatureRecord,
    FeatureTypeId, Param, Part, Registry, SlotView, apply, expr::Expr,
};
use arrix_sketch::{Constraint, Draft, Entity, Point, Sketch};

const MM: f64 = 1e-3;
const PART: PartId = PartId(Id(1));
const T: ParamId = ParamId(Id(1));

const PLATE_SKETCH: u64 = 3;
const PLATE: u64 = 4;
const DATUM: u64 = 5;

fn fid(n: u64) -> FeatureId {
    FeatureId(Id(n))
}

fn plate_slot(feature: u64, name: &str) -> Ref {
    Ref::Slot {
        feature: fid(feature),
        slot: SlotName::new(name).unwrap(),
    }
}

/// An axis-aligned rectangle of `w` × `h` from (`x`, `y`), millimetres.
fn rect(x: f64, y: f64, w: f64, h: f64) -> Sketch {
    let mut d = Draft::seeded(7);
    let p = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]
        .map(|(dx, dy)| d.add_point(Point::new((x + dx) * MM, (y + dy) * MM)));
    for i in 0..4 {
        let line = d.add_entity(Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
        d.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
    }
    d.sketch
}

fn record(id: u64, type_id: &str, name: &str) -> FeatureRecord {
    FeatureRecord {
        id: fid(id),
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: name.into(),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
    }
}

fn sketch(id: u64, on: Option<u64>, sketch: Sketch) -> FeatureRecord {
    let mut r = record(id, "core.sketch", &format!("Sketch {id}"));
    r.sketch = Some(Box::new(sketch));
    if let Some(plane) = on {
        r.inputs
            .insert("plane".into(), plate_slot(plane, "plane").into());
    }
    r
}

/// The region of a sketch's only closed loop, as evaluating it gives it.
fn only_region(sketch: &Sketch) -> RegionKey {
    let doc = with(
        &Document::default(),
        Command::AddPart {
            part: Part {
                id: PART,
                name: "Part".into(),
                history: vec![fid(1)],
                features: [(fid(1), self::sketch(1, None, sketch.clone()))].into(),
                rollback: None,
            },
        },
    );
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&doc);
    match e.slot(fid(1), "sketch") {
        Some(SlotView::Sketch(v)) if v.regions.len() == 1 => v.regions[0].key.clone(),
        other => panic!("{other:?}"),
    }
}

fn with(doc: &Document, command: Command) -> Document {
    apply(doc, &command).unwrap().document
}

/// `doc` with `feature` edited.
fn edited(doc: &Document, feature: u64, edit: FeatureEdit) -> Document {
    let feature = fid(feature);
    with(doc, Command::EditFeature { feature, edit })
}

fn extrude(id: u64, sketch_id: u64, key: &RegionKey, mode: &str, distance: &str) -> FeatureRecord {
    let mut r = record(id, "core.extrude", &format!("Extrude {id}"));
    r.params
        .insert("distance".into(), Expr::parse(distance).unwrap());
    r.choices.insert("mode".into(), mode.into());
    r.inputs.insert(
        "region".into(),
        Ref::Region {
            feature: fid(sketch_id),
            key: key.clone(),
        }
        .into(),
    );
    r
}

/// The 50 × 40 × 5 mm plate centred on the origin and a datum plane on
/// its top, then `after`.
fn part(after: Vec<FeatureRecord>) -> Document {
    let plate = rect(-25.0, -20.0, 50.0, 40.0);
    let mut datum = record(DATUM, "core.datum-plane", "Top");
    datum
        .params
        .insert("offset".into(), Expr::parse("t").unwrap());
    let mut features = vec![
        sketch(PLATE_SKETCH, None, plate.clone()),
        extrude(PLATE, PLATE_SKETCH, &only_region(&plate), "new", "t"),
        datum,
    ];
    features.extend(after);
    let param = Param {
        name: "t".into(),
        kind: QuantityKind::Length,
        expr: Expr::parse("5 mm").unwrap(),
    };
    let doc = with(
        &Document::default(),
        Command::AddParam {
            param: T,
            record: param,
        },
    );
    let part = Part {
        id: PART,
        name: "Part".into(),
        history: features.iter().map(|f| f.id).collect(),
        features: features.into_iter().map(|f| (f.id, f)).collect(),
        rollback: None,
    };
    with(&doc, Command::AddPart { part })
}

fn volume(e: &Evaluation, feature: u64) -> f64 {
    match e.slot(fid(feature), "body") {
        Some(SlotView::Body(m)) => m.volume * 1e9,
        other => panic!("{:?}: {other:?}", e.outcome(fid(feature))),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-4 * b.abs().max(1.0)
}

fn code(e: &Evaluation, feature: u64) -> &str {
    match e.outcome(fid(feature)) {
        Some(FeatureOutcome::Failed { diagnostic, .. }) => diagnostic.code.as_str(),
        other => panic!("{other:?}"),
    }
}

fn eval(doc: &Document) -> Evaluation {
    Evaluator::new(Registry::with_core_types()).evaluate(doc)
}

const BOSS_SKETCH: u64 = 6;
const BOSS: u64 = 7;
const MIRROR_PLANE: u64 = 8;
const MIRROR: u64 = 9;

const PLATE_VOLUME: f64 = 50.0 * 40.0 * 5.0;

/// A datum plane at `x` along the world `yz` plane.
fn plane_at(x: &str) -> FeatureRecord {
    let mut r = record(MIRROR_PLANE, "core.datum-plane", "Mirror plane");
    r.choices.insert("world".into(), "yz".into());
    r.params.insert("offset".into(), Expr::parse(x).unwrap());
    r
}

fn mirror(tools: bool) -> FeatureRecord {
    let mut r = record(MIRROR, "core.mirror", "Mirror");
    r.inputs
        .insert("target".into(), plate_slot(PLATE, "body").into());
    r.inputs
        .insert("plane".into(), plate_slot(MIRROR_PLANE, "plane").into());
    if tools {
        r.inputs
            .insert("tools".into(), vec![Ref::Feature(fid(BOSS))].into());
    }
    r
}

/// The plate, a 10 × 10 × 3 mm boss joined at x 5 to 15 mm, the mirror
/// plane at `x`, and the mirror.
fn boss_doc(x: &str, tools: bool) -> Document {
    let boss = rect(5.0, -5.0, 10.0, 10.0);
    let key = only_region(&boss);
    let mut join = extrude(BOSS, BOSS_SKETCH, &key, "join", "3 mm");
    join.inputs
        .insert("target".into(), plate_slot(PLATE, "body").into());
    part(vec![
        sketch(BOSS_SKETCH, Some(DATUM), boss),
        join,
        plane_at(x),
        mirror(tools),
    ])
}

#[test]
fn a_boss_mirrored_is_joined() {
    // The boss, with the target the plate beneath it: the mirror of the
    // boss's tool lands at x -15 to -5, 300 mm³ more.
    let e = eval(&boss_doc("0 mm", true));
    let want = PLATE_VOLUME + 2.0 * 300.0;
    assert!(
        near(volume(&e, MIRROR), want),
        "{:?}",
        e.outcome(fid(MIRROR))
    );
    // The plane moved to x = 2 mm: the copy is at x -11 to -1.
    let e = eval(&boss_doc("2 mm", true));
    assert!(
        near(volume(&e, MIRROR), want),
        "{:?}",
        e.outcome(fid(MIRROR))
    );
    // At x = 7 mm the copy overlaps the boss: x -1 to 15 mm, 16 × 10 × 3.
    let e = eval(&boss_doc("7 mm", true));
    let want = PLATE_VOLUME + 16.0 * 10.0 * 3.0;
    assert!(
        near(volume(&e, MIRROR), want),
        "{:?}",
        e.outcome(fid(MIRROR))
    );
}

#[test]
fn a_body_mirrored_is_fused_to_itself() {
    // The plate with its boss reflected about its end face, x = 25 mm: a
    // second plate and boss, touching the first, fused to it.
    let e = eval(&boss_doc("25 mm", false));
    let want = 2.0 * (PLATE_VOLUME + 300.0);
    assert!(
        near(volume(&e, MIRROR), want),
        "{:?} / {want}",
        e.outcome(fid(MIRROR))
    );
    // A plane far from the body: the reflection misses it.
    let e = eval(&boss_doc("500 mm", false));
    assert_eq!(code(&e, MIRROR), "mirror.misses");
}

#[test]
fn a_mirror_needs_its_target_and_plane() {
    let doc = boss_doc("0 mm", true);
    for (input, want) in [("target", "mirror.no-target"), ("plane", "mirror.no-plane")] {
        let edit = FeatureEdit {
            inputs: BTreeMap::from([(input.to_string(), None)]),
            ..FeatureEdit::default()
        };
        let e = eval(&edited(&doc, MIRROR, edit));
        assert_eq!(code(&e, MIRROR), want);
    }
}
