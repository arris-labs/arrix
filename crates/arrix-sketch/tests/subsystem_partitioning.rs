//! Subsystem graph partitioning and Powell's DogLeg.

use arrix_sketch::{
    Constraint, ConstraintPriority, Draft, Entity, Point, Sketch, SolverAlgorithm, SolverOptions,
    System, solve, solve_with_options,
};

fn almost_eq(a: f64, b: f64, tol: f64, msg: &str) {
    assert!(
        (a - b).abs() < tol,
        "{msg}: expected {b}, got {a} (diff: {}, tol: {tol})",
        (a - b).abs()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Disjoint Subsystem Graph Partitioning
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_subsystem_partitioning_disjoint_rectangles() {
    let mut s = Draft::seeded(1);

    // Profile 1: Rectangle at (0.0, 0.0) -> (0.1, 0.05)
    let p1_p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1_p1 = s.add_point(Point::new(0.09, 0.0));
    let p1_p2 = s.add_point(Point::new(0.09, 0.04));
    let p1_p3 = s.add_point(Point::new(0.0, 0.04));
    let p1_l0 = s.add_entity(Entity::Line {
        start: p1_p0,
        end: p1_p1,
    });
    let p1_l1 = s.add_entity(Entity::Line {
        start: p1_p1,
        end: p1_p2,
    });
    let p1_l2 = s.add_entity(Entity::Line {
        start: p1_p2,
        end: p1_p3,
    });
    let p1_l3 = s.add_entity(Entity::Line {
        start: p1_p3,
        end: p1_p0,
    });
    s.add_constraint(Constraint::Horizontal { line: p1_l0 });
    s.add_constraint(Constraint::Vertical { line: p1_l1 });
    s.add_constraint(Constraint::Horizontal { line: p1_l2 });
    s.add_constraint(Constraint::Vertical { line: p1_l3 });
    s.add_constraint(Constraint::Distance {
        a: p1_p0,
        b: p1_p1,
        value: 0.1,
    });
    s.add_constraint(Constraint::Distance {
        a: p1_p1,
        b: p1_p2,
        value: 0.05,
    });

    // Profile 2: Disjoint Rectangle at (1.0, 1.0) -> (1.2, 1.3)
    let p2_p0 = s.add_point(Point::fixed(1.0, 1.0));
    let p2_p1 = s.add_point(Point::new(1.15, 1.0));
    let p2_p2 = s.add_point(Point::new(1.15, 1.25));
    let p2_p3 = s.add_point(Point::new(1.0, 1.25));
    let p2_l0 = s.add_entity(Entity::Line {
        start: p2_p0,
        end: p2_p1,
    });
    let p2_l1 = s.add_entity(Entity::Line {
        start: p2_p1,
        end: p2_p2,
    });
    let p2_l2 = s.add_entity(Entity::Line {
        start: p2_p2,
        end: p2_p3,
    });
    let p2_l3 = s.add_entity(Entity::Line {
        start: p2_p3,
        end: p2_p0,
    });
    s.add_constraint(Constraint::Horizontal { line: p2_l0 });
    s.add_constraint(Constraint::Vertical { line: p2_l1 });
    s.add_constraint(Constraint::Horizontal { line: p2_l2 });
    s.add_constraint(Constraint::Vertical { line: p2_l3 });
    s.add_constraint(Constraint::Distance {
        a: p2_p0,
        b: p2_p1,
        value: 0.2,
    });
    s.add_constraint(Constraint::Distance {
        a: p2_p1,
        b: p2_p2,
        value: 0.3,
    });

    // Profile 3: Disjoint Circle at (-0.5, 0.5) with Radius 0.15
    let p3_c = s.add_point(Point::fixed(-0.5, 0.5));
    let p3_circle = s.add_entity(Entity::Circle {
        center: p3_c,
        radius: 0.1,
    });
    s.add_constraint(Constraint::Radius {
        target: p3_circle,
        value: 0.15,
    });

    // Validate graph partitioning splits into 3 independent subsystems
    let sys = System::build(&s);
    let subsystems = sys.partition(&s);
    assert_eq!(
        subsystems.len(),
        3,
        "System must be partitioned into exactly 3 disjoint subsystems"
    );

    // Solve with DogLeg
    let res = solve(&mut s);
    assert!(res.converged);
    assert_eq!(res.dof, 0);

    // Verify Profile 1 geometry
    almost_eq(s.points()[&p1_p1].x, 0.1, 1e-7, "P1 P1.x");
    almost_eq(s.points()[&p1_p1].y, 0.0, 1e-7, "P1 P1.y");
    almost_eq(s.points()[&p1_p2].x, 0.1, 1e-7, "P1 P2.x");
    almost_eq(s.points()[&p1_p2].y, 0.05, 1e-7, "P1 P2.y");
    almost_eq(s.points()[&p1_p3].x, 0.0, 1e-7, "P1 P3.x");
    almost_eq(s.points()[&p1_p3].y, 0.05, 1e-7, "P1 P3.y");

    // Verify Profile 2 geometry
    almost_eq(s.points()[&p2_p1].x, 1.2, 1e-7, "P2 P1.x");
    almost_eq(s.points()[&p2_p1].y, 1.0, 1e-7, "P2 P1.y");
    almost_eq(s.points()[&p2_p2].x, 1.2, 1e-7, "P2 P2.x");
    almost_eq(s.points()[&p2_p2].y, 1.3, 1e-7, "P2 P2.y");
    almost_eq(s.points()[&p2_p3].x, 1.0, 1e-7, "P2 P3.x");
    almost_eq(s.points()[&p2_p3].y, 1.3, 1e-7, "P2 P3.y");

    // Verify Profile 3 geometry
    if let Entity::Circle { radius, .. } = s.entities()[&p3_circle] {
        almost_eq(radius, 0.15, 1e-7, "P3 circle radius");
    } else {
        panic!("Expected circle entity");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Powell's DogLeg vs Levenberg-Marquardt Numerical Equivalence
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_dogleg_and_lm_convergence_equivalence() {
    let make_sketch = || {
        let mut s = Draft::seeded(1);
        // Triangle with 3 non-linear distance constraints
        let p0 = s.add_point(Point::fixed(0.0, 0.0));
        let p1 = s.add_point(Point::new(0.04, 0.01));
        let p2 = s.add_point(Point::new(0.02, 0.05));
        let l0 = s.add_entity(Entity::Line { start: p0, end: p1 });
        let _l1 = s.add_entity(Entity::Line { start: p1, end: p2 });
        let _l2 = s.add_entity(Entity::Line { start: p2, end: p0 });
        s.add_constraint(Constraint::Distance {
            a: p0,
            b: p1,
            value: 0.05,
        });
        s.add_constraint(Constraint::Distance {
            a: p1,
            b: p2,
            value: 0.04,
        });
        s.add_constraint(Constraint::Distance {
            a: p2,
            b: p0,
            value: 0.03,
        });
        s.add_constraint(Constraint::Horizontal { line: l0 });
        (s, p1, p2)
    };

    let (mut s_dogleg, p1_dl, p2_dl) = make_sketch();
    let res_dl = solve_with_options(
        &mut s_dogleg,
        &SolverOptions {
            algorithm: SolverAlgorithm::DogLeg,
            ..Default::default()
        },
    );
    assert!(res_dl.converged);
    assert!(res_dl.residual_norm < 1e-9);

    let (mut s_lm, p1_lm, p2_lm) = make_sketch();
    let res_lm = solve_with_options(
        &mut s_lm,
        &SolverOptions {
            algorithm: SolverAlgorithm::LevenbergMarquardt,
            ..Default::default()
        },
    );
    assert!(res_lm.converged);
    assert!(res_lm.residual_norm < 1e-9);

    // Both algorithms must find the exact same geometric solution
    almost_eq(
        s_dogleg.points()[&p1_dl].x,
        s_lm.points()[&p1_lm].x,
        1e-7,
        "P1.x equivalence",
    );
    almost_eq(
        s_dogleg.points()[&p1_dl].y,
        s_lm.points()[&p1_lm].y,
        1e-7,
        "P1.y equivalence",
    );
    almost_eq(
        s_dogleg.points()[&p2_dl].x,
        s_lm.points()[&p2_lm].x,
        1e-7,
        "P2.x equivalence",
    );
    almost_eq(
        s_dogleg.points()[&p2_dl].y,
        s_lm.points()[&p2_lm].y,
        1e-7,
        "P2.y equivalence",
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Multi-Priority Dragging
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// 4. ConstraintPriority Serde Roundtrip
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_constraint_priority_serde_and_sketch_api() {
    let mut s = Draft::seeded(1);
    let p = s.add_point(Point::new(0.01, 0.02));
    let cid = s.add_constraint_with_full_options(
        Constraint::Fix {
            point: p,
            x: 0.01,
            y: 0.02,
        },
        true,
        true,
        None,
        ConstraintPriority::Drag,
    );

    assert_eq!(s.constraints()[&cid].priority, ConstraintPriority::Drag);
    assert!(s.constraints()[&cid].is_drag());

    s.set_constraint_priority(cid, ConstraintPriority::Primary);
    assert_eq!(s.constraints()[&cid].priority, ConstraintPriority::Primary);
    assert!(!s.constraints()[&cid].is_drag());

    // JSON round-trip
    let json = serde_json::to_string_pretty(&s.sketch).expect("JSON serialization failed");
    let s_deserialized: Sketch = serde_json::from_str(&json).expect("JSON deserialization failed");
    assert_eq!(
        s_deserialized.constraints()[&cid].priority,
        ConstraintPriority::Primary
    );
}
