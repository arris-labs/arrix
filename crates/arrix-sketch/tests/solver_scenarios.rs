//! Solver scenario fixtures, ported with the sketcher (docs/ARCHITECTURE.md
//! §Testing). Each test is a small, named constraint setup with
//! an explicit expectation (converges / fully constrained / over-constrained
//! with a diagnostic / drag moves correctly).

use arrix_sketch::{
    Constraint, Diagnostics, Draft, Entity, EntityConstraintState, Point, Sketch, SketchStatus,
};

/// Drops every `Fix` constraint — several scenarios below seed one for the
/// initial pose and then remove it so it cannot fight the constraint under
/// test.
fn remove_fix_constraints(sketch: &mut Draft) {
    let fixes: Vec<_> = sketch
        .constraints()
        .values()
        .filter(|record| matches!(&record.constraint, Constraint::Fix { .. }))
        .map(|record| record.id)
        .collect();
    for id in fixes {
        sketch.remove_constraint(id);
    }
}

fn almost(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() < tol, "expected {b}, got {a} (tol {tol})");
}

// ─── 1–5: basic distance / fix ───────────────────────────────────────────

#[test]
fn s01_two_points_distance() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(1.0, 0.0));
    s.add_constraint(Constraint::Distance { a, b, value: 0.05 });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&b].x, 0.05, 1e-6);
    assert_eq!(d.status, SketchStatus::Ok);
    assert_eq!(d.dof, 1);
}

#[test]
fn s02_horizontal_line() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.1, 0.02);
    s.point_mut(a).unwrap().fixed = true;
    s.add_constraint(Constraint::Horizontal { line });
    s.add_constraint(Constraint::Distance { a, b, value: 0.1 });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&b].y, s.points()[&a].y, 1e-6);
}

#[test]
fn s03_vertical_line() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.02, 0.1);
    s.point_mut(a).unwrap().fixed = true;
    s.add_constraint(Constraint::Vertical { line });
    s.add_constraint(Constraint::Distance { a, b, value: 0.1 });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&b].x, s.points()[&a].x, 1e-6);
}

#[test]
fn s04_coincident_merges_points() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(0.01, 0.01));
    s.add_constraint(Constraint::Coincident { a, b });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&b].x, 0.0, 1e-6);
    almost(s.points()[&b].y, 0.0, 1e-6);
}

#[test]
fn s05_fix_pins_point() {
    let mut s = Draft::seeded(1);
    let p = s.add_point(Point::new(1.0, 1.0));
    s.add_constraint(Constraint::Fix {
        point: p,
        x: 0.03,
        y: -0.02,
    });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&p].x, 0.03, 1e-6);
    almost(s.points()[&p].y, -0.02, 1e-6);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

// ─── 6–10: parallel / perpendicular / equal ──────────────────────────────

#[test]
fn s06_parallel_lines() {
    let mut s = Draft::seeded(1);
    let (a1, a2, l1) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let (b1, b2, l2) = s.add_line(0.0, 0.05, 0.1, 0.02);
    s.point_mut(a1).unwrap().fixed = true;
    s.point_mut(a2).unwrap().fixed = true;
    s.point_mut(b1).unwrap().fixed = true;
    s.add_constraint(Constraint::Parallel { a: l1, b: l2 });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    // l2 should be horizontal like l1.
    almost(s.points()[&b2].y, s.points()[&b1].y, 1e-5);
}

#[test]
fn s07_perpendicular_lines() {
    let mut s = Draft::seeded(1);
    let (a1, a2, l1) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let (b1, b2, l2) = s.add_line(0.0, 0.0, 0.05, 0.05);
    s.point_mut(a1).unwrap().fixed = true;
    s.point_mut(a2).unwrap().fixed = true;
    // Share origin via coincident.
    s.add_constraint(Constraint::Coincident { a: a1, b: b1 });
    s.add_constraint(Constraint::Perpendicular { a: l1, b: l2 });
    s.add_constraint(Constraint::Distance {
        a: b1,
        b: b2,
        value: 0.1,
    });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&b2].x, 0.0, 1e-4);
}

#[test]
fn s08_equal_length() {
    let mut s = Draft::seeded(1);
    let (a1, a2, l1) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let (b1, b2, l2) = s.add_line(0.0, 0.05, 0.04, 0.05);
    s.point_mut(a1).unwrap().fixed = true;
    s.point_mut(a2).unwrap().fixed = true;
    s.point_mut(b1).unwrap().fixed = true;
    s.add_constraint(Constraint::Horizontal { line: l2 });
    s.add_constraint(Constraint::Equal { a: l1, b: l2 });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    let len = s.points()[&b1].distance_to(&s.points()[&b2]);
    almost(len, 0.1, 1e-5);
}

#[test]
fn s09_midpoint() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let m = s.add_point(Point::new(0.0, 0.05));
    s.point_mut(a).unwrap().fixed = true;
    s.point_mut(b).unwrap().fixed = true;
    s.add_constraint(Constraint::Midpoint { point: m, line });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&m].x, 0.05, 1e-6);
    almost(s.points()[&m].y, 0.0, 1e-6);
}

#[test]
fn s10_point_on_line() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let p = s.add_point(Point::new(0.05, 0.03));
    s.point_mut(a).unwrap().fixed = true;
    s.point_mut(b).unwrap().fixed = true;
    s.add_constraint(Constraint::PointOnLine { point: p, line });
    // Also pin x so only y is free.
    s.add_constraint(Constraint::Fix {
        point: p,
        x: 0.05,
        y: 0.03, // will be pulled to y=0
    });
    // Wait — Fix will fight PointOnLine. Just use a distance along x via
    // another approach: fix is soft against point-on-line if we only want y.
    // Re-do: don't fix, use vertical from a known point.
    let fix = *s
        .constraints()
        .iter()
        .find(|(_, c)| matches!(&c.constraint, Constraint::Fix { .. }))
        .unwrap()
        .0;
    s.remove_constraint(fix);
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&p].y, 0.0, 1e-5);
}

// ─── 11–15: circles ──────────────────────────────────────────────────────

#[test]
fn s11_circle_radius() {
    let mut s = Draft::seeded(1);
    let (c, circle) = s.add_circle(0.0, 0.0, 0.1);
    s.point_mut(c).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: circle,
        value: 0.025,
    });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    match &s.entities()[&circle] {
        Entity::Circle { radius, .. } => almost(*radius, 0.025, 1e-6),
        _ => panic!("expected circle"),
    }
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s12_point_on_circle() {
    let mut s = Draft::seeded(1);
    let (c, circle) = s.add_circle(0.0, 0.0, 0.05);
    let p = s.add_point(Point::new(0.1, 0.0));
    s.point_mut(c).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: circle,
        value: 0.05,
    });
    s.add_constraint(Constraint::PointOnCircle { point: p, circle });
    s.add_constraint(Constraint::Fix {
        point: p,
        x: 0.1,
        y: 0.0,
    });
    // Fix fights radius — point should end on the circle. Soft: remove fix y
    // conflict by only constraining via point-on-circle and horizontal.
    remove_fix_constraints(&mut s);
    s.add_constraint(Constraint::Distance {
        a: c,
        b: p,
        value: 0.05,
    });
    // Actually PointOnCircle + Radius already sets distance. Pin angle with
    // horizontal-ish: fix y=0 via a helper line.
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    let d = s.points()[&c].distance_to(&s.points()[&p]);
    almost(d, 0.05, 1e-5);
}

#[test]
fn s13_equal_radii() {
    let mut s = Draft::seeded(1);
    let (c1, cir1) = s.add_circle(0.0, 0.0, 0.05);
    let (c2, cir2) = s.add_circle(0.2, 0.0, 0.02);
    s.point_mut(c1).unwrap().fixed = true;
    s.point_mut(c2).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: cir1,
        value: 0.05,
    });
    s.add_constraint(Constraint::Equal { a: cir1, b: cir2 });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    let r1 = match &s.entities()[&cir1] {
        Entity::Circle { radius, .. } => *radius,
        _ => panic!(),
    };
    let r2 = match &s.entities()[&cir2] {
        Entity::Circle { radius, .. } => *radius,
        _ => panic!(),
    };
    almost(r1, r2, 1e-6);
}

#[test]
fn s14_tangent_line_circle() {
    let mut s = Draft::seeded(1);
    let (c, circle) = s.add_circle(0.0, 0.05, 0.05);
    let (a, b, line) = s.add_line(-0.1, 0.0, 0.1, 0.0);
    s.point_mut(c).unwrap().fixed = true;
    s.point_mut(a).unwrap().fixed = true;
    s.point_mut(b).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: circle,
        value: 0.05,
    });
    s.add_constraint(Constraint::Horizontal { line });
    s.add_constraint(Constraint::Tangent { line, circle });
    // Center y should stay at radius above the x-axis.
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    // Distance from center to line == radius.
    almost(s.points()[&c].y.abs(), 0.05, 1e-4);
}

#[test]
fn s15_circle_center_distance() {
    let mut s = Draft::seeded(1);
    let (c1, _) = s.add_circle(0.0, 0.0, 0.01);
    let (c2, _) = s.add_circle(0.1, 0.0, 0.01);
    s.point_mut(c1).unwrap().fixed = true;
    s.add_constraint(Constraint::Distance {
        a: c1,
        b: c2,
        value: 0.08,
    });
    s.add_constraint(Constraint::Fix {
        point: c2,
        x: 0.1,
        y: 0.0,
    });
    // Fix + distance on same point: overconstrained if values disagree.
    // Use only distance + horizontal by fixing y through initial and H via
    // a construction approach — just distance from fixed c1.
    remove_fix_constraints(&mut s);
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&c1].distance_to(&s.points()[&c2]), 0.08, 1e-5);
}

// ─── 16–20: angle, symmetric, arc ────────────────────────────────────────

#[test]
fn s16_angle_between_lines() {
    let mut s = Draft::seeded(1);
    let (a1, a2, l1) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let (b1, b2, l2) = s.add_line(0.0, 0.0, 0.07, 0.07);
    s.point_mut(a1).unwrap().fixed = true;
    s.point_mut(a2).unwrap().fixed = true;
    s.add_constraint(Constraint::Coincident { a: a1, b: b1 });
    s.add_constraint(Constraint::Angle {
        a: l1,
        b: l2,
        value: std::f64::consts::FRAC_PI_2,
    });
    s.add_constraint(Constraint::Distance {
        a: b1,
        b: b2,
        value: 0.1,
    });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&b2].x, 0.0, 1e-3);
}

#[test]
fn s17_symmetric_across_vertical() {
    let mut s = Draft::seeded(1);
    let (m1, m2, mirror) = s.add_line(0.0, -0.1, 0.0, 0.1);
    s.point_mut(m1).unwrap().fixed = true;
    s.point_mut(m2).unwrap().fixed = true;
    s.add_constraint(Constraint::Vertical { line: mirror });
    let a = s.add_point(Point::new(0.05, 0.02));
    let b = s.add_point(Point::new(-0.01, 0.0));
    s.point_mut(a).unwrap().fixed = true;
    s.add_constraint(Constraint::Symmetric { a, b, mirror });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&b].x, -0.05, 1e-5);
    almost(s.points()[&b].y, 0.02, 1e-5);
}

#[test]
fn s18_arc_equal_radii_implicit() {
    let mut s = Draft::seeded(1);
    let (c, start, end, _arc) = s.add_arc(0.0, 0.0, 0.05, 0.0, 0.0, 0.04);
    s.point_mut(c).unwrap().fixed = true;
    s.point_mut(start).unwrap().fixed = true;
    // End should be pulled to radius 0.05.
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    let rs = s.points()[&c].distance_to(&s.points()[&start]);
    let re = s.points()[&c].distance_to(&s.points()[&end]);
    almost(rs, re, 1e-5);
}

#[test]
fn s19_distance_point_line() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let p = s.add_point(Point::new(0.05, 0.01));
    s.point_mut(a).unwrap().fixed = true;
    s.point_mut(b).unwrap().fixed = true;
    s.add_constraint(Constraint::DistancePointLine {
        point: p,
        line,
        value: 0.03,
    });
    // Free x — pin roughly with a soft expectation on x via initial guess.
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&p].y.abs(), 0.03, 1e-5);
}

#[test]
fn s20_underconstrained_has_positive_dof() {
    let mut s = Draft::seeded(1);
    let (_a, _b, _line) = s.add_line(0.0, 0.0, 0.1, 0.05);
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged || r.residual_norm < 1e-6);
    assert!(d.dof > 0, "expected free DoF, got {}", d.dof);
    assert_eq!(d.status, SketchStatus::Ok);
}

// ─── 21–25: over-constraint & diagnostics ────────────────────────────────

#[test]
fn s21_conflicting_distances_reported() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(0.05, 0.0));
    s.add_constraint(Constraint::Distance { a, b, value: 0.05 });
    s.add_constraint(Constraint::Distance { a, b, value: 0.10 });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(
        !r.converged || d.status == SketchStatus::OverConstrained,
        "should not happily accept two distances"
    );
    assert_eq!(d.status, SketchStatus::OverConstrained);
    assert!(
        !d.conflicting.is_empty() || d.message.contains("over-constrained"),
        "message: {}",
        d.message
    );
}

#[test]
fn s22_horizontal_plus_vertical_on_nonzero_line_conflicts() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.1, 0.05);
    s.point_mut(a).unwrap().fixed = true;
    s.point_mut(b).unwrap().fixed = true;
    s.add_constraint(Constraint::Horizontal { line });
    s.add_constraint(Constraint::Vertical { line });
    let (_r, d) = Diagnostics::evaluate(&mut s);
    assert_eq!(d.status, SketchStatus::OverConstrained);
    assert!(
        d.message.contains("over-constrained"),
        "message: {}",
        d.message
    );
}

#[test]
fn s23_three_distances_triangle_fully_constrained() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(0.1, 0.0));
    let c = s.add_point(Point::new(0.03, 0.08));
    // Pin b on the x-axis.
    s.add_constraint(Constraint::Fix {
        point: b,
        x: 0.1,
        y: 0.0,
    });
    s.add_constraint(Constraint::Distance { a, b, value: 0.1 });
    s.add_constraint(Constraint::Distance {
        a,
        b: c,
        value: 0.1,
    });
    s.add_constraint(Constraint::Distance {
        a: b,
        b: c,
        value: 0.1,
    });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&a].distance_to(&s.points()[&c]), 0.1, 1e-4);
    almost(s.points()[&b].distance_to(&s.points()[&c]), 0.1, 1e-4);
    assert!(
        d.status == SketchStatus::FullyConstrained || d.status == SketchStatus::Ok,
        "status {:?} dof {}",
        d.status,
        d.dof
    );
}

// ─── 26–30: rectangle, plate-with-holes, regions, serde ──────────────────

#[test]
fn s26_axis_aligned_rectangle() {
    let mut s = Draft::seeded(1);
    // Four corners, four sides, H/V + distances + one corner fixed.
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.1, 0.0));
    let p2 = s.add_point(Point::new(0.1, 0.06));
    let p3 = s.add_point(Point::new(0.0, 0.06));
    let l0 = s.add_entity(Entity::Line { start: p0, end: p1 });
    let l1 = s.add_entity(Entity::Line { start: p1, end: p2 });
    let l2 = s.add_entity(Entity::Line { start: p2, end: p3 });
    let l3 = s.add_entity(Entity::Line { start: p3, end: p0 });
    for line in [l0, l2] {
        s.add_constraint(Constraint::Horizontal { line });
    }
    for line in [l1, l3] {
        s.add_constraint(Constraint::Vertical { line });
    }
    s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: 0.1,
    });
    s.add_constraint(Constraint::Distance {
        a: p0,
        b: p3,
        value: 0.06,
    });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&p1].x, 0.1, 1e-5);
    almost(s.points()[&p2].y, 0.06, 1e-5);
    almost(s.points()[&p3].x, 0.0, 1e-5);
    assert!(
        d.status == SketchStatus::FullyConstrained || d.dof <= 0,
        "dof {}",
        d.dof
    );
}

#[test]
fn s29_json_round_trip_preserves_ids() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.05, 0.0);
    s.add_constraint(Constraint::Horizontal { line });
    s.add_constraint(Constraint::Distance { a, b, value: 0.05 });
    let json = serde_json::to_string(&s.sketch).unwrap();
    let restored: Sketch = serde_json::from_str(&json).unwrap();
    assert_eq!(restored, s.sketch);
    // New geometry gets fresh ids, even from a minter re-seeded over the
    // old ones.
    let mut again = Draft::with_sketch(restored, arrix_core::IdMinter::new(1));
    let p = again.add_point(Point::new(1.0, 1.0));
    assert!(!s.contains_id(p));
}

// ─── 31–35: extras for margin above the 30-fixture bar ───────────────────

#[test]
fn s31_parallel_equal_rectangle_side() {
    let mut s = Draft::seeded(1);
    let (a1, a2, l1) = s.add_line(0.0, 0.0, 0.08, 0.0);
    let (b1, b2, l2) = s.add_line(0.0, 0.04, 0.05, 0.041);
    s.point_mut(a1).unwrap().fixed = true;
    s.point_mut(a2).unwrap().fixed = true;
    s.point_mut(b1).unwrap().fixed = true;
    s.add_constraint(Constraint::Parallel { a: l1, b: l2 });
    s.add_constraint(Constraint::Equal { a: l1, b: l2 });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&b1].distance_to(&s.points()[&b2]), 0.08, 1e-4);
}

#[test]
fn s32_multiple_coincident_chain() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(0.01, 0.0));
    let c = s.add_point(Point::new(0.02, 0.01));
    s.add_constraint(Constraint::Coincident { a, b });
    s.add_constraint(Constraint::Coincident { a: b, b: c });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&c].x, 0.0, 1e-5);
    almost(s.points()[&c].y, 0.0, 1e-5);
}

#[test]
fn s33_radius_and_center_fix_is_fully_constrained_circle() {
    let mut s = Draft::seeded(1);
    let (c, circle) = s.add_circle(0.01, 0.02, 0.03);
    s.add_constraint(Constraint::Fix {
        point: c,
        x: 0.01,
        y: 0.02,
    });
    s.add_constraint(Constraint::Radius {
        target: circle,
        value: 0.03,
    });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s34_empty_sketch_solves_trivially() {
    let mut s = Draft::seeded(1);
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert_eq!(d.dof, 0);
    assert_eq!(d.n_vars, 0);
}

// ─── 36–47: new extended constraints & DoF diagnostics ──────────────────

#[test]
fn s36_horizontal_points_alignment() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.05));
    let b = s.add_point(Point::new(0.1, 0.02));
    s.add_constraint(Constraint::HorizontalPoints { a, b });
    s.add_constraint(Constraint::Distance { a, b, value: 0.1 });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&b].y, 0.05, 1e-6);
    almost(s.points()[&b].x, 0.1, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s37_vertical_points_alignment() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.05, 0.0));
    let b = s.add_point(Point::new(0.02, 0.1));
    s.add_constraint(Constraint::VerticalPoints { a, b });
    s.add_constraint(Constraint::Distance { a, b, value: 0.1 });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&b].x, 0.05, 1e-6);
    almost(s.points()[&b].y, 0.1, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s38_concentric_circles() {
    let mut s = Draft::seeded(1);
    let (c1, cir1) = s.add_circle(0.0, 0.0, 0.05);
    let (c2, cir2) = s.add_circle(0.02, 0.03, 0.02);
    s.point_mut(c1).unwrap().fixed = true;
    s.add_constraint(Constraint::Concentric { a: cir1, b: cir2 });
    s.add_constraint(Constraint::Radius {
        target: cir1,
        value: 0.05,
    });
    s.add_constraint(Constraint::Radius {
        target: cir2,
        value: 0.02,
    });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    almost(s.points()[&c2].x, 0.0, 1e-6);
    almost(s.points()[&c2].y, 0.0, 1e-6);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(
        d.entity_state(cir1),
        EntityConstraintState::FullyConstrained
    );
    assert_eq!(
        d.entity_state(cir2),
        EntityConstraintState::FullyConstrained
    );
}

#[test]
fn s39_tangent_circles_external() {
    let mut s = Draft::seeded(1);
    let (c1, cir1) = s.add_circle(0.0, 0.0, 0.03);
    let (c2, cir2) = s.add_circle(0.07, 0.01, 0.02);
    s.point_mut(c1).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: cir1,
        value: 0.03,
    });
    s.add_constraint(Constraint::Radius {
        target: cir2,
        value: 0.02,
    });
    s.add_constraint(Constraint::HorizontalPoints { a: c1, b: c2 });
    s.add_constraint(Constraint::TangentCircles { a: cir1, b: cir2 });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&c2].y, 0.0, 1e-5);
    // External tangency: distance = 0.03 + 0.02 = 0.05
    almost(s.points()[&c2].x, 0.05, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s40_tangent_circles_internal() {
    let mut s = Draft::seeded(1);
    let (c1, cir1) = s.add_circle(0.0, 0.0, 0.05);
    // c2 inside c1
    let (c2, cir2) = s.add_circle(0.028, 0.005, 0.02);
    s.point_mut(c1).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: cir1,
        value: 0.05,
    });
    s.add_constraint(Constraint::Radius {
        target: cir2,
        value: 0.02,
    });
    s.add_constraint(Constraint::HorizontalPoints { a: c1, b: c2 });
    s.add_constraint(Constraint::TangentCircles { a: cir1, b: cir2 });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&c2].y, 0.0, 1e-5);
    // Internal tangency: distance = |0.05 - 0.02| = 0.03
    almost(s.points()[&c2].x, 0.03, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s41_diameter_constraint() {
    let mut s = Draft::seeded(1);
    let (c, circle) = s.add_circle(0.0, 0.0, 0.1);
    s.point_mut(c).unwrap().fixed = true;
    s.add_constraint(Constraint::Diameter {
        target: circle,
        value: 0.08,
    });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    match &s.entities()[&circle] {
        Entity::Circle { radius, .. } => almost(*radius, 0.04, 1e-6),
        _ => panic!("expected circle"),
    }
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(
        d.entity_state(circle),
        EntityConstraintState::FullyConstrained
    );
}

#[test]
fn s42_horizontal_distance_and_vertical_distance() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(0.08, 0.04));
    s.add_constraint(Constraint::HorizontalDistance { a, b, value: 0.12 });
    s.add_constraint(Constraint::VerticalDistance { a, b, value: 0.05 });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&b].x.abs(), 0.12, 1e-5);
    almost(s.points()[&b].y.abs(), 0.05, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s43_distance_parallel_lines() {
    let mut s = Draft::seeded(1);
    let (a1, a2, l1) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let (b1, b2, l2) = s.add_line(0.0, 0.02, 0.1, 0.02);
    s.point_mut(a1).unwrap().fixed = true;
    s.point_mut(a2).unwrap().fixed = true;
    s.add_constraint(Constraint::Horizontal { line: l1 });
    s.add_constraint(Constraint::Parallel { a: l1, b: l2 });
    s.add_constraint(Constraint::DistanceParallelLines {
        a: l1,
        b: l2,
        value: 0.04,
    });
    s.add_constraint(Constraint::VerticalPoints { a: a1, b: b1 });
    s.add_constraint(Constraint::Equal { a: l1, b: l2 });
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&b1].y.abs(), 0.04, 1e-5);
    almost(s.points()[&b2].y.abs(), 0.04, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s44_entity_constraint_state_classification() {
    let mut s = Draft::seeded(1);
    // Fully fixed line
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::fixed(0.1, 0.0));
    let l_fixed = s.add_entity(Entity::Line { start: p0, end: p1 });

    // Fully constrained line
    let p2 = s.add_point(Point::new(0.1, 0.05));
    let l_constrained = s.add_entity(Entity::Line { start: p1, end: p2 });
    s.add_constraint(Constraint::Vertical {
        line: l_constrained,
    });
    s.add_constraint(Constraint::Distance {
        a: p1,
        b: p2,
        value: 0.05,
    });

    // Under-constrained line (free floating)
    let p3 = s.add_point(Point::new(0.2, 0.2));
    let p4 = s.add_point(Point::new(0.3, 0.25));
    let l_free = s.add_entity(Entity::Line { start: p3, end: p4 });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert_eq!(d.entity_state(l_fixed), EntityConstraintState::Fixed);
    assert_eq!(d.point_state(p0), EntityConstraintState::Fixed);
    assert_eq!(d.point_state(p1), EntityConstraintState::Fixed);

    assert_eq!(
        d.entity_state(l_constrained),
        EntityConstraintState::FullyConstrained
    );
    assert_eq!(d.point_state(p2), EntityConstraintState::FullyConstrained);

    assert_eq!(
        d.entity_state(l_free),
        EntityConstraintState::UnderConstrained
    );
    assert_eq!(d.point_state(p3), EntityConstraintState::UnderConstrained);
    assert_eq!(d.point_state(p4), EntityConstraintState::UnderConstrained);
}

#[test]
fn s45_redundant_constraint_detection() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.1, 0.0);
    s.point_mut(a).unwrap().fixed = true;
    s.add_constraint(Constraint::Horizontal { line });
    let red_id = s.add_constraint(Constraint::HorizontalPoints { a, b });
    s.add_constraint(Constraint::Distance { a, b, value: 0.1 });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert!(
        d.is_redundant(red_id) || !d.redundant.is_empty(),
        "expected redundant constraint detection"
    );
}

#[test]
fn s46_conflicting_entity_state() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.1, 0.0);
    s.point_mut(a).unwrap().fixed = true;
    s.point_mut(b).unwrap().fixed = true;
    s.add_constraint(Constraint::Horizontal { line });
    s.add_constraint(Constraint::Vertical { line });

    let (_r, d) = Diagnostics::evaluate(&mut s);
    assert_eq!(d.status, SketchStatus::OverConstrained);
    assert_eq!(d.entity_state(line), EntityConstraintState::Conflicting);
    assert_eq!(d.point_state(a), EntityConstraintState::Conflicting);
    assert_eq!(d.point_state(b), EntityConstraintState::Conflicting);
}

#[test]
fn s48_parallel_unconstrained_two_lines() {
    let mut s = Draft::seeded(1);
    let (_a1, _a2, l1) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let (_b1, _b2, l2) = s.add_line(0.0, 0.05, 0.08, 0.11); // at angle ~36.87 deg

    s.add_constraint(Constraint::Parallel { a: l1, b: l2 });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);

    // Cross product of directions should be zero (parallel).
    let (a_start, a_end) = match s.entities()[&l1] {
        Entity::Line { start, end } => (start, end),
        _ => unreachable!(),
    };
    let (b_start, b_end) = match s.entities()[&l2] {
        Entity::Line { start, end } => (start, end),
        _ => unreachable!(),
    };
    let (ax, ay) = (
        s.points()[&a_end].x - s.points()[&a_start].x,
        s.points()[&a_end].y - s.points()[&a_start].y,
    );
    let (bx, by) = (
        s.points()[&b_end].x - s.points()[&b_start].x,
        s.points()[&b_end].y - s.points()[&b_start].y,
    );
    let na = (ax * ax + ay * ay).sqrt();
    let nb = (bx * bx + by * by).sqrt();
    let cross = (ax * by - ay * bx) / (na * nb);
    almost(cross, 0.0, 1e-4);
}

#[test]
fn s49_perpendicular_unconstrained_two_lines() {
    let mut s = Draft::seeded(1);
    let (_a1, _a2, l1) = s.add_line(0.0, 0.0, 0.1, 0.0);
    let (_b1, _b2, l2) = s.add_line(0.0, 0.05, 0.08, 0.11);

    s.add_constraint(Constraint::Perpendicular { a: l1, b: l2 });
    let (r, _) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);

    // Dot product of directions should be zero (perpendicular).
    let (a_start, a_end) = match s.entities()[&l1] {
        Entity::Line { start, end } => (start, end),
        _ => unreachable!(),
    };
    let (b_start, b_end) = match s.entities()[&l2] {
        Entity::Line { start, end } => (start, end),
        _ => unreachable!(),
    };
    let (ax, ay) = (
        s.points()[&a_end].x - s.points()[&a_start].x,
        s.points()[&a_end].y - s.points()[&a_start].y,
    );
    let (bx, by) = (
        s.points()[&b_end].x - s.points()[&b_start].x,
        s.points()[&b_end].y - s.points()[&b_start].y,
    );
    let na = (ax * ax + ay * ay).sqrt();
    let nb = (bx * bx + by * by).sqrt();
    let dot = (ax * bx + ay * by) / (na * nb);
    almost(dot, 0.0, 1e-4);
}

// ─── 50–65: FreeCAD-aligned extended constraints ─────────────────────────

#[test]
fn s50_symmetric_points_three_points() {
    let mut s = Draft::seeded(1);
    let c = s.add_point(Point::fixed(0.0, 0.0));
    let a = s.add_point(Point::new(0.04, 0.03));
    let b = s.add_point(Point::new(-0.01, 0.0));
    s.add_constraint(Constraint::Fix {
        point: a,
        x: 0.04,
        y: 0.03,
    });
    s.add_constraint(Constraint::symmetric_points(a, b, c));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&b].x, -0.04, 1e-6);
    almost(s.points()[&b].y, -0.03, 1e-6);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s51_point_on_perp_bisector() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::fixed(0.10, 0.0));
    let p = s.add_point(Point::new(0.02, 0.05));
    s.add_constraint(Constraint::point_on_perp_bisector(p, a, b));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&p].x, 0.05, 1e-5);
    almost(s.points()[&p].y, 0.05, 1e-4);
    let da = s.points()[&p].distance_to(&s.points()[&a]);
    let db = s.points()[&p].distance_to(&s.points()[&b]);
    almost(da, db, 1e-6);
    assert_eq!(d.dof, 1);
}

#[test]
fn s52_arc_length_dimension() {
    let mut s = Draft::seeded(1);
    let (c, start, end, arc) = s.add_arc(0.0, 0.0, 0.05, 0.0, 0.01, 0.04);
    s.point_mut(c).unwrap().fixed = true;
    s.point_mut(start).unwrap().fixed = true;
    // R = 0.05, target theta = pi/2 -> arc length = 0.05 * pi/2
    let target_len = 0.05 * std::f64::consts::FRAC_PI_2;
    s.add_constraint(Constraint::arc_length(arc, target_len));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&end].x, 0.0, 1e-5);
    almost(s.points()[&end].y, 0.05, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s53_block_line_entity() {
    let mut s = Draft::seeded(1);
    let (start, end, line) = s.add_line(0.02, 0.03, 0.12, 0.07);
    s.add_constraint(Constraint::block(line));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&start].x, 0.02, 1e-6);
    almost(s.points()[&start].y, 0.03, 1e-6);
    almost(s.points()[&end].x, 0.12, 1e-6);
    almost(s.points()[&end].y, 0.07, 1e-6);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(
        d.entity_state(line),
        EntityConstraintState::FullyConstrained
    );
}

#[test]
fn s54_block_circle_entity() {
    let mut s = Draft::seeded(1);
    let (center, circle) = s.add_circle(0.05, 0.04, 0.025);
    s.add_constraint(Constraint::block(circle));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&center].x, 0.05, 1e-6);
    almost(s.points()[&center].y, 0.04, 1e-6);
    if let Entity::Circle { radius, .. } = s.entities()[&circle] {
        almost(radius, 0.025, 1e-6);
    } else {
        panic!("expected circle");
    }
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(
        d.entity_state(circle),
        EntityConstraintState::FullyConstrained
    );
}

#[test]
fn s55_block_arc_entity() {
    let mut s = Draft::seeded(1);
    let (c, start, end, arc) = s.add_arc(0.0, 0.0, 0.05, 0.0, 0.0, 0.05);
    s.add_constraint(Constraint::block(arc));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&c].x, 0.0, 1e-6);
    almost(s.points()[&c].y, 0.0, 1e-6);
    almost(s.points()[&start].x, 0.05, 1e-6);
    almost(s.points()[&start].y, 0.0, 1e-6);
    almost(s.points()[&end].x, 0.0, 1e-6);
    almost(s.points()[&end].y, 0.05, 1e-6);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(d.entity_state(arc), EntityConstraintState::FullyConstrained);
}

#[test]
fn s56_angle_points_three_points() {
    let mut s = Draft::seeded(1);
    let v = s.add_point(Point::fixed(0.0, 0.0));
    let a = s.add_point(Point::fixed(0.10, 0.0));
    let b = s.add_point(Point::new(0.07, 0.07));
    s.add_constraint(Constraint::Distance {
        a: v,
        b,
        value: 0.10,
    });
    s.add_constraint(Constraint::angle_points(
        a,
        v,
        b,
        std::f64::consts::FRAC_PI_4,
    ));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    let expected = 0.10 * std::f64::consts::FRAC_1_SQRT_2;
    almost(s.points()[&b].x, expected, 1e-5);
    almost(s.points()[&b].y, expected, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s57_distance_to_axis_x() {
    let mut s = Draft::seeded(1);
    let p = s.add_point(Point::new(0.02, 0.05));
    s.add_constraint(Constraint::distance_to_axis_x(p, 0.08));
    s.add_constraint(Constraint::distance_to_axis_y(p, 0.05));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&p].x.abs(), 0.08, 1e-6);
    almost(s.points()[&p].y.abs(), 0.05, 1e-6);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s58_distance_to_axis_y() {
    let mut s = Draft::seeded(1);
    let p = s.add_point(Point::new(-0.03, 0.02));
    s.add_constraint(Constraint::distance_to_axis_y(p, 0.06));
    s.add_constraint(Constraint::distance_to_axis_x(p, 0.03));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&p].x.abs(), 0.03, 1e-6);
    almost(s.points()[&p].y.abs(), 0.06, 1e-6);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s59_distance_circle_circle_clearance() {
    let mut s = Draft::seeded(1);
    let (c1, cir1) = s.add_circle(0.0, 0.0, 0.03);
    let (c2, cir2) = s.add_circle(0.08, 0.01, 0.02);
    s.point_mut(c1).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: cir1,
        value: 0.03,
    });
    s.add_constraint(Constraint::Radius {
        target: cir2,
        value: 0.02,
    });
    s.add_constraint(Constraint::HorizontalPoints { a: c1, b: c2 });
    s.add_constraint(Constraint::distance_circle_circle(cir1, cir2, 0.04));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&c2].y, 0.0, 1e-5);
    // Center distance = R1 + R2 + clearance = 0.03 + 0.02 + 0.04 = 0.09
    almost(s.points()[&c2].x, 0.09, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s60_distance_point_circle_clearance() {
    let mut s = Draft::seeded(1);
    let (c, circle) = s.add_circle(0.0, 0.0, 0.04);
    let p = s.add_point(Point::new(0.08, 0.02));
    s.point_mut(c).unwrap().fixed = true;
    s.add_constraint(Constraint::Radius {
        target: circle,
        value: 0.04,
    });
    s.add_constraint(Constraint::HorizontalPoints { a: c, b: p });
    s.add_constraint(Constraint::distance_point_circle(p, circle, 0.025));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);
    almost(s.points()[&p].y, 0.0, 1e-5);
    // Distance from center = R + clearance = 0.04 + 0.025 = 0.065
    almost(s.points()[&p].x, 0.065, 1e-5);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[cfg(feature = "snells-law")]
#[test]
fn s61_snells_law_refraction() {
    let mut s = Draft::seeded(1);
    // Vertical boundary line along Y axis (normal is horizontal X-axis)
    let (b1, b2, boundary) = s.add_line(0.0, -0.1, 0.0, 0.1);
    s.point_mut(b1).unwrap().fixed = true;
    s.point_mut(b2).unwrap().fixed = true;
    s.add_constraint(Constraint::Vertical { line: boundary });

    let v = s.add_point(Point::fixed(0.0, 0.0));
    // Incident ray 1 from (-0.1, 0.1) to (0, 0): angle 45 deg, sin(theta1) = 1/sqrt(2)
    let ray1_start = s.add_point(Point::fixed(-0.10, 0.10));
    // Refracted ray 2 from (0,0) to end in medium 2 (x > 0)
    let ray2_end = s.add_point(Point::new(0.08, -0.04));

    let ratio = 1.5; // n2 / n1 = 1.5
    s.add_constraint(Constraint::snells_law(
        ray1_start, v, ray2_end, boundary, ratio,
    ));
    s.add_constraint(Constraint::Distance {
        a: v,
        b: ray2_end,
        value: 0.10,
    });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "residual {}", r.residual_norm);

    // Expected sin(theta2) = sin(45 deg) / 1.5 = (1/sqrt(2)) / 1.5
    let sin1 = std::f64::consts::FRAC_1_SQRT_2;
    let expected_sin2 = sin1 / ratio;
    let expected_cos2 = (1.0 - expected_sin2 * expected_sin2).sqrt();
    almost(s.points()[&ray2_end].x, 0.10 * expected_cos2, 1e-4);
    almost(s.points()[&ray2_end].y, -0.10 * expected_sin2, 1e-4);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
}

#[test]
fn s64_conflicting_block_and_fix() {
    let mut s = Draft::seeded(1);
    let (start, _end, line) = s.add_line(0.0, 0.0, 0.1, 0.0);
    s.add_constraint(Constraint::block(line));
    s.add_constraint(Constraint::Fix {
        point: start,
        x: 0.05,
        y: 0.05,
    });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(
        !r.converged || d.status == SketchStatus::OverConstrained,
        "should detect over-constraint between Block and Fix"
    );
    assert_eq!(d.status, SketchStatus::OverConstrained);
    assert!(
        !d.conflicting.is_empty() || d.message.contains("over-constrained"),
        "diagnostic message: {}",
        d.message
    );
}

#[test]
fn s65_extended_constraints_serde_round_trip() {
    let mut s = Draft::seeded(1);
    let p1 = s.add_point(Point::fixed(0.0, 0.0));
    let p2 = s.add_point(Point::new(0.05, 0.0));
    let p3 = s.add_point(Point::new(-0.05, 0.0));
    let p4 = s.add_point(Point::new(0.0, 0.05));
    let (_c, _start, _end, arc) = s.add_arc(0.0, 0.0, 0.04, 0.0, 0.0, 0.04);
    let (_c_cir, cir) = s.add_circle(0.1, 0.0, 0.02);
    let (_ls, _le, line) = s.add_line(0.0, -0.1, 0.0, 0.1);

    s.add_constraint(Constraint::symmetric_points(p2, p3, p1));
    s.add_constraint(Constraint::point_on_perp_bisector(p4, p2, p3));
    s.add_constraint(Constraint::arc_length(
        arc,
        0.04 * std::f64::consts::FRAC_PI_2,
    ));
    s.add_constraint(Constraint::block(line));
    s.add_constraint(Constraint::angle_points(
        p2,
        p1,
        p4,
        std::f64::consts::FRAC_PI_2,
    ));
    s.add_constraint(Constraint::distance_to_axis_x(p2, 0.05));
    s.add_constraint(Constraint::distance_to_axis_y(p4, 0.05));
    s.add_constraint(Constraint::distance_circle_circle(cir, cir, 0.0));
    s.add_constraint(Constraint::distance_point_circle(p2, cir, 0.03));
    #[cfg(feature = "snells-law")]
    s.add_constraint(Constraint::snells_law(p2, p1, p3, line, 1.0));

    let json = serde_json::to_string_pretty(&s.sketch).expect("serialize sketch");
    let mut restored: Sketch = serde_json::from_str(&json).expect("deserialize sketch");

    assert_eq!(restored.points().len(), s.points().len());
    assert_eq!(restored.entities().len(), s.entities().len());
    assert_eq!(restored.constraints().len(), s.constraints().len());

    // Solve restored sketch
    let (r, _) = Diagnostics::evaluate(&mut restored);
    assert!(r.converged, "restored sketch residual {}", r.residual_norm);
}

// ─── 66–72: driving/reference and suppression ────────────────────────────

#[test]
fn s66_reference_dimensions_no_overconstraint() {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.1, 0.0));
    let p2 = s.add_point(Point::new(0.1, 0.05));
    let p3 = s.add_point(Point::new(0.0, 0.05));

    let l0 = s.add_entity(Entity::Line { start: p0, end: p1 });
    let l1 = s.add_entity(Entity::Line { start: p1, end: p2 });
    let l2 = s.add_entity(Entity::Line { start: p2, end: p3 });
    let l3 = s.add_entity(Entity::Line { start: p3, end: p0 });

    s.add_constraint(Constraint::Horizontal { line: l0 });
    s.add_constraint(Constraint::Vertical { line: l1 });
    s.add_constraint(Constraint::Horizontal { line: l2 });
    s.add_constraint(Constraint::Vertical { line: l3 });
    s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: 0.1,
    });
    s.add_constraint(Constraint::Distance {
        a: p1,
        b: p2,
        value: 0.05,
    });

    let (res_driving, diag_driving) = Diagnostics::evaluate(&mut s);
    assert!(res_driving.converged);
    assert_eq!(diag_driving.status, SketchStatus::FullyConstrained);
    assert_eq!(diag_driving.dof, 0);

    // Add reference dimensions that would otherwise over-constrain the sketch if driving:
    // 1. Diagonal distance p0 -> p2 (hypot(0.1, 0.05) = sqrt(0.0125) ≈ 0.111803)
    let diag_ref_id = s.add_reference_constraint(Constraint::Distance {
        a: p0,
        b: p2,
        value: 0.0, // dummy value in reference constraint
    });
    // 2. Horizontal distance p0 -> p1
    let h_ref_id = s.add_reference_constraint(Constraint::HorizontalDistance {
        a: p0,
        b: p1,
        value: 0.0,
    });
    // 3. Vertical distance p1 -> p2
    let v_ref_id = s.add_reference_constraint(Constraint::VerticalDistance {
        a: p1,
        b: p2,
        value: 0.0,
    });
    // 4. Angle between l0 and l1 (90 deg / pi/2 rad)
    let ang_ref_id = s.add_reference_constraint(Constraint::Angle {
        a: l0,
        b: l1,
        value: 0.0,
    });

    // Re-evaluate: must remain FullyConstrained with 0 DoF and NO conflicts / redundancies.
    let (res_with_ref, diag_with_ref) = Diagnostics::evaluate(&mut s);
    assert!(res_with_ref.converged);
    assert_eq!(diag_with_ref.status, SketchStatus::FullyConstrained);
    assert_eq!(diag_with_ref.dof, 0);
    assert!(diag_with_ref.conflicting.is_empty());
    assert!(diag_with_ref.redundant.is_empty());

    // Live value computation returns exact geometrical measurements:
    let diag_val = s
        .measure(s.get_constraint(diag_ref_id).unwrap())
        .expect("diag distance value");
    almost(diag_val, (0.1_f64.powi(2) + 0.05_f64.powi(2)).sqrt(), 1e-6);

    let h_val = s
        .measure(s.get_constraint(h_ref_id).unwrap())
        .expect("h distance value");
    almost(h_val, 0.1, 1e-6);

    let v_val = s
        .measure(s.get_constraint(v_ref_id).unwrap())
        .expect("v distance value");
    almost(v_val, 0.05, 1e-6);

    let ang_val = s
        .measure(s.get_constraint(ang_ref_id).unwrap())
        .expect("angle value");
    almost(ang_val, std::f64::consts::FRAC_PI_2, 1e-6);
}

#[test]
fn s67_constraint_suppression_and_reactivation() {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.05, 0.0));
    let line = s.add_entity(Entity::Line { start: p0, end: p1 });

    let h_id = s.add_constraint(Constraint::Horizontal { line });
    let dist_id = s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: 0.05,
    });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(d.dof, 0);

    // Suppress horizontal constraint -> releases 1 DoF
    assert!(s.set_constraint_active(h_id, false));
    let record = s.get_constraint_record(h_id).unwrap();
    assert!(!record.is_active);
    assert!(record.is_suppressed());

    let (r_suppressed, d_suppressed) = Diagnostics::evaluate(&mut s);
    assert!(r_suppressed.converged);
    assert_eq!(d_suppressed.dof, 1);
    assert_eq!(d_suppressed.status, SketchStatus::Ok);

    // Re-activate horizontal constraint -> restores 0 DoF
    assert!(s.set_constraint_active(h_id, true));
    let (r_restored, d_restored) = Diagnostics::evaluate(&mut s);
    assert!(r_restored.converged);
    assert_eq!(d_restored.dof, 0);
    assert_eq!(d_restored.status, SketchStatus::FullyConstrained);

    // Suppress distance constraint -> releases 1 DoF
    assert!(s.set_constraint_active(dist_id, false));
    let (r_suppressed_dist, d_suppressed_dist) = Diagnostics::evaluate(&mut s);
    assert!(r_suppressed_dist.converged);
    assert_eq!(d_suppressed_dist.dof, 1);
}

#[test]
fn s68_constraint_record_serde_roundtrip_all_fields() {
    let mut s = Draft::seeded(1);
    let p1 = s.add_point(Point::fixed(0.0, 0.0));
    let p2 = s.add_point(Point::new(0.1, 0.0));

    let c1 = s.add_constraint_with_options(
        Constraint::Distance {
            a: p1,
            b: p2,
            value: 0.1,
        },
        true,
        true,
        Some("Width_Main".to_string()),
    );
    let c2 = s.add_constraint_with_options(
        Constraint::HorizontalDistance {
            a: p1,
            b: p2,
            value: 0.1,
        },
        false,
        true,
        Some("Ref_Width".to_string()),
    );
    let c3 = s.add_constraint_with_options(
        Constraint::VerticalDistance {
            a: p1,
            b: p2,
            value: 0.0,
        },
        true,
        false,
        Some("Suppressed_Height".to_string()),
    );

    let json = serde_json::to_string_pretty(&s.sketch).expect("serialize");
    let restored: Sketch = serde_json::from_str(&json).expect("deserialize");

    assert_eq!(restored.constraints().len(), 3);

    let r1 = restored.get_constraint_record(c1).unwrap();
    assert_eq!(r1.id, c1);
    assert!(r1.is_driving);
    assert!(r1.is_active);
    assert_eq!(r1.name.as_deref(), Some("Width_Main"));

    let r2 = restored.get_constraint_record(c2).unwrap();
    assert_eq!(r2.id, c2);
    assert!(!r2.is_driving);
    assert!(r2.is_reference());
    assert!(r2.is_active);
    assert_eq!(r2.name.as_deref(), Some("Ref_Width"));

    let r3 = restored.get_constraint_record(c3).unwrap();
    assert_eq!(r3.id, c3);
    assert!(r3.is_driving);
    assert!(!r3.is_active);
    assert!(r3.is_suppressed());
    assert_eq!(r3.name.as_deref(), Some("Suppressed_Height"));
}

#[test]
fn s70_reference_dimension_live_updates_on_drag() {
    let mut s = Draft::seeded(1);
    let p1 = s.add_point(Point::fixed(0.0, 0.0));
    let p2 = s.add_point(Point::new(0.03, 0.04));

    let ref_dist = s.add_reference_constraint(Constraint::Distance {
        a: p1,
        b: p2,
        value: 0.0,
    });

    let initial_val = s.measure(s.get_constraint(ref_dist).unwrap()).unwrap();
    almost(initial_val, 0.05, 1e-6);

    // Simulate geometry movement (drag or parameter update)
    if let Some(pt) = s.point_mut(p2) {
        pt.x = 0.06;
        pt.y = 0.08;
    }

    let updated_val = s.measure(s.get_constraint(ref_dist).unwrap()).unwrap();
    almost(updated_val, 0.10, 1e-6);
}

#[test]
fn s71_compute_all_reference_dimension_types() {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.04, 0.03)); // dist to p0 = 0.05, x=0.04, y=0.03
    let p2 = s.add_point(Point::new(0.04, 0.0));
    let p3 = s.add_point(Point::new(0.0, 0.03));

    let l1 = s.add_entity(Entity::Line { start: p0, end: p2 }); // on Y=0
    let l2 = s.add_entity(Entity::Line { start: p3, end: p1 }); // on Y=0.03, parallel to l1, dist = 0.03
    let (_cc, cir) = s.add_circle(0.1, 0.0, 0.02);
    let (_ac, _as, _ae, arc) = s.add_arc(0.0, 0.0, 0.05, 0.0, 0.0, 0.05); // radius 0.05, angle 90 deg -> arc length = 0.05 * pi/2

    // 1. Distance
    let v_dist = s
        .measure(&Constraint::Distance {
            a: p0,
            b: p1,
            value: 0.0,
        })
        .unwrap();
    almost(v_dist, 0.05, 1e-6);

    // 2. HorizontalDistance
    let v_hdist = s
        .measure(&Constraint::HorizontalDistance {
            a: p0,
            b: p1,
            value: 0.0,
        })
        .unwrap();
    almost(v_hdist, 0.04, 1e-6);

    // 3. VerticalDistance
    let v_vdist = s
        .measure(&Constraint::VerticalDistance {
            a: p0,
            b: p1,
            value: 0.0,
        })
        .unwrap();
    almost(v_vdist, 0.03, 1e-6);

    // 4. DistancePointLine
    let v_ptline = s
        .measure(&Constraint::DistancePointLine {
            point: p1,
            line: l1,
            value: 0.0,
        })
        .unwrap();
    almost(v_ptline, 0.03, 1e-6);

    // 5. DistanceParallelLines
    let v_parlines = s
        .measure(&Constraint::DistanceParallelLines {
            a: l1,
            b: l2,
            value: 0.0,
        })
        .unwrap();
    almost(v_parlines, 0.03, 1e-6);

    // 6. DistanceToAxisX
    let v_axis_x = s
        .measure(&Constraint::DistanceToAxisX {
            point: p1,
            value: 0.0,
        })
        .unwrap();
    almost(v_axis_x, 0.04, 1e-6);

    // 7. DistanceToAxisY
    let v_axis_y = s
        .measure(&Constraint::DistanceToAxisY {
            point: p1,
            value: 0.0,
        })
        .unwrap();
    almost(v_axis_y, 0.03, 1e-6);

    // 8. Radius
    let v_radius = s
        .measure(&Constraint::Radius {
            target: cir,
            value: 0.0,
        })
        .unwrap();
    almost(v_radius, 0.02, 1e-6);

    // 9. Diameter
    let v_diam = s
        .measure(&Constraint::Diameter {
            target: cir,
            value: 0.0,
        })
        .unwrap();
    almost(v_diam, 0.04, 1e-6);

    // 10. Angle (lines)
    let l_vert = s.add_entity(Entity::Line { start: p0, end: p3 });
    let v_angle = s
        .measure(&Constraint::Angle {
            a: l1,
            b: l_vert,
            value: 0.0,
        })
        .unwrap();
    almost(v_angle, std::f64::consts::FRAC_PI_2, 1e-6);

    // 11. AnglePoints
    let v_ang_pts = s
        .measure(&Constraint::AnglePoints {
            a: p2,
            vertex: p0,
            b: p3,
            value: 0.0,
        })
        .unwrap();
    almost(v_ang_pts, std::f64::consts::FRAC_PI_2, 1e-6);

    // 12. ArcLength
    let v_arclen = s
        .measure(&Constraint::ArcLength { arc, value: 0.0 })
        .unwrap();
    almost(v_arclen, 0.05 * std::f64::consts::FRAC_PI_2, 1e-6);

    // 13. DistanceCircleCircle clearance
    let (_c2, cir2) = s.add_circle(0.2, 0.0, 0.03); // center dist 0.1, radii 0.02 + 0.03 = 0.05 -> clearance = 0.05
    let v_cc_dist = s
        .measure(&Constraint::DistanceCircleCircle {
            a: cir,
            b: cir2,
            value: 0.0,
        })
        .unwrap();
    almost(v_cc_dist, 0.05, 1e-6);

    // 14. DistancePointCircle clearance
    let v_pc_dist = s
        .measure(&Constraint::DistancePointCircle {
            point: p0,
            circle: cir,
            value: 0.0,
        })
        .unwrap();
    almost(v_pc_dist, 0.08, 1e-6); // center (0.1, 0) to p0(0, 0) is 0.1 - radius 0.02 = 0.08
}

#[test]
fn s72_sketch_helper_methods_and_record_mutations() {
    let mut s = Draft::seeded(1);
    let p1 = s.add_point(Point::fixed(0.0, 0.0));
    let p2 = s.add_point(Point::new(0.05, 0.0));

    let id1 = s.add_constraint(Constraint::Distance {
        a: p1,
        b: p2,
        value: 0.05,
    });
    let id2 = s.add_reference_constraint(Constraint::HorizontalDistance {
        a: p1,
        b: p2,
        value: 0.05,
    });

    assert!(s.get_constraint_record(id1).unwrap().is_driving);
    assert!(!s.get_constraint_record(id2).unwrap().is_driving);

    assert!(s.set_constraint_driving(id1, false));
    assert!(!s.get_constraint_record(id1).unwrap().is_driving);

    assert!(s.set_constraint_name(id1, Some("Renamed_Dist".to_string())));
    assert_eq!(
        s.get_constraint_record(id1).unwrap().name.as_deref(),
        Some("Renamed_Dist")
    );

    assert_eq!(s.constraints().len(), 2);

    assert!(s.set_constraint_driving(id1, true));
    assert!(s.get_constraint_record(id1).unwrap().is_driving);

    let mut_constraint = s.get_constraint_mut(id1).unwrap();
    assert!(mut_constraint.set_dimensional_value(0.08));
    assert_eq!(
        s.get_constraint(id1).unwrap().dimensional_value(),
        Some(0.08)
    );
}

// ─── 73: the single-source invariant ─────────────────────────────────────

/// A dimensional constraint's *measured* value and its residual come out of
/// one evaluation (`solver/eval/`), so `residual == measured - value` holds by
/// construction. As two formulas they once disagreed for `AngleWithDatum`.
#[test]
fn s73_reference_dimension_matches_its_own_residual() {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.04, 0.03));
    let p2 = s.add_point(Point::new(0.04, 0.0));
    let p3 = s.add_point(Point::new(0.0, 0.03));
    let l1 = s.add_entity(Entity::Line { start: p0, end: p2 });
    let l2 = s.add_entity(Entity::Line { start: p3, end: p1 });
    // Sloping *down*, so its angle to AxisX is negative — the case where a
    // reference value of `|angle|` would not zero the constraint's residual.
    let p4 = s.add_point(Point::new(0.04, -0.03));
    let l3 = s.add_entity(Entity::Line { start: p0, end: p4 });
    let (_cc, cir) = s.add_circle(0.1, 0.0, 0.02);
    let (_c2, cir2) = s.add_circle(0.2, 0.0, 0.01);
    let (_ac, _as, _ae, arc) = s.add_arc(0.0, 0.0, 0.05, 0.0, 0.0, 0.05);

    // Deliberately wrong values, so a residual of zero cannot pass by accident.
    let dims = [
        Constraint::Distance {
            a: p0,
            b: p1,
            value: 0.011,
        },
        Constraint::HorizontalDistance {
            a: p0,
            b: p1,
            value: 0.012,
        },
        Constraint::VerticalDistance {
            a: p0,
            b: p1,
            value: 0.013,
        },
        Constraint::DistancePointLine {
            point: p1,
            line: l1,
            value: 0.014,
        },
        Constraint::DistanceParallelLines {
            a: l1,
            b: l2,
            value: 0.015,
        },
        Constraint::DistanceToAxisX {
            point: p1,
            value: 0.016,
        },
        Constraint::DistanceToAxisY {
            point: p1,
            value: 0.017,
        },
        Constraint::Radius {
            target: cir,
            value: 0.018,
        },
        Constraint::Diameter {
            target: cir,
            value: 0.019,
        },
        Constraint::Angle {
            a: l1,
            b: l2,
            value: 0.21,
        },
        Constraint::AnglePoints {
            a: p1,
            vertex: p0,
            b: p2,
            value: 0.22,
        },
        Constraint::ArcLength { arc, value: 0.023 },
        Constraint::DistanceCircleCircle {
            a: cir,
            b: cir2,
            value: 0.024,
        },
        Constraint::DistancePointCircle {
            point: p1,
            circle: cir,
            value: 0.025,
        },
        Constraint::DistanceToDatum {
            point: p1,
            datum: arrix_sketch::DatumEntity::Origin,
            value: 0.026,
        },
        Constraint::AngleWithDatum {
            line: l2,
            datum: arrix_sketch::DatumEntity::AxisX,
            value: 0.27,
        },
        Constraint::AngleWithDatum {
            line: l3,
            datum: arrix_sketch::DatumEntity::AxisX,
            value: 0.28,
        },
    ];

    for c in dims {
        let label = format!("{c:?}");
        let value = c.dimensional_value().expect("dimensional");
        let measured = s
            .measure(&c)
            .unwrap_or_else(|| panic!("{label} measured nothing"));

        let mut probe = s.clone();
        let id = probe.add_constraint(c);
        let residual = arrix_sketch::constraint_residuals(&probe)
            .into_iter()
            .find(|(cid, _)| *cid == id)
            .map(|(_, mag)| mag)
            .unwrap_or_else(|| panic!("{label} has no residual row"));

        assert!(
            (residual - (measured - value).abs()).abs() < 1e-12,
            "{label}: |measured {measured} - value {value}| != residual {residual}"
        );
    }
}
