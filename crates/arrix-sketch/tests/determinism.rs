//! A solve is a pure function of the sketch: the evaluator re-solves from
//! stored positions (docs/DATA-MODEL.md §Sketches), so the same sketch
//! solved twice must land on the same bits, on every target.

use arrix_sketch::{Constraint, Diagnostics, Draft, SolverAlgorithm, SolverOptions};

/// A 40 × 30 mm plate drawn skewed, a 10 mm bore off centre, and a slot
/// arc tangent to the top edge: every kind of variable, and far enough
/// from the solution that the solver iterates.
fn skewed_plate() -> Draft {
    let mut s = Draft::seeded(11);
    let origin = s.add_point(arrix_sketch::Point::fixed(0.0, 0.0));
    let (p0, p1, bottom) = s.add_line(0.001, -0.002, 0.043, 0.001);
    let (_, p2, right) = s.add_line(0.043, 0.001, 0.041, 0.033);
    let (_, p3, top) = s.add_line(0.041, 0.033, -0.002, 0.029);
    let (_, _, left) = s.add_line(-0.002, 0.029, 0.001, -0.002);
    // Share the corners.
    let ends: Vec<_> = [bottom, right, top, left]
        .iter()
        .map(|l| s.entity(*l).unwrap().line_ends().unwrap())
        .collect();
    for i in 0..4 {
        let (_, end) = ends[i];
        let (start, _) = ends[(i + 1) % 4];
        s.add_constraint(Constraint::Coincident { a: end, b: start });
    }
    s.add_constraint(Constraint::Coincident { a: origin, b: p0 });
    s.add_constraint(Constraint::Horizontal { line: bottom });
    s.add_constraint(Constraint::Horizontal { line: top });
    s.add_constraint(Constraint::Vertical { line: right });
    s.add_constraint(Constraint::Vertical { line: left });
    s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: 0.04,
    });
    s.add_constraint(Constraint::Distance {
        a: p2,
        b: p3,
        value: 0.04,
    });
    s.add_constraint(Constraint::DistancePointLine {
        point: p3,
        line: bottom,
        value: 0.03,
    });
    let (c, bore) = s.add_circle(0.018, 0.017, 0.006);
    s.add_constraint(Constraint::Diameter {
        target: bore,
        value: 0.01,
    });
    s.add_constraint(Constraint::DistanceToAxisX {
        point: c,
        value: 0.02,
    });
    s.add_constraint(Constraint::DistanceToAxisY {
        point: c,
        value: 0.015,
    });
    let (_, _, _, arc) = s.add_arc(0.03, 0.02, 0.036, 0.021, 0.024, 0.021);
    s.add_constraint(Constraint::TangentCircles { a: arc, b: bore });
    s.add_constraint(Constraint::Tangent {
        line: top,
        circle: arc,
    });
    s
}

fn solved_bytes(mut s: Draft, algorithm: SolverAlgorithm) -> String {
    let options = SolverOptions {
        algorithm,
        ..SolverOptions::default()
    };
    let r = arrix_sketch::solve_with_options(&mut s, &options);
    assert!(r.converged, "{algorithm:?}: residual {}", r.residual_norm);
    assert!(r.iterations > 0, "the fixture must make the solver work");
    serde_json::to_string(&s.sketch).unwrap()
}

#[test]
fn the_same_sketch_solved_twice_gives_the_same_bits() {
    for algorithm in [SolverAlgorithm::DogLeg, SolverAlgorithm::LevenbergMarquardt] {
        let a = solved_bytes(skewed_plate(), algorithm);
        let b = solved_bytes(skewed_plate(), algorithm);
        assert_eq!(a, b, "{algorithm:?}");
    }
}

#[test]
fn a_solved_sketch_re_solves_in_place() {
    let mut s = skewed_plate();
    let (r, d) = Diagnostics::evaluate(&mut s);
    assert!(r.converged);
    assert!(d.conflicting.is_empty(), "{d:?}");
    let before = serde_json::to_string(&s.sketch).unwrap();
    let again = s.solve();
    assert!(again.converged);
    assert_eq!(again.iterations, 0, "a solved sketch is already converged");
    assert_eq!(serde_json::to_string(&s.sketch).unwrap(), before);
}
