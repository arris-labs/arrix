//! `core.fillet` and `core.chamfer` through the evaluator: edges of a
//! sketched, extruded plate named by `Ref::Topo`, blended, with volumes
//! worked out by hand and the failures a blend can end in.

use std::collections::BTreeMap;

use arrix_core::{
    CurveKey, FeatureId, Id, NameRoot, ParamId, PartId, PersistentName, QuantityKind, Ref,
    RegionKey, SlotName, SweepPartName, TopoKind,
};
use arrix_doc::{
    Command, Document, Evaluation, Evaluator, FeatureEdit, FeatureOutcome, FeatureRecord,
    FeatureTypeId, Param, Part, Registry, SlotView, apply, expr::Expr,
};
use arrix_sketch::{Constraint, Draft, Entity, EntityId, Point, Sketch};

const MM: f64 = 1e-3;
const PART: PartId = PartId(Id(1));
const T: ParamId = ParamId(Id(1));

const PLATE_SKETCH: u64 = 3;
const PLATE: u64 = 4;
const BLEND: u64 = 5;
const SECOND: u64 = 6;

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

/// A 20 × 10 mm stadium, two lines and two half circles, at the origin.
fn stadium() -> Sketch {
    let mut d = Draft::seeded(8);
    let [a, b, c, e, f, g] = [
        (5.0, 0.0),
        (15.0, 0.0),
        (15.0, 10.0),
        (5.0, 10.0),
        (15.0, 5.0),
        (5.0, 5.0),
    ]
    .map(|(x, y)| d.add_point(Point::new(x * MM, y * MM)));
    for entity in [
        Entity::Line { start: a, end: b },
        Entity::Arc {
            center: f,
            start: b,
            end: c,
        },
        Entity::Line { start: c, end: e },
        Entity::Arc {
            center: g,
            start: e,
            end: a,
        },
    ] {
        d.add_entity(entity);
    }
    d.sketch
}

/// The line of `sketch` between the points (`x0`, `y0`) and (`x1`, `y1`)
/// millimetres, in either order.
fn line_between(sketch: &Sketch, (x0, y0): (f64, f64), (x1, y1): (f64, f64)) -> EntityId {
    let at = |id| {
        let p = sketch.point(id).unwrap();
        ((p.x / MM).round(), (p.y / MM).round())
    };
    sketch
        .entities()
        .iter()
        .find_map(|(id, e)| match e {
            Entity::Line { start, end } => {
                let ends = (at(*start), at(*end));
                (ends == ((x0, y0), (x1, y1)) || ends == ((x1, y1), (x0, y0))).then_some(*id)
            }
            _ => None,
        })
        .expect("the sketch holds that line")
}

/// The plate's top edge along `entity`: the sweep's end edge of the curve
/// the sketch entity is.
fn top_edge(entity: EntityId) -> Ref {
    Ref::Topo(PersistentName {
        kind: TopoKind::Edge,
        root: NameRoot::Sweep {
            feature: fid(PLATE),
            index: 0,
            part: SweepPartName::EndEdge(CurveKey(entity.0)),
        },
        chain: vec![],
    })
}

fn blend(id: u64, ty: &str, size: &str, edges: Vec<Ref>) -> FeatureRecord {
    let mut r = record(id, ty, &format!("Blend {id}"));
    let param = if ty == "core.fillet" {
        "radius"
    } else {
        "distance"
    };
    r.params.insert(param.into(), Expr::parse(size).unwrap());
    r.inputs
        .insert("target".into(), plate_slot(PLATE, "body").into());
    r.inputs.insert("edges".into(), edges.into());
    r
}

/// `plate` extruded `t` = 5 mm, then `blends` after it.
fn part(plate: &Sketch, blends: Vec<FeatureRecord>) -> Document {
    let mut features = vec![
        sketch(PLATE_SKETCH, None, plate.clone()),
        extrude(PLATE, PLATE_SKETCH, &only_region(plate), "new", "t"),
    ];
    features.extend(blends);
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

/// The 40 × 30 mm plate and its four top edges, bottom, right, top, left.
fn plate() -> (Sketch, [Ref; 4]) {
    let sketch = rect(0.0, 0.0, 40.0, 30.0);
    let edges = [
        line_between(&sketch, (0.0, 0.0), (40.0, 0.0)),
        line_between(&sketch, (40.0, 0.0), (40.0, 30.0)),
        line_between(&sketch, (40.0, 30.0), (0.0, 30.0)),
        line_between(&sketch, (0.0, 30.0), (0.0, 0.0)),
    ]
    .map(top_edge);
    (sketch, edges)
}

/// A body slot's volume in cubic millimetres.
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

/// What a blend of depth `d(x)` at distance `x` from an edge removes where
/// two edges meet at a corner and the walls stay sharp: the two prisms
/// overlap in `∫∫ min(d(x), d(y))` over the square of side `size`.
fn corner_overlap(size: f64, depth: impl Fn(f64) -> f64) -> f64 {
    let n = 1000;
    let step = size / f64::from(n);
    let mut sum = 0.0;
    for i in 0..n {
        for j in 0..n {
            let (x, y) = ((f64::from(i) + 0.5) * step, (f64::from(j) + 0.5) * step);
            sum += depth(x).min(depth(y));
        }
    }
    sum * step * step
}

#[test]
fn four_edges_of_a_plate_filleted() {
    let (sketch, edges) = plate();
    let doc = part(
        &sketch,
        vec![blend(BLEND, "core.fillet", "2 mm", edges.to_vec())],
    );
    let e = eval(&doc);
    let r: f64 = 2.0;
    let per_edge = (1.0 - std::f64::consts::FRAC_PI_4) * r * r;
    let corner = corner_overlap(r, |x| r - (r * r - (r - x) * (r - x)).sqrt());
    let want = 6000.0 - per_edge * 140.0 + 4.0 * corner;
    assert!(
        near(volume(&e, BLEND), want),
        "{} / {want}",
        volume(&e, BLEND)
    );
}

#[test]
fn four_edges_of_a_plate_chamfered() {
    let (sketch, edges) = plate();
    let doc = part(
        &sketch,
        vec![blend(BLEND, "core.chamfer", "2 mm", edges.to_vec())],
    );
    let e = eval(&doc);
    let d: f64 = 2.0;
    // Triangles of d²/2 along the 140 mm of rim, the corners' overlap d³/3.
    let want = 6000.0 - d * d / 2.0 * 140.0 + 4.0 * d * d * d / 3.0;
    assert!(
        near(volume(&e, BLEND), want),
        "{} / {want}",
        volume(&e, BLEND)
    );
}

#[test]
fn one_edge_of_a_tangent_chain_blends_the_whole_chain() {
    let sketch = stadium();
    let chain = line_between(&sketch, (5.0, 0.0), (15.0, 0.0));
    let one = part(
        &sketch,
        vec![blend(BLEND, "core.fillet", "2 mm", vec![top_edge(chain)])],
    );
    let one = eval(&one);
    let all: Vec<Ref> = sketch.entities().keys().map(|id| top_edge(*id)).collect();
    let all = eval(&part(
        &sketch,
        vec![blend(BLEND, "core.fillet", "2 mm", all)],
    ));
    assert!(near(volume(&one, BLEND), volume(&all, BLEND)));
    assert!(volume(&one, BLEND) < volume(&one, PLATE));
}

#[test]
fn a_blend_of_a_blended_body_leaves_the_first_blend_in_place() {
    let (sketch, edges) = plate();
    let [bottom, _, top, _] = edges;
    let doc = part(
        &sketch,
        vec![
            blend(BLEND, "core.fillet", "2 mm", vec![bottom]),
            blend(SECOND, "core.chamfer", "1 mm", vec![top]),
        ],
    );
    let e = eval(&doc);
    let fillet = (1.0 - std::f64::consts::FRAC_PI_4) * 4.0 * 40.0;
    assert!(near(volume(&e, BLEND), 6000.0 - fillet));
    // Both blends are versions of the plate's slot, the second on the first.
    let chamfer = 0.5 * 1.0 * 40.0;
    assert!(
        near(volume(&e, SECOND), 6000.0 - fillet - chamfer),
        "{}",
        volume(&e, SECOND)
    );
}

#[test]
fn an_oversize_fillet_fails_as_blend_too_large_with_its_record() {
    let (sketch, edges) = plate();
    let [bottom, ..] = edges;
    let e = eval(&part(
        &sketch,
        vec![blend(BLEND, "core.fillet", "30 mm", vec![bottom])],
    ));
    assert_eq!(code(&e, BLEND), "kernel.degenerate.blend-too-large");
    let Some(FeatureOutcome::Failed { call, .. }) = e.outcome(fid(BLEND)) else {
        unreachable!()
    };
    assert!(call.is_some(), "the failed call is kept");
}

#[test]
fn a_blend_needs_its_target_and_edges_and_names_an_edge_of_the_body() {
    let (sketch, edges) = plate();
    let [bottom, ..] = edges;
    let clear = |input: &str| FeatureEdit {
        inputs: BTreeMap::from([(input.to_string(), None)]),
        ..FeatureEdit::default()
    };
    let bottom_alone = bottom.clone();
    let doc = part(
        &sketch,
        vec![blend(BLEND, "core.fillet", "2 mm", vec![bottom])],
    );
    for (edit, want) in [
        (clear("target"), "fillet.no-target"),
        (clear("edges"), "fillet.no-edges"),
        (
            FeatureEdit {
                choices: BTreeMap::from([("mode".to_string(), Some("x".to_string()))]),
                ..FeatureEdit::default()
            },
            "feature.unknown-field",
        ),
        (
            FeatureEdit {
                inputs: BTreeMap::from([("edges".to_string(), Some(bottom_alone.clone().into()))]),
                ..FeatureEdit::default()
            },
            "input.list",
        ),
    ] {
        assert_eq!(code(&eval(&edited(&doc, BLEND, edit)), BLEND), want);
    }
    // An edge the body does not have is a lost reference, with candidates.
    let nowhere = Ref::Topo(
        format!("edge:sweep.{}.end-edge.0000000000009", fid(PLATE))
            .parse()
            .unwrap(),
    );
    let doc = part(
        &sketch,
        vec![blend(BLEND, "core.chamfer", "2 mm", vec![nowhere])],
    );
    let e = eval(&doc);
    assert_eq!(code(&e, BLEND), "ref.lost");
    let Some(FeatureOutcome::Failed { diagnostic, .. }) = e.outcome(fid(BLEND)) else {
        unreachable!()
    };
    assert!(!diagnostic.candidates.is_empty(), "{diagnostic:?}");
}
