//! `core.hole` through the evaluator: points of a sketch on a plate's top
//! plane, each kind and extent of hole, with volumes worked out by hand and
//! the failures a hole can end in.

use std::collections::BTreeMap;

use arrix_core::{
    CurveKey, FeatureId, Id, NameRoot, NameStep, ParamId, PartId, PersistentName, QuantityKind,
    Ref, RegionKey, SlotName, SweepPartName, TopoKind,
};
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
const POINTS: u64 = 6;
const HOLE: u64 = 7;

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

/// A sketch of lone points at the given millimetres, and their ids.
fn points(at: &[(f64, f64)]) -> (Sketch, Vec<arrix_sketch::PointId>) {
    let mut d = Draft::seeded(9);
    let ids = at
        .iter()
        .map(|(x, y)| d.add_point(Point::new(x * MM, y * MM)))
        .collect();
    (d.sketch, ids)
}

/// A `core.hole` record on the plate and the points of `POINTS`.
fn hole(
    ids: &[arrix_sketch::PointId],
    params: &[(&str, &str)],
    choices: &[(&str, &str)],
) -> FeatureRecord {
    let mut r = record(HOLE, "core.hole", "Hole");
    for (name, value) in params {
        r.params.insert((*name).into(), Expr::parse(value).unwrap());
    }
    for (name, value) in choices {
        r.choices.insert((*name).into(), (*value).into());
    }
    r.inputs
        .insert("target".into(), plate_slot(PLATE, "body").into());
    let at: Vec<Ref> = ids
        .iter()
        .map(|entity| Ref::Sketch {
            feature: fid(POINTS),
            entity: *entity,
        })
        .collect();
    r.inputs.insert("points".into(), at.into());
    r
}

/// The 40 × 30 × 5 mm plate, a datum plane on its top, a sketch of `at`
/// on that plane, and `hole` after them.
fn part(
    at: &[(f64, f64)],
    hole: impl FnOnce(&[arrix_sketch::PointId]) -> FeatureRecord,
) -> Document {
    let plate = rect(0.0, 0.0, 40.0, 30.0);
    let (pts, ids) = points(at);
    let mut datum = record(DATUM, "core.datum-plane", "Top");
    datum
        .params
        .insert("offset".into(), Expr::parse("t").unwrap());
    let features = vec![
        sketch(PLATE_SKETCH, None, plate.clone()),
        extrude(PLATE, PLATE_SKETCH, &only_region(&plate), "new", "t"),
        datum,
        sketch(POINTS, Some(DATUM), pts),
        hole(&ids),
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

/// The plate's volume less what holes of `params` and `choices` remove at
/// `at`.
fn drilled(at: &[(f64, f64)], params: &[(&str, &str)], choices: &[(&str, &str)]) -> f64 {
    let doc = part(at, |ids| hole(ids, params, choices));
    volume(&eval(&doc), HOLE)
}

const PI: f64 = std::f64::consts::PI;

#[test]
fn a_simple_hole_blind_and_through() {
    let one = [(20.0, 15.0)];
    let blind = drilled(&one, &[("diameter", "5 mm"), ("depth", "3 mm")], &[]);
    assert!(near(blind, 6000.0 - PI * 6.25 * 3.0), "{blind}");
    let through = drilled(&one, &[("diameter", "5 mm")], &[("extent", "through")]);
    assert!(near(through, 6000.0 - PI * 6.25 * 5.0), "{through}");
    // A blind hole deeper than the plate goes through it too.
    let deep = drilled(&one, &[("diameter", "5 mm"), ("depth", "9 mm")], &[]);
    assert!(near(deep, through), "{deep}");
}

#[test]
fn a_counterbore_and_a_countersink_blind_and_through() {
    let one = [(20.0, 15.0)];
    let bore = [
        ("diameter", "5 mm"),
        ("cb-diameter", "10 mm"),
        ("cb-depth", "2 mm"),
    ];
    let want = |depth: f64| 6000.0 - PI * (25.0 * 2.0 + 6.25 * (depth - 2.0));
    let blind = drilled(
        &one,
        &[bore[0], bore[1], bore[2], ("depth", "4 mm")],
        &[("kind", "counterbore")],
    );
    assert!(near(blind, want(4.0)), "{blind}");
    let through = drilled(
        &one,
        &bore,
        &[("kind", "counterbore"), ("extent", "through")],
    );
    assert!(near(through, want(5.0)), "{through}");

    // 90° across 9 mm narrows to 5 mm in 2 mm.
    let sink = [
        ("diameter", "5 mm"),
        ("cs-diameter", "9 mm"),
        ("cs-angle", "90 deg"),
    ];
    let frustum = PI * 2.0 / 3.0 * (4.5 * 4.5 + 4.5 * 2.5 + 2.5 * 2.5);
    let want = |depth: f64| 6000.0 - frustum - PI * 6.25 * (depth - 2.0);
    let blind = drilled(
        &one,
        &[sink[0], sink[1], sink[2], ("depth", "4 mm")],
        &[("kind", "countersink")],
    );
    assert!(near(blind, want(4.0)), "{blind}");
    let through = drilled(
        &one,
        &sink,
        &[("kind", "countersink"), ("extent", "through")],
    );
    assert!(near(through, want(5.0)), "{through}");
}

#[test]
fn every_point_is_a_hole() {
    let at = [(10.0, 8.0), (20.0, 15.0), (30.0, 22.0)];
    let v = drilled(&at, &[("diameter", "4 mm"), ("depth", "3 mm")], &[]);
    assert!(near(v, 6000.0 - 3.0 * PI * 4.0 * 3.0), "{v}");
}

#[test]
fn a_hole_moved_removes_the_same_and_a_point_off_the_body_cuts_nothing() {
    let params = [("diameter", "4 mm"), ("depth", "3 mm")];
    let a = drilled(&[(10.0, 8.0)], &params, &[]);
    let b = drilled(&[(30.0, 22.0)], &params, &[]);
    assert!(near(a, b));
    let doc = part(&[(100.0, 8.0)], |ids| hole(ids, &params, &[]));
    assert_eq!(code(&eval(&doc), HOLE), "hole.misses");
}

#[test]
fn a_hole_needs_its_target_and_points_and_sane_sizes() {
    let at = [(20.0, 15.0)];
    let clear = |input: &str| FeatureEdit {
        inputs: BTreeMap::from([(input.to_string(), None)]),
        ..FeatureEdit::default()
    };
    let choice = |name: &str, value: &str| FeatureEdit {
        choices: BTreeMap::from([(name.to_string(), Some(value.to_string()))]),
        ..FeatureEdit::default()
    };
    let param = |name: &str, value: &str| FeatureEdit {
        params: BTreeMap::from([(name.to_string(), Some(Expr::parse(value).unwrap()))]),
        ..FeatureEdit::default()
    };
    let doc = part(&at, |ids| hole(ids, &[], &[]));
    assert!(volume(&eval(&doc), HOLE) < 6000.0, "the defaults drill");
    for (edit, want) in [
        (clear("target"), "hole.no-target"),
        (clear("points"), "hole.no-points"),
        (choice("extent", "x"), "feature.choice"),
        (choice("kind", "x"), "feature.choice"),
        (choice("mode", "x"), "feature.unknown-field"),
        (param("diameter", "0 mm"), "hole.diameter"),
        (param("depth", "0 mm"), "hole.depth"),
    ] {
        let e = eval(&edited(&doc, HOLE, edit));
        assert_eq!(code(&e, HOLE), want);
    }
    let bore = edited(&doc, HOLE, choice("kind", "counterbore"));
    let e = eval(&edited(&bore, HOLE, param("cb-diameter", "4 mm")));
    assert_eq!(code(&e, HOLE), "hole.counterbore");
    let sink = edited(&doc, HOLE, choice("kind", "countersink"));
    let e = eval(&edited(&sink, HOLE, param("cs-angle", "180 deg")));
    assert_eq!(code(&e, HOLE), "hole.countersink");
}

/// A datum plane on the face `name` names, offset 0, as feature 8.
fn on_face(doc: &Document, name: PersistentName) -> Document {
    let mut datum = record(8, "core.datum-plane", "On face");
    datum
        .params
        .insert("offset".into(), Expr::parse("0 mm").unwrap());
    datum.inputs.insert("plane".into(), Ref::Topo(name).into());
    with(
        doc,
        Command::AddFeature {
            part: PART,
            at: 5,
            record: datum,
        },
    )
}

/// The floor of hole `index`'s counterbore: its tool's face from the
/// profile's third curve, as the cut left it.
fn counterbore_floor(index: u32) -> PersistentName {
    let tool = PersistentName {
        kind: TopoKind::Face,
        root: NameRoot::Revolve {
            feature: fid(HOLE),
            index,
            part: SweepPartName::Side(CurveKey(Id(3))),
        },
        chain: vec![],
    };
    PersistentName {
        chain: vec![NameStep::Generated {
            feature: fid(HOLE),
            boolean: index,
            from: vec![tool.clone()],
        }],
        ..tool
    }
}

#[test]
fn a_hole_moved_keeps_its_faces_names() {
    let params = [
        ("diameter", "4 mm"),
        ("cb-diameter", "8 mm"),
        ("cb-depth", "2 mm"),
        ("depth", "4 mm"),
    ];
    for at in [(20.0, 15.0), (12.0, 10.0)] {
        let doc = part(&[at], |ids| hole(ids, &params, &[("kind", "counterbore")]));
        let doc = on_face(&doc, counterbore_floor(0));
        let e = eval(&doc);
        let Some(SlotView::Plane(floor)) = e.slot(fid(8), "plane") else {
            panic!("{:?}", e.outcome(fid(8)))
        };
        // The counterbore's floor, 2 mm below the plate's top, facing up.
        assert!(near(floor.origin().z * 1e3, 3.0), "{floor:?}");
    }
}

#[test]
fn a_hole_added_after_keeps_the_first_holes_names() {
    let params = [
        ("diameter", "4 mm"),
        ("cb-diameter", "8 mm"),
        ("cb-depth", "2 mm"),
        ("depth", "4 mm"),
    ];
    for at in [vec![(10.0, 8.0)], vec![(10.0, 8.0), (30.0, 22.0)]] {
        let doc = part(&at, |ids| hole(ids, &params, &[("kind", "counterbore")]));
        for index in 0..at.len() as u32 {
            let e = eval(&on_face(&doc, counterbore_floor(index)));
            let Some(SlotView::Plane(floor)) = e.slot(fid(8), "plane") else {
                panic!("hole {index}: {:?}", e.outcome(fid(8)))
            };
            assert!(near(floor.origin().z * 1e3, 3.0), "{floor:?}");
        }
    }
}
