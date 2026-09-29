//! `core.boolean` through the evaluator: a plate and a separate boss fused
//! or cut, the target's slot modified and the tool's consumed.

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

const TOP: u64 = 1;
const PLATE_SKETCH: u64 = 3;
const PLATE: u64 = 4;
const BOSS_SKETCH: u64 = 5;
const BOSS: u64 = 6;
const BOOLEAN: u64 = 7;
const AGAIN: u64 = 8;

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

fn datum(id: u64, offset: &str) -> FeatureRecord {
    let mut r = record(id, "core.datum-plane", &format!("Plane {id}"));
    r.params
        .insert("offset".into(), Expr::parse(offset).unwrap());
    r
}

fn sketch(id: u64, on: Option<u64>, sketch: Sketch) -> FeatureRecord {
    let mut r = record(id, "core.sketch", &format!("Sketch {id}"));
    r.sketch = Some(Box::new(sketch));
    if let Some(plane) = on {
        r.inputs.insert("plane".into(), plate_slot(plane, "plane"));
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
        },
    );
    r
}

/// A `core.boolean` of the plate's slot with `tool`'s body, by `op`.
fn boolean(id: u64, op: &str, tool: u64) -> FeatureRecord {
    let mut r = record(id, "core.boolean", &format!("Boolean {id}"));
    r.choices.insert("op".into(), op.into());
    r.inputs.insert("target".into(), plate_slot(PLATE, "body"));
    r.inputs.insert("tool".into(), plate_slot(tool, "body"));
    r
}

/// A 40 × 30 mm plate `t` thick and a separate 10 × 10 mm boss standing on
/// its top plane, rising 10 mm, then a boolean of the two.
fn part(op: &str) -> Document {
    let plate = rect(0.0, 0.0, 40.0, 30.0);
    let boss = rect(5.0, 5.0, 10.0, 10.0);
    let features = vec![
        datum(TOP, "t"),
        sketch(PLATE_SKETCH, None, plate.clone()),
        extrude(PLATE, PLATE_SKETCH, &only_region(&plate), "new", "t"),
        sketch(BOSS_SKETCH, Some(TOP), boss.clone()),
        extrude(BOSS, BOSS_SKETCH, &only_region(&boss), "new", "10 mm"),
        boolean(BOOLEAN, op, BOSS),
    ];
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

/// A body slot's volume in cubic millimetres.
fn volume(e: &Evaluation, feature: u64) -> f64 {
    match e.slot(fid(feature), "body") {
        Some(SlotView::Body(m)) => m.volume * 1e9,
        other => panic!("{:?}: {other:?}", e.outcome(fid(feature))),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6 * b.abs().max(1.0)
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

#[test]
fn a_fused_boss_adds_its_volume_to_the_plate() {
    let e = eval(&part("fuse"));
    assert!(near(volume(&e, PLATE), 6000.0));
    assert!(near(volume(&e, BOSS), 1000.0));
    // The boss touches the plate along a face and the fuse joins them.
    assert!(
        near(volume(&e, BOOLEAN), 6000.0 + 1000.0),
        "{}",
        volume(&e, BOOLEAN)
    );
}

#[test]
fn a_cut_boss_leaves_the_plate_whole_when_it_only_touches() {
    let e = eval(&part("cut"));
    assert!(near(volume(&e, BOOLEAN), 6000.0), "{}", volume(&e, BOOLEAN));
}

#[test]
fn the_tools_slot_is_consumed_and_the_targets_moves_on() {
    let mut doc = part("fuse");
    let again = boolean(AGAIN, "fuse", BOSS);
    doc = with(
        &doc,
        Command::AddFeature {
            part: PART,
            at: 6,
            record: again,
        },
    );
    let e = eval(&doc);
    assert!(near(volume(&e, BOOLEAN), 7000.0));
    assert_eq!(code(&e, AGAIN), "slot.consumed");
}

#[test]
fn a_boolean_needs_two_bodies_and_a_known_op() {
    let clear = |input: &str| FeatureEdit {
        inputs: BTreeMap::from([(input.to_string(), None)]),
        ..FeatureEdit::default()
    };
    let op = |op: &str| FeatureEdit {
        choices: BTreeMap::from([("op".to_string(), Some(op.to_string()))]),
        ..FeatureEdit::default()
    };
    let wrong_kind = FeatureEdit {
        inputs: BTreeMap::from([("tool".to_string(), Some(plate_slot(TOP, "plane")))]),
        ..FeatureEdit::default()
    };
    for (edit, want) in [
        (clear("tool"), "boolean.no-body"),
        (clear("target"), "boolean.no-body"),
        (op("common"), "boolean.op"),
        (wrong_kind, "input.wrong-kind"),
    ] {
        let e = eval(&edited(&part("fuse"), BOOLEAN, edit));
        assert_eq!(code(&e, BOOLEAN), want);
    }
}
