//! Projected sketch geometry through the evaluator (plans/c1-m2-features
//! step 6, docs/DATA-MODEL.md §Sketches): a sketch on a plate's top face
//! holds the plate's top edges, re-projected on every evaluation, held by
//! the solver, driving what is drawn against them, and lost like any
//! reference when its edge is gone.

use std::collections::BTreeMap;

use arrix_core::{DVec2, DVec3, FeatureId, Id, ParamId, PartId, QuantityKind, Ref, RegionKey};
use arrix_doc::{
    Command, Document, Evaluation, Evaluator, FeatureEdit, FeatureOutcome, FeatureRecord,
    FeatureTypeId, Param, Part, Registry, SketchView, SlotView, apply, expr::Expr,
};
use arrix_sketch::{Constraint, Draft, Entity, EntityId, Point, PointId, Sketch};

const MM: f64 = 1e-3;
const PART: PartId = PartId(Id(1));
const W: ParamId = ParamId(Id(1));

const BASE: u64 = 1;
const PLATE: u64 = 2;
const TOP: u64 = 3;
const SIDE: u64 = 4;

fn fid(n: u64) -> FeatureId {
    FeatureId(Id(n))
}

fn record(id: u64, type_id: &str) -> FeatureRecord {
    FeatureRecord {
        id: fid(id),
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: format!("Feature {id}"),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

fn sketch_on(id: u64, plane: Option<Ref>, sketch: Sketch) -> FeatureRecord {
    let mut r = record(id, "core.sketch");
    r.sketch = Some(Box::new(sketch));
    if let Some(plane) = plane {
        r.inputs.insert("plane".into(), plane.into());
    }
    r
}

fn with(doc: &Document, command: Command) -> Document {
    apply(doc, &command).unwrap().document
}

fn add(doc: &Document, record: FeatureRecord) -> Document {
    let at = doc.parts()[&PART].history.len();
    with(
        doc,
        Command::AddFeature {
            part: PART,
            at,
            record,
        },
    )
}

fn evaluate(doc: &Document) -> Evaluation {
    Evaluator::new(Registry::with_core_types()).evaluate(doc)
}

fn view(e: &Evaluation, feature: u64) -> &SketchView {
    match e.slot(fid(feature), "sketch") {
        Some(SlotView::Sketch(v)) => v,
        _ => panic!("{:?}", e.outcome(fid(feature))),
    }
}

fn failure(e: &Evaluation, feature: u64) -> &arrix_core::Diagnostic {
    match e.outcome(fid(feature)) {
        Some(FeatureOutcome::Failed { diagnostic, .. }) => diagnostic,
        other => panic!("{other:?}"),
    }
}

fn topo(text: String) -> Ref {
    Ref::Topo(text.parse().unwrap())
}

/// The top edge the side swept by `line` ends in.
fn top_edge(line: EntityId) -> Ref {
    topo(format!("edge:sweep.{}.end-edge.{}", fid(PLATE), line))
}

/// A `w` × 30 mm rectangle from the origin, its width driven by `w`, and
/// its four sides, bottom first, anticlockwise.
fn base() -> (Sketch, [EntityId; 4]) {
    let mut d = Draft::seeded(7);
    let p = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)]
        .map(|(x, y)| d.add_point(Point::new(x * MM, y * MM)));
    d.point_mut(p[0]).unwrap().fixed = true;
    let sides = [0, 1, 2, 3].map(|i| {
        let line = d.add_entity(Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
        d.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
        line
    });
    let width = d.add_constraint(Constraint::Distance {
        a: p[0],
        b: p[1],
        value: 0.04,
    });
    d.set_constraint_expr(width, Some("w".into()));
    d.add_constraint(Constraint::Distance {
        a: p[1],
        b: p[2],
        value: 0.03,
    });
    (d.sketch, sides)
}

/// The plate: `w` = 40 mm, the base sketch and its region extruded 5 mm.
fn plate() -> (Document, [EntityId; 4]) {
    let (sketch, sides) = base();
    let doc = with(
        &Document::default(),
        Command::AddParam {
            param: W,
            record: Param {
                name: "w".into(),
                kind: QuantityKind::Length,
                expr: Expr::parse("40 mm").unwrap(),
            },
        },
    );
    let doc = with(
        &doc,
        Command::AddPart {
            part: Part {
                id: PART,
                name: "Part".into(),
                history: vec![fid(BASE)],
                features: [(fid(BASE), sketch_on(BASE, None, sketch))].into(),
                rollback: None,
                material: None,
                body_materials: Default::default(),
            },
        },
    );
    let key = view(&evaluate(&doc), BASE).regions[0].key.clone();
    (add(&doc, extrude(&key)), sides)
}

fn extrude(key: &RegionKey) -> FeatureRecord {
    let mut r = record(PLATE, "core.extrude");
    r.params
        .insert("distance".into(), Expr::parse("5 mm").unwrap());
    r.choices.insert("mode".into(), "new".into());
    r.inputs.insert(
        "region".into(),
        Ref::Region {
            feature: fid(BASE),
            key: key.clone(),
        }
        .into(),
    );
    r
}

/// The top sketch: the four top edges projected, each a line of its own
/// stored nowhere near the plate, and a drawn line held parallel to the
/// projected front edge from its start.
struct Top {
    projected: [EntityId; 4],
    drawn: (PointId, PointId),
}

fn top(sides: [EntityId; 4]) -> (Sketch, Top) {
    let mut d = Draft::seeded(8);
    let projected = sides.map(|side| {
        let (_, _, line) = d.add_line(0.0, 0.0, 0.001, 0.0);
        d.set_source(line, Some(top_edge(side))).unwrap();
        line
    });
    let (a, b, drawn) = d.add_line(0.005, 0.004, 0.012, 0.009);
    d.add_constraint(Constraint::Parallel {
        a: projected[0],
        b: drawn,
    });
    let (front_start, _) = d.entity(projected[0]).unwrap().line_ends().unwrap();
    d.add_constraint(Constraint::Coincident { a, b: front_start });
    (
        d.sketch,
        Top {
            projected,
            drawn: (a, b),
        },
    )
}

fn top_face() -> Ref {
    topo(format!("face:sweep.{}.end-cap", fid(PLATE)))
}

fn world(v: &SketchView, p: PointId) -> DVec3 {
    let q = v.sketch.point(p).unwrap();
    v.plane.point(DVec2::new(q.x, q.y))
}

fn ends(v: &SketchView, line: EntityId) -> (DVec3, DVec3) {
    let (a, b) = v.sketch.entity(line).unwrap().line_ends().unwrap();
    (world(v, a), world(v, b))
}

fn near(a: DVec3, b: DVec3) -> bool {
    a.distance(b) < 1e-9
}

/// Whether `line` is the segment from `p` to `q`, either way round.
fn spans(v: &SketchView, line: EntityId, p: DVec3, q: DVec3) -> bool {
    let (a, b) = ends(v, line);
    (near(a, p) && near(b, q)) || (near(a, q) && near(b, p))
}

fn mm(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z) * MM
}

#[test]
fn projected_top_edges_follow_a_width_edit_and_drive_what_is_drawn_on_them() {
    let (doc, sides) = plate();
    let (sketch, t) = top(sides);
    let doc = add(&doc, sketch_on(TOP, Some(top_face()), sketch));

    let check = |doc: &Document, w: f64| {
        let e = evaluate(doc);
        let v = view(&e, TOP);
        let corners = [(0.0, 0.0), (w, 0.0), (w, 30.0), (0.0, 30.0)].map(|(x, y)| mm(x, y, 5.0));
        for (i, line) in t.projected.iter().enumerate() {
            assert!(
                spans(v, *line, corners[i], corners[(i + 1) % 4]),
                "projected edge {i} at w = {w}: {:?}",
                ends(v, *line)
            );
        }
        // The drawn line starts on the front edge's start and runs along it.
        let (front_start, front_end) = {
            let (a, b) = v
                .sketch
                .entity(t.projected[0])
                .unwrap()
                .line_ends()
                .unwrap();
            (world(v, a), world(v, b))
        };
        let (a, b) = (world(v, t.drawn.0), world(v, t.drawn.1));
        assert!(near(a, front_start));
        let (along, front) = ((b - a).normalize(), (front_end - front_start).normalize());
        assert!(along.cross(front).length() < 1e-9);
        along.dot(front)
    };
    let before = check(&doc, 40.0);
    let wider = with(
        &doc,
        Command::SetParam {
            param: W,
            expr: Expr::parse("60 mm").unwrap(),
        },
    );
    // The same branch: the drawn line runs the same way along the edge.
    assert_eq!(check(&wider, 60.0).signum(), before.signum());
}

#[test]
fn a_projected_edge_redrawn_away_upstream_is_lost_naming_the_entity() {
    let (doc, sides) = plate();
    let (sketch, t) = top(sides);
    let doc = add(&doc, sketch_on(TOP, Some(top_face()), sketch));

    // The base's bottom side redrawn: a new line between the same points,
    // and the plate re-pointed at the region it now bounds.
    let (_, _, base_record) = doc.feature(fid(BASE)).unwrap();
    let mut redrawn = Draft::with_sketch(
        base_record.sketch.as_deref().unwrap().clone(),
        arrix_core::IdMinter::new(99),
    );
    let (start, end) = redrawn.entity(sides[0]).unwrap().line_ends().unwrap();
    redrawn.remove_entity(sides[0]);
    let line = redrawn.add_entity(Entity::Line { start, end });
    redrawn.add_constraint(Constraint::Horizontal { line });
    let doc = with(
        &doc,
        Command::SketchEdit {
            feature: fid(BASE),
            edit: arrix_sketch::SketchEdit::diff(base_record.sketch.as_deref().unwrap(), &redrawn),
        },
    );
    let key = view(&evaluate(&doc), BASE).regions[0].key.clone();
    let doc = with(
        &doc,
        Command::EditFeature {
            feature: fid(PLATE),
            edit: FeatureEdit {
                inputs: [(
                    "region".into(),
                    Some(
                        Ref::Region {
                            feature: fid(BASE),
                            key,
                        }
                        .into(),
                    ),
                )]
                .into(),
                ..FeatureEdit::default()
            },
        },
    );

    let e = evaluate(&doc);
    assert!(matches!(
        e.outcome(fid(PLATE)),
        Some(FeatureOutcome::Ok { .. })
    ));
    let lost = failure(&e, TOP);
    assert_eq!(lost.code.as_str(), "ref.lost", "{lost:?}");
    assert!(lost.refs.contains(&top_edge(sides[0])));
    assert!(lost.refs.contains(&Ref::Sketch {
        feature: fid(TOP),
        entity: t.projected[0],
    }));
    assert!(lost.candidates.contains(&top_edge(line)), "{lost:?}");
}

#[test]
fn an_edge_seen_end_on_fails_the_sketch_with_the_kernels_code() {
    let (doc, sides) = plate();
    // On the front side face, the left side's top edge runs straight at
    // the viewer.
    let mut d = Draft::seeded(9);
    let (_, _, line) = d.add_line(0.0, 0.0, 0.001, 0.0);
    d.set_source(line, Some(top_edge(sides[3]))).unwrap();
    let front = topo(format!("face:sweep.{}.side.{}", fid(PLATE), sides[0]));
    let doc = add(&doc, sketch_on(SIDE, Some(front), d.sketch));

    let e = evaluate(&doc);
    let failed = failure(&e, SIDE);
    assert_eq!(
        failed.code.as_str(),
        "kernel.degenerate.projection-collapses",
        "{failed:?}"
    );
    assert!(failed.refs.contains(&Ref::Sketch {
        feature: fid(SIDE),
        entity: line,
    }));
    assert!(matches!(
        e.outcome(fid(SIDE)),
        Some(FeatureOutcome::Failed { call: Some(_), .. })
    ));
}

#[test]
fn a_projection_of_another_shape_than_the_entity_names_both() {
    let (doc, sides) = plate();
    let mut d = Draft::seeded(10);
    let (_, circle) = d.add_circle(0.01, 0.01, 0.002);
    d.set_source(circle, Some(top_edge(sides[1]))).unwrap();
    let doc = add(&doc, sketch_on(TOP, Some(top_face()), d.sketch));

    let failed = failure(&evaluate(&doc), TOP).clone();
    assert_eq!(failed.code.as_str(), "sketch.source-kind", "{failed:?}");
    assert!(failed.message.contains("a line, where a circle is drawn"));
    assert!(failed.refs.contains(&top_edge(sides[1])));
}
