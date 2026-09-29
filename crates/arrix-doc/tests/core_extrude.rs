//! `core.extrude` through the evaluator: a sketched plate extruded to a new
//! body, a boss joined to it and a pocket cut through it, each a version of
//! the plate's slot, with volumes worked out by hand.

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
const LOW: u64 = 2;
const PLATE_SKETCH: u64 = 3;
const PLATE: u64 = 4;
const BOSS_SKETCH: u64 = 5;
const BOSS: u64 = 6;
const POCKET_SKETCH: u64 = 7;
const POCKET: u64 = 8;

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
    if mode != "new" {
        r.inputs.insert("target".into(), plate_slot(PLATE, "body"));
    }
    r
}

/// A 40 × 30 mm plate `t` thick, a 10 × 10 mm boss on its top plane rising
/// 10 mm, and a 4 × 4 mm pocket cut through it from below.
fn part() -> Document {
    let plate = rect(0.0, 0.0, 40.0, 30.0);
    let boss = rect(5.0, 5.0, 10.0, 10.0);
    let pocket = rect(20.0, 20.0, 4.0, 4.0);
    let features = vec![
        datum(TOP, "t"),
        datum(LOW, "-1 mm"),
        sketch(PLATE_SKETCH, None, plate.clone()),
        extrude(PLATE, PLATE_SKETCH, &only_region(&plate), "new", "t"),
        sketch(BOSS_SKETCH, Some(TOP), boss.clone()),
        extrude(BOSS, BOSS_SKETCH, &only_region(&boss), "join", "10 mm"),
        sketch(POCKET_SKETCH, Some(LOW), pocket.clone()),
        extrude(POCKET, POCKET_SKETCH, &only_region(&pocket), "cut", "7 mm"),
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

fn cached(e: &Evaluation, feature: u64) -> bool {
    match e.outcome(fid(feature)) {
        Some(FeatureOutcome::Ok { cached, .. }) => *cached,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_plate_a_joined_boss_and_a_cut_pocket_each_a_version_of_the_plate() {
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&part());
    assert!(near(volume(&e, PLATE), 40.0 * 30.0 * 5.0));
    // The boss stands on the plate's top: its 10 × 10 × 10 mm is added.
    assert!(
        near(volume(&e, BOSS), 6000.0 + 1000.0),
        "{}",
        volume(&e, BOSS)
    );
    // The pocket passes 4 × 4 mm through the 5 mm plate, clear of the boss.
    assert!(
        near(volume(&e, POCKET), 7000.0 - 80.0),
        "{}",
        volume(&e, POCKET)
    );
}

#[test]
fn nothing_upstream_changed_is_a_cache_hit_and_a_thicker_plate_moves_the_rest() {
    let mut ev = Evaluator::new(Registry::with_core_types());
    let doc = part();
    let first = ev.evaluate(&doc);
    assert!((1..=8).all(|f| !cached(&first, f)));
    let again = ev.evaluate(&doc);
    assert!((1..=8).all(|f| cached(&again, f)));

    let thicker = with(
        &doc,
        Command::SetParam {
            param: T,
            expr: Expr::parse("6 mm").unwrap(),
        },
    );
    let e = ev.evaluate(&thicker);
    // The pocket now cuts 6 mm of plate; the boss rides the top plane up.
    assert!(near(volume(&e, PLATE), 40.0 * 30.0 * 6.0));
    assert!(near(volume(&e, BOSS), 7200.0 + 1000.0));
    assert!(near(volume(&e, POCKET), 8200.0 - 4.0 * 4.0 * 6.0));
    assert!(cached(&e, LOW) && cached(&e, PLATE_SKETCH) && cached(&e, POCKET_SKETCH));
    assert!(!cached(&e, PLATE) && !cached(&e, BOSS) && !cached(&e, POCKET));
}

#[test]
fn join_and_cut_need_a_body_target_and_new_takes_none() {
    let inputs = |target: Option<Ref>| FeatureEdit {
        inputs: BTreeMap::from([("target".to_string(), target)]),
        ..FeatureEdit::default()
    };
    let mode = |mode: &str| FeatureEdit {
        choices: BTreeMap::from([("mode".to_string(), Some(mode.to_string()))]),
        ..FeatureEdit::default()
    };
    // (the edit, the boss's failure, whether the boss is still a version of
    // the plate's slot by its record, so that the pocket waits on it)
    for (edit, want, modifies) in [
        (inputs(None), "extrude.no-target", false),
        (
            inputs(Some(plate_slot(TOP, "plane"))),
            "input.wrong-kind",
            false,
        ),
        (mode("new"), "extrude.stray-target", true),
        (mode("sweep"), "extrude.mode", true),
    ] {
        let doc = edited(&part(), BOSS, edit);
        let e = Evaluator::new(Registry::with_core_types()).evaluate(&doc);
        assert_eq!(code(&e, BOSS), want);
        if modifies {
            // The pocket is not cut from an earlier version of the plate
            // behind a failed modifier's back.
            assert_eq!(code(&e, POCKET), "input.unavailable");
        } else {
            // A boss that names no plate body is no version of it.
            assert!(near(volume(&e, POCKET), 6000.0 - 80.0));
        }
    }
}

#[test]
fn a_joined_extrudes_own_slot_is_not_a_body_to_name() {
    // The boss (join) has no slot of its own: its `body` is the plate's
    // next version, and a target names the feature that made the slot.
    let target = Some(plate_slot(BOSS, "body"));
    let edit = FeatureEdit {
        inputs: BTreeMap::from([("target".to_string(), target)]),
        ..FeatureEdit::default()
    };
    let doc = edited(&part(), POCKET, edit);
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&doc);
    assert_eq!(code(&e, POCKET), "ref.lost");
}
