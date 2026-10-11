//! `core.pattern-linear` and `core.pattern-circular` through the
//! evaluator: a hole patterned along a sketched line and about a sketched
//! axis, then a body, with volumes worked out by hand and the names of the
//! copies held while the count changes.

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
const PATTERN: u64 = 8;
const FLOOR: u64 = 9;
const AXIS: u64 = 10;

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

const PI: f64 = std::f64::consts::PI;

/// The sketch `POINTS` stands on the top plane: the hole's one point at
/// (`x`, `y`) mm, and a 10 mm line along +x from the origin, the linear
/// pattern's direction.
fn hole_sketch(x: f64, y: f64) -> (Sketch, arrix_sketch::PointId, arrix_sketch::EntityId) {
    let mut d = Draft::seeded(9);
    let hole = d.add_point(Point::new(x * MM, y * MM));
    let a = d.add_point(Point::new(0.0, 0.0));
    let b = d.add_point(Point::new(10.0 * MM, 0.0));
    let line = d.add_entity(Entity::Line { start: a, end: b });
    (d.sketch, hole, line)
}

/// The sketch `AXIS` on the world `zx` plane: a line along +z through the
/// origin, the circular pattern's axis.
fn axis_sketch() -> (Sketch, arrix_sketch::EntityId) {
    let mut d = Draft::seeded(11);
    let a = d.add_point(Point::new(0.0, 0.0));
    let b = d.add_point(Point::new(10.0 * MM, 0.0));
    let line = d.add_entity(Entity::Line { start: a, end: b });
    (d.sketch, line)
}

fn sketch_ref(feature: u64, entity: arrix_sketch::EntityId) -> Ref {
    Ref::Sketch {
        feature: fid(feature),
        entity,
    }
}

/// A 3 mm blind counterbored hole at the point of `POINTS`: 4 mm across,
/// 8 mm across and 2 mm deep at the top.
fn counterbored(point: arrix_sketch::PointId) -> FeatureRecord {
    let mut r = record(HOLE, "core.hole", "Hole");
    for (name, value) in [
        ("diameter", "4 mm"),
        ("depth", "4 mm"),
        ("cb-diameter", "8 mm"),
        ("cb-depth", "2 mm"),
    ] {
        r.params.insert(name.into(), Expr::parse(value).unwrap());
    }
    r.choices.insert("kind".into(), "counterbore".into());
    r.inputs
        .insert("target".into(), plate_slot(PLATE, "body").into());
    r.inputs
        .insert("points".into(), vec![sketch_ref(POINTS, point)].into());
    r
}

/// A pattern of the hole onto the plate, with `count` and `along`: the
/// spacing in mm or the angle in degrees.
fn pattern(ty: &str, count: u32, along: &str, input: &str, from: Ref) -> FeatureRecord {
    let mut r = record(PATTERN, ty, "Pattern");
    r.params
        .insert("count".into(), Expr::parse(&count.to_string()).unwrap());
    let name = if ty == "core.pattern-linear" {
        "spacing"
    } else {
        "angle"
    };
    r.params.insert(name.into(), Expr::parse(along).unwrap());
    r.inputs
        .insert("target".into(), plate_slot(PLATE, "body").into());
    r.inputs
        .insert("tools".into(), vec![Ref::Feature(fid(HOLE))].into());
    r.inputs.insert(input.into(), from.into());
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

/// The plate, the sketch with its hole at (`x`, `y`), the counterbored
/// hole, and `then` after.
fn with_hole(
    (x, y): (f64, f64),
    then: impl FnOnce(arrix_sketch::EntityId) -> Vec<FeatureRecord>,
) -> Document {
    let (sketch, point, line) = hole_sketch(x, y);
    let mut after = vec![
        self::sketch(POINTS, Some(DATUM), sketch),
        counterbored(point),
    ];
    after.extend(then(line));
    part(after)
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

/// What one counterbored hole removes, mm³: 8 mm across for 2 mm, then
/// 4 mm across for 2 mm.
const HOLE_VOLUME: f64 = PI * (16.0 * 2.0 + 4.0 * 2.0);
const PLATE_VOLUME: f64 = 50.0 * 40.0 * 5.0;

fn linear(count: u32, line: arrix_sketch::EntityId) -> FeatureRecord {
    pattern(
        "core.pattern-linear",
        count,
        "9 mm",
        "direction",
        sketch_ref(POINTS, line),
    )
}

#[test]
fn a_hole_patterned_linearly_at_three_five_and_two() {
    for count in [3, 5, 2] {
        let doc = with_hole((-17.0, 0.0), |line| vec![linear(count, line)]);
        let e = eval(&doc);
        let want = PLATE_VOLUME - f64::from(count) * HOLE_VOLUME;
        assert!(
            near(volume(&e, PATTERN), want),
            "{count}: {} / {want}",
            volume(&e, PATTERN)
        );
    }
}

/// Copy `i` of the hole's counterbore floor: the tool's third face as
/// copy `i` of the pattern, cut as the pattern's `i`-th boolean.
fn copy_floor(i: u32) -> PersistentName {
    let tool = PersistentName {
        kind: TopoKind::Face,
        root: NameRoot::Revolve {
            feature: fid(HOLE),
            index: 0,
            part: SweepPartName::Side(CurveKey(Id(3))),
        },
        chain: vec![],
    };
    let instance = NameStep::Instance {
        feature: fid(PATTERN),
        index: i,
    };
    let copy = PersistentName {
        chain: vec![instance.clone()],
        ..tool.clone()
    };
    PersistentName {
        chain: vec![
            instance,
            NameStep::Generated {
                feature: fid(PATTERN),
                boolean: i - 1,
                from: vec![copy],
            },
        ],
        ..tool
    }
}

/// A datum plane on `name`, after the pattern, as feature `FLOOR`.
fn on_face(doc: &Document, at: usize, name: PersistentName) -> Document {
    let mut datum = record(FLOOR, "core.datum-plane", "On face");
    datum
        .params
        .insert("offset".into(), Expr::parse("0 mm").unwrap());
    datum.inputs.insert("plane".into(), Ref::Topo(name).into());
    with(
        doc,
        Command::AddFeature {
            part: PART,
            at,
            record: datum,
        },
    )
}

#[test]
fn the_surviving_copies_keep_their_names_as_the_count_changes() {
    for count in [3, 5, 2, 4] {
        let doc = with_hole((-17.0, 0.0), |line| vec![linear(count, line)]);
        for i in 1..count {
            let e = eval(&on_face(&doc, 6, copy_floor(i)));
            let Some(SlotView::Plane(floor)) = e.slot(fid(FLOOR), "plane") else {
                panic!("{count}, copy {i}: {:?}", e.outcome(fid(FLOOR)))
            };
            // The floor is 2 mm down and the copy 9 mm on from the last.
            let (x, z) = (floor.origin().x * 1e3, floor.origin().z * 1e3);
            assert!(near(z, 3.0), "{count}, copy {i}: {floor:?}");
            assert!(
                (x - (-17.0 + 9.0 * f64::from(i))).abs() < 1e-3,
                "{count}, copy {i}: {floor:?}"
            );
        }
        // Copy `count` is not there.
        let e = eval(&on_face(&doc, 6, copy_floor(count)));
        assert_eq!(code(&e, FLOOR), "ref.lost");
    }
}

#[test]
fn a_hole_patterned_about_an_axis() {
    let (axis, line) = axis_sketch();
    let about = |count: u32, angle: &str| {
        let (sketch, point, _) = hole_sketch(10.0, 0.0);
        let mut on_zx = self::sketch(AXIS, None, axis.clone());
        on_zx.choices.insert("world".into(), "zx".into());
        part(vec![
            self::sketch(POINTS, Some(DATUM), sketch),
            counterbored(point),
            on_zx,
            pattern(
                "core.pattern-circular",
                count,
                angle,
                "axis",
                sketch_ref(AXIS, line),
            ),
        ])
    };
    // A full turn is shared out among the instances.
    let e = eval(&about(4, "360 deg"));
    let want = PLATE_VOLUME - 4.0 * HOLE_VOLUME;
    assert!(near(volume(&e, PATTERN), want), "{}", volume(&e, PATTERN));
    // A part of one runs from the first instance to the last, here by 90°.
    let e = eval(&about(3, "180 deg"));
    let want = PLATE_VOLUME - 3.0 * HOLE_VOLUME;
    assert!(near(volume(&e, PATTERN), want), "{}", volume(&e, PATTERN));
}

#[test]
fn a_body_patterned_is_fused_to_itself() {
    // The plate copied 25 mm along x, twice: 100 mm of plate in all.
    let (sketch, _, line) = hole_sketch(0.0, 0.0);
    let mut r = pattern(
        "core.pattern-linear",
        3,
        "25 mm",
        "direction",
        sketch_ref(POINTS, line),
    );
    r.inputs.remove("tools");
    let doc = part(vec![self::sketch(POINTS, Some(DATUM), sketch), r]);
    let e = eval(&doc);
    assert!(
        near(volume(&e, PATTERN), 100.0 * 40.0 * 5.0),
        "{:?}",
        e.outcome(fid(PATTERN))
    );
    // Copies that do not touch the body are not joined to it.
    let far = edited(
        &doc,
        PATTERN,
        FeatureEdit {
            params: BTreeMap::from([("spacing".to_string(), Some(Expr::parse("80 mm").unwrap()))]),
            ..FeatureEdit::default()
        },
    );
    assert_eq!(code(&eval(&far), PATTERN), "pattern.misses");
}

#[test]
fn a_copy_that_misses_the_body_says_so() {
    let doc = with_hole((-17.0, 0.0), |line| {
        let mut r = linear(3, line);
        r.params
            .insert("spacing".into(), Expr::parse("100 mm").unwrap());
        vec![r]
    });
    assert_eq!(code(&eval(&doc), PATTERN), "pattern.misses");
}

#[test]
fn a_pattern_needs_its_target_direction_and_sane_numbers() {
    let doc = with_hole((-17.0, 0.0), |line| vec![linear(3, line)]);
    let clear = |input: &str| FeatureEdit {
        inputs: BTreeMap::from([(input.to_string(), None)]),
        ..FeatureEdit::default()
    };
    let param = |name: &str, value: &str| FeatureEdit {
        params: BTreeMap::from([(name.to_string(), Some(Expr::parse(value).unwrap()))]),
        ..FeatureEdit::default()
    };
    for (edit, want) in [
        (clear("target"), "pattern.no-target"),
        (clear("direction"), "pattern.no-direction"),
        (param("count", "0"), "pattern.count"),
        (param("count", "1"), "pattern.count"),
        (param("count", "100000"), "pattern.count"),
        (param("spacing", "0 mm"), "pattern.spacing"),
        (
            FeatureEdit {
                choices: BTreeMap::from([("mode".to_string(), Some("x".to_string()))]),
                ..FeatureEdit::default()
            },
            "feature.unknown-field",
        ),
    ] {
        let e = eval(&edited(&doc, PATTERN, edit));
        assert_eq!(code(&e, PATTERN), want);
    }
}
