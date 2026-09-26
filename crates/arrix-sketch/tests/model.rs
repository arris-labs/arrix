//! The sketch model: ids its author mints, references that always resolve,
//! the JSON the document writes, construction marks and a dimension's
//! expression source (docs/DATA-MODEL.md §Sketches).

use arrix_core::{IdMinter, QuantityKind};
use arrix_sketch::{
    Constraint, ConstraintRecord, Draft, Entity, EntityId, Point, PointId, Sketch, SketchError,
};

fn round_trip(sketch: &Sketch) -> (String, Sketch) {
    let json = serde_json::to_string(sketch).expect("serialize sketch");
    let restored: Sketch = serde_json::from_str(&json).expect("deserialize sketch");
    (json, restored)
}

fn dimensioned_line() -> (Draft, arrix_sketch::ConstraintId) {
    let mut sketch = Draft::seeded(1);
    let (a, b, _) = sketch.add_line(0.0, 0.0, 0.08, 0.0);
    let id = sketch.add_constraint(Constraint::Distance { a, b, value: 0.08 });
    (sketch, id)
}

// ─── ids ─────────────────────────────────────────────────────────────────

#[test]
fn ids_are_the_callers_and_one_space() {
    let mut ids = IdMinter::new(9);
    let (p, q, line): (PointId, PointId, EntityId) = (ids.mint(), ids.mint(), ids.mint());
    let mut sketch = Sketch::new();
    sketch.insert_point(p, Point::new(0.0, 0.0)).unwrap();
    sketch.insert_point(q, Point::new(0.01, 0.0)).unwrap();
    sketch
        .insert_entity(line, Entity::Line { start: p, end: q })
        .unwrap();
    assert_eq!(
        sketch.entity(line),
        Some(&Entity::Line { start: p, end: q })
    );

    // A constraint may not take a point's id, nor an entity a point's.
    let taken = ConstraintRecord::new(p, Constraint::Horizontal { line });
    assert_eq!(
        sketch.insert_constraint(taken),
        Err(SketchError::IdTaken(p))
    );
    assert_eq!(
        sketch.insert_entity(q, Entity::Point(p)),
        Err(SketchError::IdTaken(q))
    );
    assert_eq!(sketch.constraints().len(), 0);
}

#[test]
fn a_draft_mints_from_its_seed_and_skips_what_the_sketch_holds() {
    let mut a = Draft::seeded(3);
    let mut b = Draft::seeded(3);
    let (pa, _, la) = a.add_line(0.0, 0.0, 1.0, 0.0);
    let (pb, _, lb) = b.add_line(0.0, 0.0, 1.0, 0.0);
    assert_eq!((pa, la), (pb, lb), "the same seed mints the same ids");

    // A second draft over the same sketch and seed does not collide.
    let mut again = Draft::with_sketch(a.sketch.clone(), IdMinter::new(3));
    let p = again.add_point(Point::new(2.0, 2.0));
    assert!(!a.contains_id(p));
    assert_eq!(again.points().len(), 3);
}

#[test]
fn a_reference_must_resolve() {
    let mut ids = IdMinter::new(4);
    let (p, missing, e, c): (PointId, PointId, EntityId, _) =
        (ids.mint(), ids.mint(), ids.mint(), ids.mint());
    let mut sketch = Sketch::new();
    sketch.insert_point(p, Point::new(0.0, 0.0)).unwrap();
    assert_eq!(
        sketch.insert_entity(
            e,
            Entity::Line {
                start: p,
                end: missing
            }
        ),
        Err(SketchError::MissingPoint {
            by: e,
            point: missing
        })
    );
    assert_eq!(
        sketch.insert_constraint(ConstraintRecord::new(c, Constraint::Horizontal { line: e })),
        Err(SketchError::MissingEntity { by: c, entity: e })
    );
    assert!(sketch.entities().is_empty() && sketch.constraints().is_empty());
}

#[test]
fn removing_a_point_takes_what_was_built_on_it() {
    let mut s = Draft::seeded(5);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.05, 0.0);
    let h = s.add_constraint(Constraint::Horizontal { line });
    let d = s.add_constraint(Constraint::Distance { a, b, value: 0.05 });
    let (_, circle) = s.add_circle(0.0, 0.0, 0.01);
    let keep = s.add_constraint(Constraint::Radius {
        target: circle,
        value: 0.01,
    });
    s.remove_point(a);
    assert!(s.entity(line).is_none());
    assert!(
        s.get_constraint(h).is_none(),
        "a constraint on the line goes"
    );
    assert!(
        s.get_constraint(d).is_none(),
        "a constraint on the point goes"
    );
    assert!(s.get_constraint(keep).is_some());
    assert!(s.point(b).is_some(), "the line's other end stays");
    // What is left still reads back as a valid sketch.
    assert_eq!(round_trip(&s).1, s.sketch);
}

// ─── the written form ────────────────────────────────────────────────────

#[test]
fn a_sketch_round_trips_in_the_documents_form() {
    let mut s = Draft::seeded(1);
    let origin = s.add_point(Point::fixed(0.0, 0.0));
    let (a, b, line) = s.add_line(0.0, 0.0, 0.04, 0.0);
    s.add_constraint(Constraint::Coincident { a: origin, b: a });
    s.add_constraint(Constraint::Horizontal { line });
    let width = s.add_constraint(Constraint::Distance { a, b, value: 0.04 });
    s.set_constraint_expr(width, Some("w".into()));
    let (_, arc_start, _, arc) = s.add_arc(0.0, 0.01, 0.01, 0.01, 0.0, 0.02);
    s.add_constraint(Constraint::PointOnDatum {
        point: arc_start,
        datum: arrix_sketch::DatumEntity::AxisX,
    });
    s.add_reference_constraint(Constraint::Radius {
        target: arc,
        value: 0.01,
    });

    let (json, restored) = round_trip(&s);
    assert_eq!(restored, s.sketch);
    // Ids are the document's text form; kinds are snake_case; defaults are
    // left out; there is no id counter.
    assert!(json.contains(&format!(
        "\"{origin}\":{{\"x\":0.0,\"y\":0.0,\"fixed\":true}}"
    )));
    assert!(json.contains(&format!(
        "{{\"line\":{{\"start\":\"{a}\",\"end\":\"{b}\"}}}}"
    )));
    assert!(json.contains(&format!("\"horizontal\":{{\"line\":\"{line}\"}}")));
    assert!(json.contains("\"datum\":\"axis_x\""), "{json}");
    assert!(json.contains("\"is_driving\":false"), "{json}");
    assert!(
        !json.contains("is_active") && !json.contains("priority"),
        "{json}"
    );
    assert!(!json.contains("next_id") && !json.contains("construction"));
    // And the bytes are stable.
    assert_eq!(serde_json::to_string(&restored).unwrap(), json);
}

#[test]
fn an_empty_sketch_writes_an_empty_object() {
    assert_eq!(serde_json::to_string(&Sketch::new()).unwrap(), "{}");
    assert_eq!(serde_json::from_str::<Sketch>("{}").unwrap(), Sketch::new());
}

#[test]
fn a_file_with_a_dangling_reference_or_a_wrong_key_is_refused() {
    let (s, id) = dimensioned_line();
    let json = serde_json::to_string(&s.sketch).unwrap();

    let point = *s.points().keys().next().unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    value["points"]
        .as_object_mut()
        .unwrap()
        .remove(&point.to_string());
    let err = serde_json::from_value::<Sketch>(value).unwrap_err();
    assert!(err.to_string().contains("names point"), "{err}");

    let mut value: serde_json::Value = serde_json::from_str(&json).unwrap();
    let constraints = value["constraints"].as_object_mut().unwrap();
    let record = constraints.remove(&id.to_string()).unwrap();
    constraints.insert("0000000000001".into(), record);
    let err = serde_json::from_value::<Sketch>(value).unwrap_err();
    assert!(err.to_string().contains("says its id is"), "{err}");

    let err = serde_json::from_str::<Sketch>(r#"{"next_id":4}"#).unwrap_err();
    assert!(err.to_string().contains("unknown field"), "{err}");
}

#[cfg(not(feature = "snells-law"))]
#[test]
fn a_gated_kind_does_not_deserialise_on_the_default_build() {
    let c = r#"{"snells_law":{"ray1_start":"0000000000001","ray1_end":"0000000000002",
        "ray2_end":"0000000000003","boundary":"0000000000004","ratio":1.33}}"#;
    let err = serde_json::from_str::<Constraint>(c).unwrap_err();
    assert!(err.to_string().contains("unknown variant"), "{err}");
}

#[cfg(not(feature = "conics"))]
#[test]
fn a_conic_does_not_deserialise_on_the_default_build() {
    let e = r#"{"ellipse":{"center":"0000000000001","major_axis_end":"0000000000002",
        "minor_radius":0.01}}"#;
    assert!(serde_json::from_str::<Entity>(e).is_err());
    let c = r#"{"minor_radius":{"ellipse":"0000000000001","value":0.01}}"#;
    assert!(serde_json::from_str::<Constraint>(c).is_err());
}

// ─── constraint helpers ──────────────────────────────────────────────────

#[test]
fn s47_constraint_helper_methods() {
    let mut ids = IdMinter::new(47);
    let (p1, p2, p3): (PointId, PointId, PointId) = (ids.mint(), ids.mint(), ids.mint());
    let (e1, e2): (EntityId, EntityId) = (ids.mint(), ids.mint());
    let elsewhere: PointId = ids.mint();

    let mut c_dist = Constraint::Distance {
        a: p1,
        b: p2,
        value: 0.05,
    };
    assert!(c_dist.is_dimensional());
    assert_eq!(c_dist.dimensional_value(), Some(0.05));
    assert_eq!(c_dist.dimension_kind(), Some(QuantityKind::Length));
    assert!(c_dist.set_dimensional_value(0.08));
    assert_eq!(c_dist.dimensional_value(), Some(0.08));
    assert_eq!(c_dist.referenced_points(), vec![p1, p2]);
    assert!(c_dist.uses_point(p1));
    assert!(!c_dist.uses_point(elsewhere));

    let c_h = Constraint::Horizontal { line: e1 };
    assert!(!c_h.is_dimensional());
    assert_eq!(c_h.dimensional_value(), None);
    assert_eq!(c_h.referenced_entities(), vec![e1]);
    assert!(c_h.uses_entity(e1));
    assert!(!c_h.uses_entity(e2));

    let c_conc = Constraint::Concentric { a: e1, b: e2 };
    assert_eq!(c_conc.residual_count(), 2);
    assert_eq!(c_conc.referenced_entities(), vec![e1, e2]);

    let c_diam = Constraint::Diameter {
        target: e1,
        value: 0.1,
    };
    assert_eq!(c_diam.dimensional_value(), Some(0.1));

    let c_symm_pts = Constraint::symmetric_points(p1, p2, p3);
    assert_eq!(c_symm_pts.residual_count(), 2);
    assert_eq!(c_symm_pts.referenced_points(), vec![p1, p2, p3]);
    assert!(!c_symm_pts.is_dimensional());

    let c_perp_bis = Constraint::point_on_perp_bisector(p1, p2, p3);
    assert_eq!(c_perp_bis.residual_count(), 1);
    assert_eq!(c_perp_bis.referenced_points(), vec![p1, p2, p3]);

    let mut c_arc_len = Constraint::arc_length(e1, 0.05);
    assert_eq!(c_arc_len.residual_count(), 1);
    assert!(c_arc_len.set_dimensional_value(0.07));
    assert_eq!(c_arc_len.dimensional_value(), Some(0.07));
    assert_eq!(c_arc_len.referenced_entities(), vec![e1]);

    assert_eq!(Constraint::block(e1).referenced_entities(), vec![e1]);

    let mut c_ang_pts = Constraint::angle_points(p1, p2, p3, 1.57);
    assert_eq!(c_ang_pts.dimension_kind(), Some(QuantityKind::Angle));
    assert!(c_ang_pts.set_dimensional_value(0.785));
    assert_eq!(c_ang_pts.dimensional_value(), Some(0.785));

    for mut c in [
        Constraint::distance_to_axis_x(p1, 0.04),
        Constraint::distance_to_axis_y(p1, 0.04),
        Constraint::distance_circle_circle(e1, e2, 0.04),
        Constraint::distance_point_circle(p1, e1, 0.04),
    ] {
        assert_eq!(c.dimensional_value(), Some(0.04));
        assert!(c.set_dimensional_value(0.06));
        assert_eq!(c.dimensional_value(), Some(0.06));
    }
}

#[cfg(feature = "snells-law")]
#[test]
fn snells_law_drives_a_ratio() {
    let mut ids = IdMinter::new(48);
    let (p1, p2, p3, e1) = (ids.mint(), ids.mint(), ids.mint(), ids.mint());
    let mut c = Constraint::snells_law(p1, p2, p3, e1, 1.33);
    assert_eq!(c.dimension_kind(), Some(QuantityKind::Ratio));
    assert!(c.set_dimensional_value(1.5));
    assert_eq!(c.dimensional_value(), Some(1.5));
}

// ─── construction marks ──────────────────────────────────────────────────

#[test]
fn a_construction_mark_round_trips() {
    let mut sketch = Draft::seeded(1);
    let (_, circle) = sketch.add_circle(0.0, 0.0, 0.02);
    let (_, _, line) = sketch.add_line(-0.03, -0.03, 0.03, 0.03);
    assert!(sketch.set_construction(circle, true));
    assert!(sketch.is_construction(circle));
    assert!(!sketch.is_construction(line));

    let (json, restored) = round_trip(&sketch);
    assert!(json.contains("\"construction\":["), "{json}");
    assert!(restored.is_construction(circle));
    assert!(!restored.is_construction(line));
    assert_eq!(restored, sketch.sketch);

    assert!(sketch.set_construction(circle, false));
    assert!(sketch.construction().is_empty());
}

#[test]
fn marking_a_missing_entity_changes_nothing() {
    let mut sketch = Draft::seeded(1);
    let point = sketch.add_point(Point::new(0.0, 0.0));
    assert!(
        !sketch.set_construction(point, true),
        "a point is no entity"
    );
    assert!(sketch.construction().is_empty());
}

#[test]
fn removing_an_entity_removes_its_mark() {
    let mut sketch = Draft::seeded(1);
    let (_, circle) = sketch.add_circle(0.0, 0.0, 0.02);
    sketch.set_construction(circle, true);
    sketch.remove_entity(circle);
    assert!(sketch.construction().is_empty());
}

#[test]
fn removing_a_point_removes_the_marks_of_entities_built_on_it() {
    let mut sketch = Draft::seeded(1);
    let (start, _, line) = sketch.add_line(0.0, 0.0, 0.05, 0.0);
    let (_, circle) = sketch.add_circle(0.0, 0.0, 0.02);
    sketch.set_construction(line, true);
    sketch.set_construction(circle, true);
    sketch.remove_point(start);
    assert!(sketch.entity(line).is_none());
    assert!(!sketch.is_construction(line));
    assert!(sketch.is_construction(circle), "an unrelated mark stays");
}

// ─── a dimension's expression source ─────────────────────────────────────

#[test]
fn an_expression_source_round_trips() {
    let (mut sketch, id) = dimensioned_line();
    assert!(sketch.set_constraint_expr(id, Some("plate_w / 2".into())));
    assert_eq!(sketch.constraint_expr(id), Some("plate_w / 2"));

    let (json, restored) = round_trip(&sketch);
    assert!(json.contains("\"expr\":\"plate_w / 2\""), "{json}");
    assert_eq!(restored.constraint_expr(id), Some("plate_w / 2"));
    assert_eq!(restored, sketch.sketch);
}

#[test]
fn a_number_only_dimension_writes_no_expr_key() {
    let (sketch, id) = dimensioned_line();
    assert_eq!(sketch.constraint_expr(id), None);
    let (json, restored) = round_trip(&sketch);
    assert!(
        !json.contains("expr"),
        "an absent source must not reach disk: {json}"
    );
    assert_eq!(restored, sketch.sketch);
}

#[test]
fn clearing_a_source_and_a_missing_record_behave() {
    let (mut sketch, id) = dimensioned_line();
    sketch.set_constraint_expr(id, Some("plate_w".into()));
    assert!(sketch.set_constraint_expr(id, None));
    assert_eq!(sketch.constraint_expr(id), None);

    let missing = sketch.mint();
    assert!(!sketch.set_constraint_expr(missing, Some("plate_w".into())));
    assert_eq!(sketch.constraint_expr(missing), None);
}

#[test]
fn removing_a_constraint_removes_its_source() {
    let (mut sketch, id) = dimensioned_line();
    sketch.set_constraint_expr(id, Some("plate_w".into()));
    assert!(sketch.remove_constraint(id));
    assert_eq!(sketch.constraint_expr(id), None);
    assert!(
        !serde_json::to_string(&sketch.sketch)
            .unwrap()
            .contains("plate_w")
    );
}

#[test]
fn removing_a_dimensioned_point_removes_its_source() {
    let (mut sketch, id) = dimensioned_line();
    sketch.set_constraint_expr(id, Some("plate_w".into()));
    let point = *sketch.points().keys().next().unwrap();
    sketch.remove_point(point);
    assert!(sketch.get_constraint_record(id).is_none());
    assert!(
        !serde_json::to_string(&sketch.sketch)
            .unwrap()
            .contains("plate_w")
    );
}
