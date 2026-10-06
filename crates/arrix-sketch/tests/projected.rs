//! Projected geometry in the solve: an entity with a source keeps the
//! place its source gives it. Its points and a circle's radius are not
//! solver variables, as a fixed point's are not, so a constraint to it
//! drives the drawn geometry and never moves it (docs/DATA-MODEL.md
//! §Sketches).

use arrix_sketch::{Constraint, Diagnostics, Draft, EntityConstraintState};

fn edge_name(feature: u64) -> arrix_core::Ref {
    arrix_core::Ref::Topo(format!("edge:sweep.{feature:013}.end-cap").parse().unwrap())
}

#[test]
fn a_constraint_to_a_projected_line_drives_the_drawn_one() {
    let mut d = Draft::seeded(11);
    let (a, b, projected) = d.add_line(0.0, 0.01, 0.04, 0.01);
    d.set_source(projected, Some(edge_name(3))).unwrap();
    let (c, _, drawn) = d.add_line(0.0, 0.0, 0.03, 0.002);
    d.add_constraint(Constraint::Parallel {
        a: projected,
        b: drawn,
    });
    d.add_constraint(Constraint::Coincident { a, b: c });

    let (result, diagnostics) = Diagnostics::evaluate(&mut d);
    assert!(result.converged);
    // The projected line stays where its source put it; the drawn one
    // turned parallel and moved its start onto it.
    assert_eq!(d.point(a).unwrap().pos(), [0.0, 0.01]);
    assert_eq!(d.point(b).unwrap().pos(), [0.04, 0.01]);
    let start = d.point(c).unwrap();
    assert!((start.x - 0.0).abs() < 1e-9 && (start.y - 0.01).abs() < 1e-9);
    assert_eq!(
        diagnostics.entity_state(projected),
        EntityConstraintState::Fixed
    );
    assert_eq!(
        diagnostics.entity_state(drawn),
        EntityConstraintState::UnderConstrained
    );
}

#[test]
fn a_projected_circle_keeps_its_radius_and_a_drawn_one_follows_it() {
    let mut d = Draft::seeded(12);
    let (_, projected) = d.add_circle(0.0, 0.0, 0.005);
    d.set_source(projected, Some(edge_name(4))).unwrap();
    let (_, drawn) = d.add_circle(0.02, 0.0, 0.008);
    d.add_constraint(Constraint::Equal {
        a: projected,
        b: drawn,
    });

    let (result, diagnostics) = Diagnostics::evaluate(&mut d);
    assert!(result.converged);
    let radius = |e| match d.entity(e) {
        Some(arrix_sketch::Entity::Circle { radius, .. }) => *radius,
        _ => unreachable!(),
    };
    assert_eq!(radius(projected), 0.005);
    assert!((radius(drawn) - 0.005).abs() < 1e-9);
    assert_eq!(
        diagnostics.entity_state(projected),
        EntityConstraintState::Fixed
    );
    // Taking the source away makes it drawn geometry again: free.
    d.set_source(projected, None).unwrap();
    let (_, diagnostics) = Diagnostics::evaluate(&mut d);
    assert_eq!(
        diagnostics.entity_state(projected),
        EntityConstraintState::UnderConstrained
    );
}
