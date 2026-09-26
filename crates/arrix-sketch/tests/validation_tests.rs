//! Validation: open vertices, the gaps between them, degenerate geometry
//! with its purge, and the report that adds the regions a sketch closes.

use arrix_sketch::{Constraint, DegenerateKind, Draft, Entity, Point, ValidationOptions};

#[test]
fn test_open_vertex_detection_open_chain() {
    let mut sketch = Draft::seeded(1);
    // A chain of 3 lines: p0 -> p1, p1 -> p2, p2 -> p3 (p0 and p3 are open)
    let p0 = sketch.add_point(Point::new(0.0, 0.0));
    let p1 = sketch.add_point(Point::new(10.0, 0.0));
    let p2 = sketch.add_point(Point::new(10.0, 10.0));
    let p3 = sketch.add_point(Point::new(0.0, 10.0));

    sketch.add_entity(Entity::Line { start: p0, end: p1 });
    sketch.add_entity(Entity::Line { start: p1, end: p2 });
    sketch.add_entity(Entity::Line { start: p2, end: p3 });

    let open = sketch.detect_open_vertices();
    assert_eq!(open.len(), 2);
    let open_ids: Vec<_> = open.iter().map(|v| v.point_id).collect();
    assert!(open_ids.contains(&p0));
    assert!(open_ids.contains(&p3));
    assert!(!open_ids.contains(&p1));
    assert!(!open_ids.contains(&p2));
}

#[test]
fn test_open_vertex_detection_closed_rectangle() {
    let mut sketch = Draft::seeded(1);
    let p: Vec<_> = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
        .iter()
        .map(|&(x, y)| sketch.add_point(Point::new(x, y)))
        .collect();
    for i in 0..4 {
        sketch.add_entity(Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
    }
    assert!(sketch.detect_open_vertices().is_empty());
}

#[test]
fn test_open_vertex_coincident_cluster() {
    let mut sketch = Draft::seeded(1);
    // Two lines whose ends are separate points tied by a coincidence.
    let (_, e1, _) = sketch.add_line(0.0, 0.0, 10.0, 0.0);
    let (s2, _, _) = sketch.add_line(10.0, 0.0, 10.0, 10.0);
    assert_eq!(sketch.detect_open_vertices().len(), 4);
    let tie = sketch.add_constraint(Constraint::Coincident { a: e1, b: s2 });
    assert_eq!(sketch.detect_open_vertices().len(), 2);
    // A suppressed coincidence joins nothing.
    sketch.set_constraint_active(tie, false);
    assert_eq!(sketch.detect_open_vertices().len(), 4);
}

#[test]
fn a_construction_line_has_no_open_ends() {
    let mut sketch = Draft::seeded(1);
    let (_, _, line) = sketch.add_line(0.0, 0.0, 10.0, 0.0);
    assert_eq!(sketch.detect_open_vertices().len(), 2);
    sketch.set_construction(line, true);
    assert!(sketch.detect_open_vertices().is_empty());
}

#[test]
fn test_detect_open_gaps() {
    let mut sketch = Draft::seeded(1);
    // Rectangle with a 0.05 mm open gap between p3's line end and p0.
    let p0 = sketch.add_point(Point::new(0.0, 0.0));
    let p1 = sketch.add_point(Point::new(0.010, 0.0));
    let p2 = sketch.add_point(Point::new(0.010, 0.010));
    let p3 = sketch.add_point(Point::new(0.0, 0.010));
    let p0_end = sketch.add_point(Point::new(0.0, 0.00005));

    sketch.add_entity(Entity::Line { start: p0, end: p1 });
    sketch.add_entity(Entity::Line { start: p1, end: p2 });
    sketch.add_entity(Entity::Line { start: p2, end: p3 });
    sketch.add_entity(Entity::Line {
        start: p3,
        end: p0_end,
    });

    let gaps = sketch.detect_open_gaps(0.0001);
    assert_eq!(gaps.len(), 1);
    assert!((gaps[0].distance - 0.00005).abs() < 1e-9);
    assert!(
        sketch.detect_open_gaps(0.00001).is_empty(),
        "past tolerance"
    );

    // Closed by a coincidence, the gap is gone.
    sketch.add_constraint(Constraint::Coincident { a: p0, b: p0_end });
    assert!(sketch.detect_open_gaps(0.0001).is_empty());
    assert!(sketch.detect_open_vertices().is_empty());
}

#[test]
fn test_detect_and_purge_degenerate_geometries() {
    let mut sketch = Draft::seeded(1);
    // Valid line, constrained, so it is the survivor of its duplicate.
    let (p1, p2, l1) = sketch.add_line(0.0, 0.0, 0.02, 0.0);
    sketch.add_constraint(Constraint::Horizontal { line: l1 });

    // Zero-length line
    let (_p3, _p4, l_zero) = sketch.add_line(0.05, 0.05, 0.05, 0.05);

    // Zero-radius circle
    let (_pc, c_zero) = sketch.add_circle(0.01, 0.01, 0.0);

    // Duplicate line over l1
    let (_p5, _p6, l_dup) = sketch.add_line(0.0, 0.0, 0.02, 0.0);

    let degenerates = sketch.detect_degenerate_geometries(1e-4);
    assert_eq!(degenerates.len(), 3);

    let deg_kinds: Vec<_> = degenerates.iter().map(|d| d.kind).collect();
    assert!(deg_kinds.contains(&DegenerateKind::ZeroLengthLine));
    assert!(deg_kinds.contains(&DegenerateKind::ZeroRadiusCircle));
    assert!(deg_kinds.contains(&DegenerateKind::DuplicateLine));
    let dup = degenerates
        .iter()
        .find(|d| d.kind == DegenerateKind::DuplicateLine)
        .unwrap();
    assert_eq!((dup.entity_id, dup.duplicate_of), (l_dup, Some(l1)));

    let purged = sketch.purge_degenerate_geometries(1e-4);
    assert_eq!(purged.len(), 3);
    assert!(purged.contains(&l_zero));
    assert!(purged.contains(&c_zero));
    assert!(purged.contains(&l_dup));
    assert!(!purged.contains(&l1));

    // Only the valid line, its constraint and its points remain.
    assert_eq!(sketch.entities().len(), 1);
    assert!(sketch.entities().contains_key(&l1));
    assert_eq!(sketch.constraints().len(), 1);
    assert_eq!(sketch.points().len(), 2);
    assert!(sketch.points().contains_key(&p1));
    assert!(sketch.points().contains_key(&p2));

    assert!(sketch.detect_degenerate_geometries(1e-4).is_empty());
}

#[test]
fn test_detect_and_purge_degenerate_arcs_and_circles() {
    let mut sketch = Draft::seeded(1);
    let (_c1, circle1) = sketch.add_circle(0.0, 0.0, 0.05);
    sketch.add_constraint(Constraint::Radius {
        target: circle1,
        value: 0.05,
    });
    let (_c2, circle_dup) = sketch.add_circle(0.0, 0.0, 0.05);

    let (_ca, _sa, _ea, arc1) = sketch.add_arc(0.1, 0.1, 0.12, 0.1, 0.1, 0.12);
    sketch.add_constraint(Constraint::Radius {
        target: arc1,
        value: 0.02,
    });
    let (_ca2, _sa2, _ea2, arc_dup) = sketch.add_arc(0.1, 0.1, 0.12, 0.1, 0.1, 0.12);

    // Degenerate collapsed arc (center == start)
    let (_ca3, _sa3, _ea3, arc_collapsed) = sketch.add_arc(0.2, 0.2, 0.2, 0.2, 0.25, 0.25);

    let degenerates = sketch.detect_degenerate_geometries(1e-4);
    assert_eq!(degenerates.len(), 3);

    let purged = sketch.purge_degenerate_geometries(1e-4);
    assert_eq!(purged.len(), 3);
    assert!(purged.contains(&circle_dup));
    assert!(purged.contains(&arc_dup));
    assert!(purged.contains(&arc_collapsed));
    assert!(sketch.entities().contains_key(&circle1));
    assert!(sketch.entities().contains_key(&arc1));
}

#[test]
fn of_two_equal_duplicates_the_smaller_id_survives() {
    let mut sketch = Draft::seeded(1);
    let (_, _, a) = sketch.add_line(0.0, 0.0, 0.02, 0.0);
    let (_, _, b) = sketch.add_line(0.02, 0.0, 0.0, 0.0);
    let found = sketch.detect_degenerate_geometries(1e-4);
    assert_eq!(found.len(), 1, "a line reversed is still the same line");
    assert_eq!(found[0].duplicate_of, Some(a.min(b)));
    assert_eq!(found[0].entity_id, a.max(b));
}

#[test]
fn an_arc_with_swapped_ends_is_its_complement_not_a_duplicate() {
    let mut sketch = Draft::seeded(1);
    sketch.add_arc(0.0, 0.0, 0.01, 0.0, 0.0, 0.01);
    sketch.add_arc(0.0, 0.0, 0.0, 0.01, 0.01, 0.0);
    assert!(sketch.detect_degenerate_geometries(1e-4).is_empty());
}

#[test]
fn purging_a_degenerate_entity_removes_its_mark() {
    let mut sketch = Draft::seeded(1);
    let a = sketch.add_point(Point::new(0.01, 0.01));
    let b = sketch.add_point(Point::new(0.01, 0.01));
    let collapsed = sketch.add_entity(Entity::Line { start: a, end: b });
    let (_, circle) = sketch.add_circle(0.0, 0.0, 0.02);
    sketch.add_constraint(Constraint::Radius {
        target: circle,
        value: 0.02,
    });
    sketch.set_construction(collapsed, true);
    sketch.set_construction(circle, true);

    let removed = sketch.purge_degenerate_geometries(1e-9);
    assert_eq!(removed, vec![collapsed]);
    assert!(!sketch.is_construction(collapsed));
    assert!(sketch.is_construction(circle));
}

#[test]
fn a_purge_keeps_a_point_something_else_uses() {
    let mut sketch = Draft::seeded(1);
    let anchor = sketch.add_point(Point::fixed(0.0, 0.0));
    let collapsed = sketch.add_entity(Entity::Line {
        start: anchor,
        end: anchor,
    });
    let (c, circle) = sketch.add_circle(0.0, 0.0, 0.02);
    sketch.add_constraint(Constraint::Coincident { a: anchor, b: c });
    assert_eq!(sketch.purge_degenerate_geometries(1e-4), vec![collapsed]);
    assert!(
        sketch.point(anchor).is_some(),
        "the coincidence still uses it"
    );
    assert!(sketch.entity(circle).is_some());
}

#[test]
fn test_open_vertex_detection_with_arcs() {
    let mut sketch = Draft::seeded(1);
    // Arc from (10, 0) to (0, 10) centered at (0, 0)
    let (_c, s, e, _arc) = sketch.add_arc(0.0, 0.0, 10.0, 0.0, 0.0, 10.0);
    let p_origin = sketch.add_point(Point::new(0.0, 0.0));
    sketch.add_entity(Entity::Line {
        start: e,
        end: p_origin,
    });

    let open = sketch.detect_open_vertices();
    assert_eq!(open.len(), 2);
    let open_pids: Vec<_> = open.iter().map(|v| v.point_id).collect();
    assert!(open_pids.contains(&s));
    assert!(open_pids.contains(&p_origin));

    // Complete the pie slice by adding the closing line
    sketch.add_entity(Entity::Line {
        start: p_origin,
        end: s,
    });
    assert!(sketch.detect_open_vertices().is_empty());
}

// ─── the validation report ───────────────────────────────────────────────

#[test]
fn a_closed_rectangle_is_valid_for_extrusion() {
    let mut sketch = Draft::seeded(1);
    let p: Vec<_> = [(0.0, 0.0), (0.01, 0.0), (0.01, 0.01), (0.0, 0.01)]
        .iter()
        .map(|&(x, y)| sketch.add_point(Point::new(x, y)))
        .collect();
    for i in 0..4 {
        sketch.add_entity(Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
    }
    let report = sketch.validate_sketch(&ValidationOptions::default());
    assert!(report.open_vertices.is_empty());
    assert_eq!(report.closed_profiles_count, 1);
    assert_eq!(report.total_loops_count, 1);
    assert!(report.is_fully_closed);
    assert!(report.is_valid_for_extrusion);
}

#[test]
fn an_open_chain_is_not_closed_and_a_collapsed_curve_is_not_extrudable() {
    let mut sketch = Draft::seeded(1);
    let (_, _, _) = sketch.add_line(0.0, 0.0, 0.01, 0.0);
    let report = sketch.validate_sketch(&ValidationOptions::default());
    assert_eq!(report.open_vertices.len(), 2);
    assert!(!report.is_fully_closed && !report.is_valid_for_extrusion);

    let mut sketch = Draft::seeded(1);
    sketch.add_circle(0.0, 0.0, 0.01);
    sketch.add_circle(0.03, 0.0, 0.0);
    let report = sketch.validate_sketch(&ValidationOptions::default());
    assert!(report.is_fully_closed);
    assert_eq!(report.degenerate_entities.len(), 1);
    assert!(!report.is_valid_for_extrusion);
}

#[test]
fn a_pie_slice_closes_and_a_washer_counts_its_hole() {
    let mut sketch = Draft::seeded(1);
    let (_c, s, e, _arc) = sketch.add_arc(0.0, 0.0, 0.01, 0.0, 0.0, 0.01);
    let o = sketch.add_point(Point::new(0.0, 0.0));
    sketch.add_entity(Entity::Line { start: e, end: o });
    sketch.add_entity(Entity::Line { start: o, end: s });
    let report = sketch.validate_sketch(&ValidationOptions::default());
    assert!(report.is_fully_closed);
    assert_eq!(report.closed_profiles_count, 1);

    let mut washer = Draft::seeded(1);
    washer.add_circle(0.0, 0.0, 0.02);
    washer.add_circle(0.0, 0.0, 0.01);
    let report = washer.validate_sketch(&ValidationOptions::default());
    assert_eq!(report.closed_profiles_count, 2, "the ring and the bore");
    assert_eq!(report.total_loops_count, 3);
}
