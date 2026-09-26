//! `SketchStatus::OverConstrained` versus `SketchStatus::DidNotConverge`: both
//! leave residual, but one is a constraint set that cannot be met and the
//! other a solver that stopped short of one that can. The user fixes them
//! differently — delete a constraint, or move the geometry — so the
//! verdict says which.

use arrix_sketch::{
    Constraint, Diagnostics, Draft, Point, SketchStatus, SolverOptions, solve_with_options,
};

/// A free point 50 mm from a fixed one, asked to be 100 mm from it: one
/// equation on two unknowns, which the solver meets at once.
fn reachable_distance() -> Draft {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(0.05, 0.0));
    s.add_constraint(Constraint::Distance { a, b, value: 0.10 });
    s
}

/// Out of iterations before a satisfiable set is met: residual is left,
/// but the unsatisfied equation depends on nothing else — so no conflict is
/// named, and the verdict and message say the solver stopped short.
#[test]
fn a_solver_stopped_short_did_not_converge() {
    let mut s = reachable_distance();
    let options = SolverOptions {
        max_iterations: 0,
        ..SolverOptions::default()
    };
    let result = solve_with_options(&mut s, &options);
    assert!(!result.converged);
    let d = Diagnostics::analyze(&s, &result);
    assert_eq!(d.status, SketchStatus::DidNotConverge, "{}", d.message);
    assert!(d.conflicting.is_empty(), "{:?}", d.conflicting);
    assert!(d.message.contains("did not converge"), "{}", d.message);

    // Given its iterations, the same sketch solves.
    let mut s = reachable_distance();
    let (result, d) = Diagnostics::evaluate(&mut s);
    assert!(result.converged);
    assert_eq!(d.status, SketchStatus::Ok);
}

/// A triangle whose sides break the triangle inequality cannot be met from
/// anywhere: the solver ends collinear, where the three distances' rows
/// are dependent — a conflict, however many iterations it is given.
#[test]
fn an_impossible_triangle_is_over_constrained() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(0.01, 0.0));
    let c = s.add_point(Point::new(0.005, 0.01));
    s.add_constraint(Constraint::Distance { a, b, value: 0.01 });
    s.add_constraint(Constraint::Distance {
        a: b,
        b: c,
        value: 0.01,
    });
    s.add_constraint(Constraint::Distance {
        a,
        b: c,
        value: 0.03,
    });
    let (result, d) = Diagnostics::evaluate(&mut s);
    assert!(!result.converged);
    assert_eq!(d.status, SketchStatus::OverConstrained, "{}", d.message);
    assert!(d.message.contains("over-constrained"), "{}", d.message);
}

/// An unsatisfied constraint on geometry that cannot move at all conflicts
/// with whatever fixed it — there is nothing for the solver to reach. With
/// free geometry beside it too: `System::partition` then sets the
/// constraint aside in a subsystem with no variables, which carries it no
/// more than an empty system does.
#[test]
fn a_wrong_distance_between_fixed_points_is_over_constrained() {
    for with_free_geometry in [false, true] {
        let mut s = Draft::seeded(1);
        let a = s.add_point(Point::fixed(0.0, 0.0));
        let b = s.add_point(Point::fixed(0.05, 0.0));
        if with_free_geometry {
            s.add_line(0.1, 0.1, 0.2, 0.1);
        }
        s.add_constraint(Constraint::Distance { a, b, value: 0.10 });
        let (_, d) = Diagnostics::evaluate(&mut s);
        assert_eq!(
            d.status,
            SketchStatus::OverConstrained,
            "free geometry: {with_free_geometry}: {}",
            d.message
        );
    }
}
