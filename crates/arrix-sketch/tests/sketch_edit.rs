//! `SketchEdit`: the whole-record puts and removals the document's
//! `SketchEdit` command carries. A diff applied gives the sketch it was
//! taken from, its inverse gives back the one before, and a removal that
//! would leave a reference dangling is refused, never swept.

use arrix_sketch::{Constraint, Draft, Point, Sketch, SketchEdit, SketchError};

fn rectangle() -> (Draft, [arrix_sketch::PointId; 4]) {
    let mut d = Draft::seeded(5);
    let p = [(0.0, 0.0), (0.04, 0.0), (0.04, 0.03), (0.0, 0.03)]
        .map(|(x, y)| d.add_point(Point::new(x, y)));
    for i in 0..4 {
        let line = d.add_entity(arrix_sketch::Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
        d.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
    }
    (d, p)
}

#[test]
fn a_diff_applies_to_the_sketch_it_was_taken_from_and_inverts() {
    let (mut d, p) = rectangle();
    let before = d.sketch.clone();
    let w = d.add_constraint(Constraint::Distance {
        a: p[0],
        b: p[1],
        value: 0.05,
    });
    d.set_constraint_expr(w, Some("w".into()));
    assert!(d.solve().converged);
    let construction = *d.entities().keys().next().unwrap();
    d.set_construction(construction, true);

    let edit = SketchEdit::diff(&before, &d);
    assert_eq!(edit.constraints.len(), 1);
    assert_eq!(edit.construction, [(construction, true)].into());
    let mut sketch = before.clone();
    let inverse = edit.apply(&mut sketch).unwrap();
    assert_eq!(sketch, d.sketch);
    inverse.apply(&mut sketch).unwrap();
    assert_eq!(sketch, before);
    assert!(SketchEdit::diff(&before, &before).is_empty());
}

#[test]
fn a_removal_names_what_stands_on_it_or_is_refused() {
    let (d, p) = rectangle();
    let mut sketch = d.sketch.clone();
    let bare = SketchEdit {
        points: [(p[0], None)].into(),
        ..SketchEdit::default()
    };
    assert!(matches!(
        bare.apply(&mut sketch),
        Err(SketchError::MissingPoint { point, .. }) if point == p[0]
    ));
    assert_eq!(sketch, d.sketch, "refused whole");

    // The point, both lines on it and their two constraints.
    let whole = SketchEdit::remove(&sketch, &[p[0]].into());
    assert_eq!(
        (
            whole.points.len(),
            whole.entities.len(),
            whole.constraints.len()
        ),
        (1, 2, 2)
    );
    let inverse = whole.apply(&mut sketch).unwrap();
    assert_eq!(sketch.entities().len(), 2);
    inverse.apply(&mut sketch).unwrap();
    assert_eq!(sketch, d.sketch);
}

#[test]
fn writes_null_for_a_removal_and_leaves_out_what_it_keeps() {
    let (d, p) = rectangle();
    let mut after = d.sketch.clone();
    after.point_mut(p[2]).unwrap().set_pos(0.05, 0.03);
    after.remove_constraint(*after.constraints().keys().next().unwrap());
    let edit = SketchEdit::diff(&d, &after);
    let json = serde_json::to_string(&edit).unwrap();
    assert_eq!(
        json,
        format!(
            r#"{{"points":{{"{}":{{"x":0.05,"y":0.03}}}},"constraints":{{"{}":null}}}}"#,
            p[2],
            d.constraints().keys().next().unwrap()
        )
    );
    assert_eq!(serde_json::from_str::<SketchEdit>(&json).unwrap(), edit);
    assert_eq!(
        serde_json::to_string(&SketchEdit::diff(&Sketch::new(), &Sketch::new())).unwrap(),
        "{}"
    );
}

fn edge_name(feature: u64) -> arrix_core::Ref {
    arrix_core::Ref::Topo(format!("edge:sweep.{feature:013}.end-cap").parse().unwrap())
}

#[test]
fn a_projected_entity_carries_its_source_through_edit_diff_and_json() {
    let (mut d, _) = rectangle();
    let before = d.sketch.clone();
    let line = *d.entities().keys().next().unwrap();
    let source = edge_name(7);
    d.set_source(line, Some(source.clone())).unwrap();
    assert_eq!(d.source(line), Some(&source));

    let edit = SketchEdit::diff(&before, &d);
    assert_eq!(edit.sources, [(line, Some(source.clone()))].into());
    let mut sketch = before.clone();
    let inverse = edit.apply(&mut sketch).unwrap();
    assert_eq!(sketch, d.sketch);
    assert_eq!(inverse.sources, [(line, None)].into());
    inverse.apply(&mut sketch).unwrap();
    assert_eq!(sketch, before);

    // The source is in the file and reads back; a sketch without one
    // writes no `sources` key, so a schema-2 sketch's bytes are unchanged.
    let json = serde_json::to_string(&d.sketch).unwrap();
    assert!(json.contains("\"sources\""));
    assert_eq!(serde_json::from_str::<Sketch>(&json).unwrap(), d.sketch);
    assert!(!serde_json::to_string(&before).unwrap().contains("sources"));
}

#[test]
fn a_source_is_a_topo_name_on_a_projectable_entity_and_goes_with_it() {
    let (mut d, p) = rectangle();
    let line = *d.entities().keys().next().unwrap();
    assert_eq!(
        d.set_source(
            line,
            Some(arrix_core::Ref::Param(arrix_core::ParamId(arrix_core::Id(
                1
            ))))
        ),
        Err(SketchError::SourceNotTopo { entity: line })
    );
    let nothing = arrix_sketch::SketchEntityId(arrix_core::Id(99));
    assert!(matches!(
        d.set_source(nothing, Some(edge_name(7))),
        Err(SketchError::MissingEntity { .. })
    ));
    // A point entity can be a vertex's projection.
    let lone = d.add_entity(arrix_sketch::Entity::Point(p[0]));
    d.set_source(lone, Some(edge_name(8))).unwrap();
    d.set_source(line, Some(edge_name(7))).unwrap();
    // A removal sweeps the source, and the edit that does it puts it back
    // on undo.
    let mut sketch = d.sketch.clone();
    let remove = SketchEdit::remove(&sketch, &[line].into());
    assert_eq!(remove.sources, [(line, None)].into());
    let inverse = remove.apply(&mut sketch).unwrap();
    assert!(sketch.source(line).is_none());
    inverse.apply(&mut sketch).unwrap();
    assert_eq!(sketch, d.sketch);
    // A stored sketch whose source names a missing entity does not read.
    let bad = r#"{"sources":{"0000000000063":{"topo":"edge:sweep.0000000000007.end-cap"}}}"#;
    assert!(serde_json::from_str::<Sketch>(bad).is_err());
}
