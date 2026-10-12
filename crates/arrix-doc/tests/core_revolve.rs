//! `core.revolve` through the evaluator: a rectangle turned about a
//! sketched line, new, joined to or cut from a plate, with volumes worked
//! out by hand.

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
const PROFILE_SKETCH: u64 = 5;
const AXIS_SKETCH: u64 = 6;
const REVOLVE: u64 = 7;

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
        file: None,
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
                material: None,
                body_materials: Default::default(),
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

/// `sketch` on the world YZ plane, where a sketch's `v` is the world Z.
fn upright(mut sketch: FeatureRecord) -> FeatureRecord {
    sketch.choices.insert("world".into(), "yz".into());
    sketch
}

/// A sketch holding one line, `(x0, y0)` to `(x1, y1)` in millimetres, and
/// that line's id.
fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> (Sketch, arrix_core::SketchEntityId) {
    let mut d = Draft::seeded(9);
    let a = d.add_point(Point::new(x0 * MM, y0 * MM));
    let b = d.add_point(Point::new(x1 * MM, y1 * MM));
    let id = d.add_entity(Entity::Line { start: a, end: b });
    (d.sketch, id)
}

fn revolve(mode: &str, angle: Option<&str>) -> FeatureRecord {
    let mut r = record(REVOLVE, "core.revolve", "Revolve");
    if let Some(angle) = angle {
        r.params.insert("angle".into(), Expr::parse(angle).unwrap());
    }
    r.choices.insert("mode".into(), mode.into());
    r
}

/// A 60 × 60 mm plate `t` = 5 mm thick on the world XY plane, a sketch
/// on YZ holding a `profile` rectangle and one holding the axis, the world Z
/// line, and a revolve of the two, by `mode` and `angle`.
fn part(profile: Sketch, mode: &str, angle: Option<&str>) -> Document {
    let plate = rect(-30.0, -30.0, 60.0, 60.0);
    let (axis, line) = line(0.0, 0.0, 0.0, 10.0);
    let mut revolve = revolve(mode, angle);
    revolve.inputs.insert(
        "region".into(),
        Ref::Region {
            feature: fid(PROFILE_SKETCH),
            key: only_region(&profile),
        }
        .into(),
    );
    revolve.inputs.insert(
        "axis".into(),
        Ref::Sketch {
            feature: fid(AXIS_SKETCH),
            entity: line,
        }
        .into(),
    );
    if mode != "new" {
        revolve
            .inputs
            .insert("target".into(), plate_slot(PLATE, "body").into());
    }
    let features = vec![
        sketch(PLATE_SKETCH, None, plate.clone()),
        extrude(PLATE, PLATE_SKETCH, &only_region(&plate), "new", "t"),
        upright(sketch(PROFILE_SKETCH, None, profile)),
        upright(sketch(AXIS_SKETCH, None, axis)),
        revolve,
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
        material: None,
        body_materials: Default::default(),
    };
    with(&doc, Command::AddPart { part })
}

/// The body slot's volume in cubic millimetres.
fn volume(e: &Evaluation, feature: u64) -> f64 {
    match e.slot(fid(feature), "body") {
        Some(SlotView::Body(m)) => m.volume * 1e9,
        other => panic!("{:?}: {other:?}", e.outcome(fid(feature))),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-5 * b.abs().max(1.0)
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

/// A 10 × 10 mm profile from the axis outward: a cylinder of radius 10 mm
/// and height 10 mm along Z, from the plate's bottom up.
fn cylinder_profile() -> Sketch {
    rect(0.0, 0.0, 10.0, 10.0)
}

#[test]
fn a_full_turn_makes_a_cylinder() {
    let e = eval(&part(cylinder_profile(), "new", None));
    let want = std::f64::consts::PI * 1000.0;
    assert!(near(volume(&e, REVOLVE), want), "{}", volume(&e, REVOLVE));
}

#[test]
fn a_partial_turn_makes_its_share() {
    let e = eval(&part(cylinder_profile(), "new", Some("180 deg")));
    let want = std::f64::consts::PI * 500.0;
    assert!(near(volume(&e, REVOLVE), want), "{}", volume(&e, REVOLVE));
    let quarter = eval(&part(cylinder_profile(), "new", Some("90 deg")));
    assert!(near(volume(&quarter, REVOLVE), want / 2.0));
}

#[test]
fn joined_it_stands_on_the_plate_and_cut_leaves_a_pocket() {
    let cylinder = std::f64::consts::PI * 1000.0;
    // From the plate's top, upward.
    let e = eval(&part(rect(0.0, 5.0, 10.0, 10.0), "join", None));
    let want = 18000.0 + cylinder;
    assert!(near(volume(&e, REVOLVE), want), "{}", volume(&e, REVOLVE));
    // From 1 mm above the bottom: 4 mm of it is in the plate.
    let e = eval(&part(rect(0.0, 1.0, 10.0, 10.0), "cut", None));
    let want = 18000.0 - std::f64::consts::PI * 100.0 * 4.0;
    assert!(near(volume(&e, REVOLVE), want), "{}", volume(&e, REVOLVE));
    // Through the plate, a hole.
    let e = eval(&part(rect(0.0, -1.0, 10.0, 10.0), "cut", None));
    let want = 18000.0 - std::f64::consts::PI * 100.0 * 5.0;
    assert!(near(volume(&e, REVOLVE), want), "{}", volume(&e, REVOLVE));
}

#[test]
fn an_axis_through_the_region_is_the_kernels_refusal_with_its_record() {
    // The profile straddles the axis: the kernel refuses.
    let e = eval(&part(rect(-5.0, 0.0, 10.0, 10.0), "new", None));
    let Some(FeatureOutcome::Failed {
        diagnostic, call, ..
    }) = e.outcome(fid(REVOLVE))
    else {
        panic!("{:?}", e.outcome(fid(REVOLVE)));
    };
    assert!(
        diagnostic.code.as_str().starts_with("kernel."),
        "{diagnostic:?}"
    );
    assert!(call.is_some(), "the failed call is kept");
}

#[test]
fn a_revolve_needs_its_region_axis_and_a_sensible_angle_and_mode() {
    let clear = |input: &str| FeatureEdit {
        inputs: BTreeMap::from([(input.to_string(), None)]),
        ..FeatureEdit::default()
    };
    let param = |expr: &str| FeatureEdit {
        params: BTreeMap::from([("angle".to_string(), Some(Expr::parse(expr).unwrap()))]),
        ..FeatureEdit::default()
    };
    let choice = |name: &str, value: &str| FeatureEdit {
        choices: BTreeMap::from([(name.to_string(), Some(value.to_string()))]),
        ..FeatureEdit::default()
    };
    let target = FeatureEdit {
        inputs: BTreeMap::from([("target".to_string(), Some(plate_slot(PLATE, "body").into()))]),
        ..FeatureEdit::default()
    };
    let wrong_axis = FeatureEdit {
        inputs: BTreeMap::from([("axis".to_string(), Some(plate_slot(PLATE, "body").into()))]),
        ..FeatureEdit::default()
    };
    for (edit, want) in [
        (clear("region"), "revolve.no-region"),
        (clear("axis"), "revolve.no-axis"),
        (param("0 deg"), "revolve.angle"),
        (param("400 deg"), "revolve.angle"),
        (choice("mode", "spin"), "feature.choice"),
        (choice("mode", "join"), "revolve.no-target"),
        (choice("depth", "1"), "feature.unknown-field"),
        (target, "revolve.stray-target"),
        (wrong_axis, "input.wrong-kind"),
    ] {
        let doc = edited(&part(cylinder_profile(), "new", None), REVOLVE, edit);
        assert_eq!(code(&eval(&doc), REVOLVE), want);
    }
}
