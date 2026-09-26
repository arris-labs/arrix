//! Comprehensive FreeCAD Parity Validation Test Suite
//!
//! Validates numerical, behavioural and diagnostic parity between this
//! sketcher and FreeCAD's Sketcher / PlanarGCS.
//!
//! Scenarios covered:
//! 1. 3-point symmetry (central inversion) & perpendicular bisector alignment
//! 2. Blocked line & blocked arc with connected movable features
//! 3. Reference dimension live updates when dragging driving geometry
//! 4. Arc length dimensioning and solve convergence within 1e-9 tolerance
//! 5. Snell's refraction law convergence across multiple refractive index ratios
//! 6. Circle-to-circle and point-to-circle clearance solving with radii variations
//! 7. Progressive constraint suppression and re-activation cycles (DoF tracking)

use arrix_sketch::{
    Constraint, Diagnostics, Draft, Entity, EntityConstraintState, Point, SketchStatus,
};

fn almost_eq(a: f64, b: f64, tol: f64, msg: &str) {
    assert!(
        (a - b).abs() < tol,
        "{msg}: expected {b}, got {a} (diff: {}, tol: {tol})",
        (a - b).abs()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. 3-Point Symmetry & Perpendicular Bisector Alignment
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_freecad_parity_isosceles_triangle_symmetry_bisector_combination() {
    let mut s = Draft::seeded(1);

    // Base endpoints A and B, base midpoint M, apex P
    let a = s.add_point(Point::new(-0.05, 0.0));
    let b = s.add_point(Point::new(0.05, 0.0));
    let m = s.add_point(Point::fixed(0.0, 0.0));
    let p = s.add_point(Point::new(0.0, 0.08));

    let base_line = s.add_entity(Entity::Line { start: a, end: b });
    s.add_constraint(Constraint::Horizontal { line: base_line });
    // Midpoint symmetry: A and B are symmetric across M
    s.add_constraint(Constraint::symmetric_points(a, b, m));
    // Base width dimension = 0.10
    s.add_constraint(Constraint::Distance { a, b, value: 0.10 });
    // Apex on perpendicular bisector of AB
    s.add_constraint(Constraint::point_on_perp_bisector(p, a, b));
    // Apex height distance from midpoint M = 0.06
    s.add_constraint(Constraint::Distance {
        a: m,
        b: p,
        value: 0.06,
    });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert!(
        r.residual_norm < 1e-9,
        "Residual {} < 1e-9",
        r.residual_norm
    );
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(d.dof, 0);

    almost_eq(s.points()[&a].x, -0.05, 1e-6, "Apex A.x");
    almost_eq(s.points()[&b].x, 0.05, 1e-6, "Apex B.x");
    almost_eq(s.points()[&p].x, 0.0, 1e-6, "Apex P.x");
    almost_eq(s.points()[&p].y.abs(), 0.06, 1e-6, "Apex P.y");

    // Hypotenuse length parity: sqrt(0.05^2 + 0.06^2) = sqrt(0.0025 + 0.0036) = sqrt(0.0061) ≈ 0.078102496759
    let side_a = s.points()[&p].distance_to(&s.points()[&a]);
    let side_b = s.points()[&p].distance_to(&s.points()[&b]);
    let expected_hypot = (0.05_f64.powi(2) + 0.06_f64.powi(2)).sqrt();
    almost_eq(side_a, expected_hypot, 1e-6, "Side AP");
    almost_eq(side_b, expected_hypot, 1e-6, "Side BP");
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Blocked Line & Blocked Arc With Connected Movable Features
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_freecad_parity_blocked_line_with_connected_movable_features() {
    let mut s = Draft::seeded(1);

    // Blocked line L1 from (0.02, 0.03) to (0.12, 0.03)
    let (l1_s, l1_e, l1) = s.add_line(0.02, 0.03, 0.12, 0.03);
    s.add_constraint(Constraint::block(l1));

    // Movable line L2 starting at L1's end point l1_e and going to p2
    let p2 = s.add_point(Point::new(0.12, 0.09));
    let l2 = s.add_entity(Entity::Line {
        start: l1_e,
        end: p2,
    });

    // L2 perpendicular to L1 with length 0.05
    s.add_constraint(Constraint::Perpendicular { a: l1, b: l2 });
    s.add_constraint(Constraint::Distance {
        a: l1_e,
        b: p2,
        value: 0.05,
    });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert!(
        r.residual_norm < 1e-9,
        "Residual must converge < 1e-9: {}",
        r.residual_norm
    );

    // Verify L1 endpoints remained strictly locked
    almost_eq(s.points()[&l1_s].x, 0.02, 1e-8, "Blocked L1 start.x");
    almost_eq(s.points()[&l1_s].y, 0.03, 1e-8, "Blocked L1 start.y");
    almost_eq(s.points()[&l1_e].x, 0.12, 1e-8, "Blocked L1 end.x");
    almost_eq(s.points()[&l1_e].y, 0.03, 1e-8, "Blocked L1 end.y");

    // Verify L2 end point solved to (0.12, 0.08)
    almost_eq(s.points()[&p2].x, 0.12, 1e-6, "Movable point p2.x");
    almost_eq(s.points()[&p2].y, 0.08, 1e-6, "Movable point p2.y");

    assert_eq!(
        d.entity_state(l1),
        EntityConstraintState::FullyConstrained,
        "Blocked entity must report FullyConstrained"
    );
}

#[test]
fn test_freecad_parity_blocked_arc_with_connected_tangent_line() {
    let mut s = Draft::seeded(1);

    // Arc centered at (0, 0) with radius 0.05, from (0.05, 0) to (0.0, 0.05)
    let (arc_c, _arc_s, arc_e, arc) = s.add_arc(0.0, 0.0, 0.05, 0.0, 0.0, 0.05);
    s.add_constraint(Constraint::block(arc));

    // Movable line extending horizontally from arc end (0.0, 0.05)
    let p_line_end = s.add_point(Point::new(0.06, 0.05));
    let line = s.add_entity(Entity::Line {
        start: arc_e,
        end: p_line_end,
    });

    // Tangency constraint between line and arc
    s.add_constraint(Constraint::Tangent { line, circle: arc });
    s.add_constraint(Constraint::Distance {
        a: arc_e,
        b: p_line_end,
        value: 0.06,
    });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert!(
        r.residual_norm < 1e-9,
        "Residual < 1e-9: {}",
        r.residual_norm
    );

    // Blocked arc center unchanged
    almost_eq(s.points()[&arc_c].x, 0.0, 1e-8, "Blocked arc center.x");
    almost_eq(s.points()[&arc_c].y, 0.0, 1e-8, "Blocked arc center.y");

    // Tangent line end point is horizontal at y = 0.05 and x = -0.06 or 0.06
    almost_eq(s.points()[&p_line_end].y, 0.05, 1e-6, "Tangent line end.y");
    almost_eq(
        s.points()[&p_line_end].x.abs(),
        0.06,
        1e-6,
        "Tangent line end.x abs",
    );
    assert_eq!(d.entity_state(arc), EntityConstraintState::FullyConstrained);
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Reference Dimension Live Updates When Dragging Driving Geometry
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_freecad_parity_reference_dimension_live_updates_on_drag() {
    let mut s = Draft::seeded(1);

    // Base driving rectangle: Origin (0,0), Width 0.08, Height 0.04
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.08, 0.0));
    let p2 = s.add_point(Point::new(0.08, 0.04));
    let p3 = s.add_point(Point::new(0.0, 0.04));

    let l0 = s.add_entity(Entity::Line { start: p0, end: p1 });
    let l1 = s.add_entity(Entity::Line { start: p1, end: p2 });
    let l2 = s.add_entity(Entity::Line { start: p2, end: p3 });
    let l3 = s.add_entity(Entity::Line { start: p3, end: p0 });

    s.add_constraint(Constraint::Horizontal { line: l0 });
    s.add_constraint(Constraint::Vertical { line: l1 });
    s.add_constraint(Constraint::Horizontal { line: l2 });
    s.add_constraint(Constraint::Vertical { line: l3 });

    let driving_w = s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: 0.08,
    });
    let driving_h = s.add_constraint(Constraint::Distance {
        a: p1,
        b: p2,
        value: 0.04,
    });

    // Reference (driven) dimensions
    let ref_diag = s.add_reference_constraint(Constraint::Distance {
        a: p0,
        b: p2,
        value: 0.0,
    });
    let ref_hdist = s.add_reference_constraint(Constraint::HorizontalDistance {
        a: p0,
        b: p2,
        value: 0.0,
    });
    let ref_vdist = s.add_reference_constraint(Constraint::VerticalDistance {
        a: p0,
        b: p2,
        value: 0.0,
    });
    let ref_ang = s.add_reference_constraint(Constraint::AnglePoints {
        a: p1,
        vertex: p0,
        b: p2,
        value: 0.0,
    });
    let ref_axis_x = s.add_reference_constraint(Constraint::DistanceToAxisX {
        point: p2,
        value: 0.0,
    });
    let ref_axis_y = s.add_reference_constraint(Constraint::DistanceToAxisY {
        point: p2,
        value: 0.0,
    });

    // Solve driving sketch
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(d.dof, 0);

    // Check initial live reference values
    let diag_val = s.measure(s.get_constraint(ref_diag).unwrap()).unwrap();
    let expected_diag = (0.08_f64.powi(2) + 0.04_f64.powi(2)).sqrt();
    almost_eq(diag_val, expected_diag, 1e-6, "Initial ref diagonal");

    let hdist_val = s.measure(s.get_constraint(ref_hdist).unwrap()).unwrap();
    almost_eq(hdist_val, 0.08, 1e-6, "Initial ref H-distance");

    let vdist_val = s.measure(s.get_constraint(ref_vdist).unwrap()).unwrap();
    almost_eq(vdist_val, 0.04, 1e-6, "Initial ref V-distance");

    let ang_val = s.measure(s.get_constraint(ref_ang).unwrap()).unwrap();
    almost_eq(
        ang_val,
        (0.04_f64 / 0.08_f64).atan(),
        1e-6,
        "Initial ref angle",
    );

    let axis_x_val = s.measure(s.get_constraint(ref_axis_x).unwrap()).unwrap();
    almost_eq(axis_x_val, 0.08, 1e-6, "Initial ref axis X");

    let axis_y_val = s.measure(s.get_constraint(ref_axis_y).unwrap()).unwrap();
    almost_eq(axis_y_val, 0.04, 1e-6, "Initial ref axis Y");

    // Drag / Mutate driving dimensions: Width 0.08 -> 0.12, Height 0.04 -> 0.05
    s.get_constraint_mut(driving_w)
        .unwrap()
        .set_dimensional_value(0.12);
    s.get_constraint_mut(driving_h)
        .unwrap()
        .set_dimensional_value(0.05);

    let (r_updated, d_updated) = Diagnostics::evaluate(&mut s);
    assert!(r_updated.converged);
    assert_eq!(d_updated.status, SketchStatus::FullyConstrained);

    // Re-verify that reference dimensions immediately reflect updated geometry
    let updated_diag = s.measure(s.get_constraint(ref_diag).unwrap()).unwrap();
    let expected_new_diag = (0.12_f64.powi(2) + 0.05_f64.powi(2)).sqrt(); // = 0.13 exactly
    almost_eq(updated_diag, 0.13, 1e-6, "Updated ref diagonal");
    almost_eq(updated_diag, expected_new_diag, 1e-6, "Diagonal match");

    let updated_h = s.measure(s.get_constraint(ref_hdist).unwrap()).unwrap();
    almost_eq(updated_h, 0.12, 1e-6, "Updated ref H-distance");

    let updated_v = s.measure(s.get_constraint(ref_vdist).unwrap()).unwrap();
    almost_eq(updated_v, 0.05, 1e-6, "Updated ref V-distance");

    let updated_ang = s.measure(s.get_constraint(ref_ang).unwrap()).unwrap();
    almost_eq(
        updated_ang,
        (0.05_f64 / 0.12_f64).atan(),
        1e-6,
        "Updated ref angle",
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. Arc Length Dimensioning & Solve Convergence within 1e-9 Tolerance
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_freecad_parity_arc_length_dimension_high_precision_convergence() {
    let mut s = Draft::seeded(1);

    // Radius R = 0.05, Center at (0, 0), Start at (0.05, 0)
    let (c, start, end, arc) = s.add_arc(0.0, 0.0, 0.05, 0.0, 0.02, 0.04);
    s.point_mut(c).unwrap().fixed = true;
    s.point_mut(start).unwrap().fixed = true;

    // FreeCAD Parity: Arc length s = R * theta
    // For theta = 2*pi/3 (120 degrees), target length = 0.05 * (2*pi/3)
    let theta_target = 2.0 * std::f64::consts::PI / 3.0;
    let target_arc_len = 0.05 * theta_target;
    let arc_len_cid = s.add_constraint(Constraint::arc_length(arc, target_arc_len));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "Residual: {}", r.residual_norm);
    assert!(
        r.residual_norm < 1e-9,
        "Residual {} must be strictly < 1e-9",
        r.residual_norm
    );
    assert_eq!(d.status, SketchStatus::FullyConstrained);

    // Expected end coordinates: (R * cos(120°), R * sin(120°)) = (-0.025, 0.05 * sqrt(3)/2)
    let expected_x = 0.05 * theta_target.cos();
    let expected_y = 0.05 * theta_target.sin();
    almost_eq(s.points()[&end].x, expected_x, 1e-7, "Arc end.x at 120 deg");
    almost_eq(s.points()[&end].y, expected_y, 1e-7, "Arc end.y at 120 deg");

    // Dynamic update test: modify arc length to theta = 3*pi/4 (135 degrees)
    let new_theta = 3.0 * std::f64::consts::PI / 4.0;
    let new_target_len = 0.05 * new_theta;
    s.get_constraint_mut(arc_len_cid)
        .unwrap()
        .set_dimensional_value(new_target_len);

    let (r2, d2) = Diagnostics::evaluate(&mut s);
    assert!(r2.converged);
    assert!(
        r2.residual_norm < 1e-9,
        "Updated residual {} must be < 1e-9",
        r2.residual_norm
    );
    assert_eq!(d2.status, SketchStatus::FullyConstrained);

    let expected_new_x = 0.05 * new_theta.cos();
    let expected_new_y = 0.05 * new_theta.sin();
    almost_eq(
        s.points()[&end].x,
        expected_new_x,
        1e-7,
        "Arc end.x at 135 deg",
    );
    almost_eq(
        s.points()[&end].y,
        expected_new_y,
        1e-7,
        "Arc end.y at 135 deg",
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Snell's Refraction Law Convergence Across Various Refractive Indices
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "snells-law")]
#[test]
fn test_freecad_parity_snells_law_optical_refraction_ratios() {
    // Test optical refraction across multiple media interfaces:
    // 1. Air to Water (n2/n1 = 1.3333333333333333)
    // 2. Air to Crown Glass (n2/n1 = 1.5168)
    // 3. Air to Diamond (n2/n1 = 2.417)
    // 4. Glass to Air (dense to rare, n2/n1 = 1.0 / 1.5168)

    let test_cases = [
        ("Air to Water", 1.3333333333333333_f64, 45.0_f64),
        ("Air to Crown Glass", 1.5168_f64, 30.0_f64),
        ("Air to Diamond", 2.4170_f64, 60.0_f64),
        ("Glass to Air", 1.0 / 1.5168_f64, 25.0_f64),
    ];

    for (name, ratio, angle_deg) in test_cases {
        let mut s = Draft::seeded(1);

        // Vertical boundary line along Y-axis (interface normal is X-axis)
        let (b1, b2, boundary) = s.add_line(0.0, -0.2, 0.0, 0.2);
        s.point_mut(b1).unwrap().fixed = true;
        s.point_mut(b2).unwrap().fixed = true;
        s.add_constraint(Constraint::Vertical { line: boundary });

        // Interface junction vertex at (0, 0)
        let v = s.add_point(Point::fixed(0.0, 0.0));

        // Incident ray in medium 1 (x < 0)
        let theta1 = angle_deg.to_radians();
        let ray1_len = 0.10;
        let ray1_start = s.add_point(Point::fixed(
            -ray1_len * theta1.cos(),
            ray1_len * theta1.sin(),
        ));

        // Refracted ray end in medium 2 (x > 0)
        let sin2 = theta1.sin() / ratio;
        assert!(
            sin2.abs() <= 1.0,
            "Total internal reflection angle check for {name}"
        );
        let cos2 = (1.0 - sin2 * sin2).sqrt();

        // Initial guess with slight perturbation
        let ray2_end = s.add_point(Point::new(0.08, -0.04));

        s.add_constraint(Constraint::snells_law(
            ray1_start, v, ray2_end, boundary, ratio,
        ));
        s.add_constraint(Constraint::Distance {
            a: v,
            b: ray2_end,
            value: 0.10,
        });

        let (r, d) = Diagnostics::evaluate(&mut s);
        assert!(
            r.converged,
            "Refraction failed to converge for {name}: residual {}",
            r.residual_norm
        );
        assert!(
            r.residual_norm < 1e-9,
            "Residual for {name} must be < 1e-9: {}",
            r.residual_norm
        );
        assert_eq!(d.status, SketchStatus::FullyConstrained);

        let expected_x = 0.10 * cos2;
        let expected_y = -0.10 * sin2;
        almost_eq(
            s.points()[&ray2_end].x,
            expected_x,
            1e-5,
            &format!("{name} ray2.x"),
        );
        almost_eq(
            s.points()[&ray2_end].y,
            expected_y,
            1e-5,
            &format!("{name} ray2.y"),
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Circle-to-Circle and Point-to-Circle Clearance Solving
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_freecad_parity_circle_clearance_solving_with_radii_variation() {
    let test_radii_and_clearances = [
        // (R1, R2, clearance, expected_center_dist)
        (0.015, 0.025, 0.040, 0.080),
        (0.030, 0.020, 0.050, 0.100),
        (0.050, 0.040, 0.000, 0.090), // Touching / 0-clearance outer tangency
        (0.025, 0.035, 0.015, 0.075),
    ];

    for (r1, r2, clearance, expected_dist) in test_radii_and_clearances {
        let mut s = Draft::seeded(1);
        let (c1, cir1) = s.add_circle(0.0, 0.0, r1);
        let (c2, cir2) = s.add_circle(expected_dist * 0.9, 0.01, r2);

        s.point_mut(c1).unwrap().fixed = true;
        s.add_constraint(Constraint::Radius {
            target: cir1,
            value: r1,
        });
        s.add_constraint(Constraint::Radius {
            target: cir2,
            value: r2,
        });
        s.add_constraint(Constraint::HorizontalPoints { a: c1, b: c2 });
        s.add_constraint(Constraint::distance_circle_circle(cir1, cir2, clearance));

        let (r, d) = Diagnostics::evaluate(&mut s);
        assert!(r.converged, "Residual: {}", r.residual_norm);
        assert!(
            r.residual_norm < 1e-9,
            "Clearance residual must be < 1e-9: {}",
            r.residual_norm
        );
        assert_eq!(d.status, SketchStatus::FullyConstrained);

        almost_eq(
            s.points()[&c2].x,
            expected_dist,
            1e-7,
            &format!("Circle clearance center distance (R1={r1}, R2={r2}, clear={clearance})"),
        );
        almost_eq(s.points()[&c2].y, 0.0, 1e-7, "Circle c2.y alignment");
    }

    // Point-to-circle clearance solving
    let point_circle_tests = [
        // (Radius, clearance, expected_center_dist)
        (0.030, 0.020, 0.050),
        (0.045, 0.015, 0.060),
        (0.020, 0.035, 0.055),
    ];

    for (r_val, clearance, expected_dist) in point_circle_tests {
        let mut s = Draft::seeded(1);
        let (c, circle) = s.add_circle(0.0, 0.0, r_val);
        let p = s.add_point(Point::new(expected_dist * 0.85, 0.02));

        s.point_mut(c).unwrap().fixed = true;
        s.add_constraint(Constraint::Radius {
            target: circle,
            value: r_val,
        });
        s.add_constraint(Constraint::HorizontalPoints { a: c, b: p });
        s.add_constraint(Constraint::distance_point_circle(p, circle, clearance));

        let (r, d) = Diagnostics::evaluate(&mut s);
        assert!(r.converged);
        assert!(
            r.residual_norm < 1e-9,
            "Point-circle clearance residual < 1e-9: {}",
            r.residual_norm
        );
        assert_eq!(d.status, SketchStatus::FullyConstrained);

        almost_eq(
            s.points()[&p].x,
            expected_dist,
            1e-7,
            &format!("Point-circle distance (R={r_val}, clear={clearance})"),
        );
        almost_eq(s.points()[&p].y, 0.0, 1e-7, "Point p.y alignment");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 7. Progressive Suppression & Re-Activation Cycles (Degrees of Freedom)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_freecad_parity_3point_symmetry_central_inversion() {
    let mut s = Draft::seeded(1);

    // Center point C at non-origin position (0.04, -0.02)
    let c = s.add_point(Point::fixed(0.04, -0.02));
    // Point A at (0.01, 0.03)
    let a = s.add_point(Point::new(0.01, 0.03));
    // Point B placed far from symmetry point
    let b = s.add_point(Point::new(0.15, -0.10));

    s.add_constraint(Constraint::symmetric_points(a, b, c));
    let fix_a = s.add_constraint(Constraint::Fix {
        point: a,
        x: 0.01,
        y: 0.03,
    });

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged, "Solver residual: {}", r.residual_norm);
    assert!(
        r.residual_norm < 1e-9,
        "Residual must converge within 1e-9: {}",
        r.residual_norm
    );
    assert_eq!(d.status, SketchStatus::FullyConstrained);
    assert_eq!(d.dof, 0);

    // FreeCAD parity: B = 2 * C - A
    // B.x = 2 * 0.04 - 0.01 = 0.07
    // B.y = 2 * (-0.02) - 0.03 = -0.07
    almost_eq(s.points()[&b].x, 0.07, 1e-7, "Symmetric point B.x");
    almost_eq(s.points()[&b].y, -0.07, 1e-7, "Symmetric point B.y");

    // Remove fix constraint to allow dragging A freely
    s.remove_constraint(fix_a);

    // Test dragging A to (0.06, 0.05) -> B should automatically follow to (0.02, -0.09)
    let r_drag = s.solve_with_drag(a, 0.06, 0.05);
    assert!(r_drag.converged);
    almost_eq(s.points()[&a].x, 0.06, 1e-6, "Dragged A.x");
    almost_eq(s.points()[&a].y, 0.05, 1e-6, "Dragged A.y");
    almost_eq(s.points()[&b].x, 0.02, 1e-6, "Follower B.x after drag");
    almost_eq(s.points()[&b].y, -0.09, 1e-6, "Follower B.y after drag");

    // Test FreeCAD parity when A and B are fixed and C is free: C must solve to midpoint (A+B)/2
    let mut s2 = Draft::seeded(1);
    let a2 = s2.add_point(Point::fixed(-0.03, 0.04));
    let b2 = s2.add_point(Point::fixed(0.09, -0.02));
    let c2 = s2.add_point(Point::new(0.0, 0.0));
    s2.add_constraint(Constraint::symmetric_points(a2, b2, c2));

    let (r2, d2) = Diagnostics::evaluate(&mut s2);
    assert!(r2.converged);
    assert_eq!(d2.status, SketchStatus::FullyConstrained);
    almost_eq(
        s2.points()[&c2].x,
        (-0.03 + 0.09) / 2.0,
        1e-7,
        "Midpoint C2.x",
    );
    almost_eq(
        s2.points()[&c2].y,
        (0.04 - 0.02) / 2.0,
        1e-7,
        "Midpoint C2.y",
    );
}

#[test]
fn test_freecad_parity_perpendicular_bisector_orthogonal_alignment() {
    let mut s = Draft::seeded(1);

    // Segment endpoints A(0.02, 0.01) and B(0.08, 0.09)
    let a = s.add_point(Point::fixed(0.02, 0.01));
    let b = s.add_point(Point::fixed(0.08, 0.09));
    // Movable point P constrained to perpendicular bisector
    let p = s.add_point(Point::new(0.01, 0.08));

    s.add_constraint(Constraint::point_on_perp_bisector(p, a, b));

    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert_eq!(d.dof, 1, "Perpendicular bisector gives 1 DoF line");

    // Check equidistant property dist(P, A) == dist(P, B)
    let dist_a = s.points()[&p].distance_to(&s.points()[&a]);
    let dist_b = s.points()[&p].distance_to(&s.points()[&b]);
    almost_eq(dist_a, dist_b, 1e-7, "Distances to endpoints must match");

    // Check orthogonality: vector (P - Midpoint) must be orthogonal to (B - A)
    let mid_x = (0.02 + 0.08) / 2.0; // 0.05
    let mid_y = (0.01 + 0.09) / 2.0; // 0.05
    let ab_x = 0.08 - 0.02; // 0.06
    let ab_y = 0.09 - 0.01; // 0.08
    let pm_x = s.points()[&p].x - mid_x;
    let pm_y = s.points()[&p].y - mid_y;
    let dot_prod = pm_x * ab_x + pm_y * ab_y;
    almost_eq(
        dot_prod,
        0.0,
        1e-7,
        "Vector PM dot AB must be 0 (orthogonality)",
    );

    // FreeCAD drag along perpendicular bisector: drag P to x = 0.01 -> y should solve to 0.08
    let r_drag = s.solve_with_drag(p, 0.01, 0.08);
    assert!(r_drag.converged);
    almost_eq(s.points()[&p].x, 0.01, 1e-6, "Dragged P.x");
    almost_eq(s.points()[&p].y, 0.08, 1e-6, "Dragged P.y");
}

#[test]
fn test_freecad_parity_constraint_suppression_and_reactivation_cycles() {
    let mut s = Draft::seeded(1);

    // 4-point quadrilateral with full constraint set:
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.10, 0.0));
    let p2 = s.add_point(Point::new(0.10, 0.06));
    let p3 = s.add_point(Point::new(0.0, 0.06));

    let l0 = s.add_entity(Entity::Line { start: p0, end: p1 });
    let l1 = s.add_entity(Entity::Line { start: p1, end: p2 });
    let l2 = s.add_entity(Entity::Line { start: p2, end: p3 });
    let l3 = s.add_entity(Entity::Line { start: p3, end: p0 });

    let c_h0 = s.add_constraint(Constraint::Horizontal { line: l0 });
    let c_v1 = s.add_constraint(Constraint::Vertical { line: l1 });
    let c_h2 = s.add_constraint(Constraint::Horizontal { line: l2 });
    let c_v3 = s.add_constraint(Constraint::Vertical { line: l3 });
    let c_len0 = s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: 0.10,
    });
    let c_len1 = s.add_constraint(Constraint::Distance {
        a: p1,
        b: p2,
        value: 0.06,
    });

    // 1. Initial State: Fully Constrained (DoF = 0)
    let (r0, d0) = Diagnostics::evaluate(&mut s);
    assert!(r0.converged);
    assert_eq!(d0.status, SketchStatus::FullyConstrained);
    assert_eq!(d0.dof, 0);

    // 2. Suppress c_len1 (height distance) -> DoF becomes 1
    assert!(s.set_constraint_active(c_len1, false));
    let (r1, d1) = Diagnostics::evaluate(&mut s);
    assert!(r1.converged);
    assert_eq!(d1.status, SketchStatus::Ok);
    assert_eq!(d1.dof, 1);

    // 3. Suppress c_len0 (width distance) -> DoF becomes 2
    assert!(s.set_constraint_active(c_len0, false));
    let (r2, d2) = Diagnostics::evaluate(&mut s);
    assert!(r2.converged);
    assert_eq!(d2.status, SketchStatus::Ok);
    assert_eq!(d2.dof, 2);

    // 4. Suppress c_v1 (vertical line) -> DoF becomes 3
    assert!(s.set_constraint_active(c_v1, false));
    let (r3, d3) = Diagnostics::evaluate(&mut s);
    assert!(r3.converged);
    assert_eq!(d3.status, SketchStatus::Ok);
    assert_eq!(d3.dof, 3);

    // 5. Drag while unconstrained: deform geometry
    let r_drag = s.solve_with_drag(p2, 0.15, 0.09);
    assert!(r_drag.converged);

    // 6. Re-activate c_v1 -> DoF decreases from 3 to 2
    assert!(s.set_constraint_active(c_v1, true));
    let (r4, d4) = Diagnostics::evaluate(&mut s);
    assert!(r4.converged);
    assert_eq!(d4.dof, 2);

    // 7. Re-activate c_len0 -> DoF decreases from 2 to 1
    assert!(s.set_constraint_active(c_len0, true));
    let (r5, d5) = Diagnostics::evaluate(&mut s);
    assert!(r5.converged);
    assert_eq!(d5.dof, 1);

    // 8. Re-activate c_len1 -> DoF decreases from 1 to 0 (FullyConstrained restored)
    assert!(s.set_constraint_active(c_len1, true));
    let (r6, d6) = Diagnostics::evaluate(&mut s);
    assert!(r6.converged);
    assert!(
        r6.residual_norm < 1e-9,
        "Re-constrained residual {} < 1e-9",
        r6.residual_norm
    );
    assert_eq!(d6.status, SketchStatus::FullyConstrained);
    assert_eq!(d6.dof, 0);

    almost_eq(s.points()[&p1].x, 0.10, 1e-6, "Restored p1.x");
    almost_eq(s.points()[&p1].y, 0.0, 1e-6, "Restored p1.y");
    almost_eq(s.points()[&p2].x, 0.10, 1e-6, "Restored p2.x");
    almost_eq(s.points()[&p2].y, 0.06, 1e-6, "Restored p2.y");

    // 9. Add a conflicting constraint that is suppressed: verify NO over-constrained conflict is reported
    let c_conflict = s.add_constraint_with_options(
        Constraint::Distance {
            a: p0,
            b: p1,
            value: 0.999, // Contradicts 0.10
        },
        true,
        false, // Suppressed / Inactive
        Some("Suppressed_Conflict".to_string()),
    );

    let (r_suppressed_conflict, d_suppressed_conflict) = Diagnostics::evaluate(&mut s);
    assert!(r_suppressed_conflict.converged);
    assert_eq!(
        d_suppressed_conflict.status,
        SketchStatus::FullyConstrained,
        "Suppressed conflicting constraint must not trigger OverConstrained"
    );
    assert_eq!(d_suppressed_conflict.dof, 0);
    assert!(d_suppressed_conflict.conflicting.is_empty());

    // Clean up
    s.remove_constraint(c_conflict);
    s.remove_constraint(c_h0);
    s.remove_constraint(c_h2);
    s.remove_constraint(c_v3);
}
