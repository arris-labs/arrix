//! `core.sketch` through the evaluator: its plane, its dimensions resolved
//! from the parameters, the solve from its stored positions, its `sketch`
//! slot, and the soft failures that name what they could not meet.

use std::collections::BTreeMap;

use arrix_core::{DVec3, FeatureId, Id, ParamId, PartId, QuantityKind, Ref, SlotName};
use arrix_sketch::{
    Constraint, ConstraintId, Draft, Entity, EntityId, Point, PointId, ResolveRegion, Sketch,
};

use super::*;
use crate::document::{FeatureTypeId, Param, Part};

const PART: PartId = PartId(Id(1));
const PLANE: FeatureId = FeatureId(Id(2));
const SKETCH: FeatureId = FeatureId(Id(3));
const MM: f64 = 1e-3;

/// A `w` × `h` rectangle from a fixed origin corner with a bore of
/// diameter `d` at its centre, fully constrained by expressions over the
/// parameters.
struct Plate {
    sketch: Sketch,
    corners: [PointId; 4],
    bore: EntityId,
    width: ConstraintId,
}

fn plate() -> Plate {
    let mut d = Draft::seeded(11);
    let p = [
        Point::fixed(0.0, 0.0),
        Point::new(40.0 * MM, 0.0),
        Point::new(40.0 * MM, 30.0 * MM),
        Point::new(0.0, 30.0 * MM),
    ]
    .map(|p| d.add_point(p));
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
    let (c, bore) = d.add_circle(20.0 * MM, 15.0 * MM, 5.0 * MM);
    let dimension = |d: &mut Draft, c: Constraint, expr: &str| {
        let id = d.add_constraint(c);
        d.set_constraint_expr(id, Some(expr.into()));
        id
    };
    let width = dimension(
        &mut d,
        Constraint::Distance {
            a: p[0],
            b: p[1],
            value: 0.0,
        },
        "w",
    );
    let pairs = [
        (
            Constraint::Distance {
                a: p[1],
                b: p[2],
                value: 0.0,
            },
            "h",
        ),
        (
            Constraint::HorizontalDistance {
                a: p[0],
                b: c,
                value: 0.0,
            },
            "w / 2",
        ),
        (
            Constraint::VerticalDistance {
                a: p[0],
                b: c,
                value: 0.0,
            },
            "h / 2",
        ),
        (
            Constraint::Diameter {
                target: bore,
                value: 0.0,
            },
            "d",
        ),
    ];
    for (c, expr) in pairs {
        dimension(&mut d, c, expr);
    }
    Plate {
        sketch: d.sketch,
        corners: p,
        bore,
        width,
    }
}

fn param(name: &str, expr: &str) -> Param {
    Param {
        name: name.into(),
        kind: QuantityKind::Length,
        expr: Expr::parse(expr).unwrap(),
    }
}

/// `w`, `h` and `d`, a datum plane 10 mm above world XY and the sketch on
/// it.
fn document(sketch: Sketch) -> Document {
    let plane = FeatureRecord {
        id: PLANE,
        type_id: FeatureTypeId::new("core.datum-plane").unwrap(),
        type_version: 1,
        name: "Plane".into(),
        params: BTreeMap::from([("offset".into(), Expr::parse("10 mm").unwrap())]),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
    };
    let on = Ref::Slot {
        feature: PLANE,
        slot: SlotName::new("plane").unwrap(),
    };
    let sketch = FeatureRecord {
        id: SKETCH,
        type_id: FeatureTypeId::new("core.sketch").unwrap(),
        name: "Sketch".into(),
        params: BTreeMap::new(),
        inputs: BTreeMap::from([("plane".into(), on)]),
        sketch: Some(Box::new(sketch)),
        ..plane.clone()
    };
    Document {
        params: [(1, "w", "40 mm"), (2, "h", "30 mm"), (3, "d", "10 mm")]
            .into_iter()
            .map(|(id, n, e)| (ParamId(Id(id)), param(n, e)))
            .collect(),
        parts: BTreeMap::from([(
            PART,
            Part {
                id: PART,
                name: "Part".into(),
                history: vec![PLANE, SKETCH],
                features: [plane, sketch].into_iter().map(|f| (f.id, f)).collect(),
                rollback: None,
            },
        )]),
    }
}

fn view(e: &Evaluation) -> &SketchView {
    match e.slot(SKETCH, "sketch") {
        Some(SlotView::Sketch(view)) => view,
        other => panic!("{:?}: {other:?}", e.outcome(SKETCH)),
    }
}

fn failure(e: &Evaluation) -> &Diagnostic {
    match e.outcome(SKETCH) {
        Some(FeatureOutcome::Failed { diagnostic, .. }) => diagnostic,
        other => panic!("{other:?}"),
    }
}

fn pos(view: &SketchView, p: PointId) -> [f64; 2] {
    view.sketch.point(p).unwrap().pos()
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) <= 1e-9
}

#[test]
fn a_sketch_solves_on_its_plane_with_its_dimensions_resolved() {
    let plate = plate();
    let mut doc = document(plate.sketch.clone());
    let mut ev = Evaluator::new(Registry::with_core_types());
    let first = ev.evaluate(&doc);
    let v = view(&first);
    assert_eq!(v.plane.origin(), DVec3::new(0.0, 0.0, 10.0 * MM));
    assert_eq!((v.dof, v.redundant.len()), (0, 0));
    assert!(near(pos(v, plate.corners[2]), [40.0 * MM, 30.0 * MM]));
    assert_eq!(v.regions.len(), 2, "the holed plate and the bore's disc");
    let holed = &v.regions[0];
    assert_eq!(holed.profile.holes().len(), 1);
    assert!(holed.key.entities.contains(&plate.bore));
    assert_eq!(holed.profile.plane(), &v.plane);

    // `w` edited: re-solved from the stored positions; the region's key
    // still resolves in the new solve.
    let w = doc.params.get_mut(&ParamId(Id(1))).unwrap();
    w.expr = Expr::parse("50 mm").unwrap();
    let second = ev.evaluate(&doc);
    let v2 = view(&second);
    assert!(near(pos(v2, plate.corners[2]), [50.0 * MM, 30.0 * MM]));
    assert_eq!(
        v2.sketch
            .get_constraint(plate.width)
            .unwrap()
            .dimensional_value(),
        Some(0.05)
    );
    assert!(holed.key.resolve(&v2.sketch).is_some());
    assert!(matches!(
        second.outcome(SKETCH),
        Some(FeatureOutcome::Ok { cached: false, .. })
    ));
}

#[test]
fn a_stored_position_is_an_input_even_where_the_solve_ends_the_same() {
    let plate = plate();
    let mut ev = Evaluator::new(Registry::with_core_types());
    let first = ev.evaluate(&document(plate.sketch.clone()));
    let mut moved = plate.sketch.clone();
    moved
        .point_mut(plate.corners[2])
        .unwrap()
        .set_pos(41.0 * MM, 31.0 * MM);
    let second = ev.evaluate(&document(moved));
    assert!(near(
        pos(view(&second), plate.corners[2]),
        pos(view(&first), plate.corners[2])
    ));
    assert!(matches!(
        second.outcome(SKETCH),
        Some(FeatureOutcome::Ok { cached: false, .. })
    ));
}

#[test]
fn a_conflict_fails_the_feature_naming_the_constraints() {
    let plate = plate();
    let mut d = Draft::with_sketch(plate.sketch, arrix_core::IdMinter::new(5));
    let extra = d.add_constraint(Constraint::Distance {
        a: plate.corners[0],
        b: plate.corners[1],
        value: 0.0,
    });
    d.set_constraint_expr(extra, Some("w + 1 mm".into()));
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&document(d.sketch));
    let diagnostic = failure(&e);
    assert_eq!(diagnostic.code.as_str(), "sketch.conflict");
    assert!(
        diagnostic.refs.iter().any(|r| matches!(
            r,
            Ref::Sketch {
                feature: SKETCH,
                ..
            }
        )),
        "{diagnostic:?}"
    );
}

#[test]
fn a_dimension_of_the_wrong_kind_fails_naming_it() {
    let plate = plate();
    let mut sketch = plate.sketch;
    sketch.set_constraint_expr(plate.width, Some("30 deg".into()));
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&document(sketch));
    let diagnostic = failure(&e);
    assert!(
        diagnostic.code.as_str().starts_with("expr."),
        "{diagnostic:?}"
    );
    assert!(diagnostic.refs.contains(&Ref::Sketch {
        feature: SKETCH,
        entity: plate.width,
    }));
}

#[test]
fn a_sketch_on_no_input_stands_on_its_world_plane() {
    let mut doc = document(plate().sketch);
    let record = doc
        .parts
        .get_mut(&PART)
        .unwrap()
        .features
        .get_mut(&SKETCH)
        .unwrap();
    record.inputs.clear();
    record.choices.insert("world".into(), "yz".into());
    let e = Evaluator::new(Registry::with_core_types()).evaluate(&doc);
    assert_eq!(view(&e).plane, arrix_core::Frame::WORLD_YZ);
}

const PAD: FeatureId = FeatureId(Id(4));

/// `test.pad`: its `region` input extruded `height`, the feature that
/// holds a region key here, as `core.extrude` will.
struct Pad(arrix_plugin_api::FeatureTypeSpec);

impl FeatureType for Pad {
    fn spec(&self) -> &arrix_plugin_api::FeatureTypeSpec {
        &self.0
    }

    fn evaluate(
        &self,
        kernel: &mut dyn arrix_plugin_api::Kernel,
        args: &FeatureArgs,
    ) -> Result<crate::registry::TypeOutput, Diagnostic> {
        let Some(InputValue::Region(profile)) = args.input("region").map(|i| &i.value) else {
            unreachable!("the evaluator resolves the region the spec asks for");
        };
        let body = kernel.extrude(profile, args.param("height").unwrap())?;
        Ok(arrix_plugin_api::FeatureOutput {
            slots: vec![arrix_plugin_api::SlotOutput {
                name: SlotName::new("body").unwrap(),
                value: OutputValue::Body(body),
            }],
        }
        .into())
    }
}

fn with_pad() -> Registry {
    use arrix_plugin_api::{InputSpec, ParamSpec, SlotSpec};
    let mut r = Registry::with_core_types();
    r.register(std::sync::Arc::new(Pad(
        arrix_plugin_api::FeatureTypeSpec {
            id: "test.pad".into(),
            version: 1,
            title: "Pad".into(),
            params: vec![ParamSpec {
                name: "height".into(),
                title: "Height".into(),
                kind: QuantityKind::Length,
                default: "5 mm".into(),
            }],
            inputs: vec![InputSpec {
                name: "region".into(),
                title: "Region".into(),
                kind: InputKind::Region,
            }],
            outputs: vec![SlotSpec {
                name: SlotName::new("body").unwrap(),
                kind: SlotKind::Body,
                modifies: None,
            }],
        },
    )))
    .unwrap();
    r
}

/// The plate document with a pad on the region `key` names.
fn padded(sketch: Sketch, key: &RegionKey) -> Document {
    let mut doc = document(sketch);
    let part = doc.parts.get_mut(&PART).unwrap();
    let pad = FeatureRecord {
        id: PAD,
        type_id: FeatureTypeId::new("test.pad").unwrap(),
        name: "Pad".into(),
        inputs: BTreeMap::from([(
            "region".into(),
            Ref::Region {
                feature: SKETCH,
                key: key.clone(),
            },
        )]),
        sketch: None,
        ..part.features[&SKETCH].clone()
    };
    part.history.push(PAD);
    part.features.insert(PAD, pad);
    doc
}

#[test]
fn a_region_reference_resolves_to_its_region_or_is_lost_with_candidates() {
    let plate = plate();
    let mut ev = Evaluator::new(with_pad());
    let first = ev.evaluate(&document(plate.sketch.clone()));
    let holed = view(&first).regions[0].key.clone();

    let e = ev.evaluate(&padded(plate.sketch.clone(), &holed));
    let Some(SlotView::Body(body)) = e.slot(PAD, "body") else {
        panic!("{:?}", e.outcome(PAD));
    };
    let want = (40.0 * 30.0 - std::f64::consts::PI * 25.0) * 5.0 * MM.powi(3);
    assert!((body.volume - want).abs() <= 1e-9 * want, "{}", body.volume);

    // The bore deleted: the sketch still evaluates, and the holed plate's
    // key answers to nothing, never to the plain rectangle.
    let mut bare = plate.sketch.clone();
    bare.remove_entity(plate.bore);
    let e = ev.evaluate(&padded(bare, &holed));
    let rectangle = view(&e).regions[0].key.clone();
    let lost = match e.outcome(PAD) {
        Some(FeatureOutcome::Failed { diagnostic, .. }) => diagnostic,
        other => panic!("{other:?}"),
    };
    assert_eq!(lost.code.as_str(), "ref.lost");
    assert_eq!(
        lost.refs,
        [Ref::Region {
            feature: SKETCH,
            key: holed
        }]
    );
    assert_eq!(
        lost.candidates,
        [Ref::Region {
            feature: SKETCH,
            key: rectangle
        }],
        "the plain rectangle, offered and not taken"
    );
}

#[test]
fn a_region_reference_reads_its_sketch_in_the_dag() {
    let plate = plate();
    let key = RegionKey::new([plate.bore], [0.0, 0.0]);
    let doc = padded(plate.sketch, &key);
    let dag = doc.validate().unwrap();
    assert!(
        dag.reads(Node::Feature(PAD))
            .any(|n| n == Node::Feature(SKETCH))
    );
}
