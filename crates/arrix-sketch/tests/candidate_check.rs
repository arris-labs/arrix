//! `Sketch::check_candidate`: the verdict on a constraint *before* it
//! is added, read off the redundancy analysis a committed system gets. The
//! question these tests settle is whether that analysis is usable on a
//! candidate at all — it names what already implies a redundant one, tells
//! a contradiction from a redundancy, and does not cry wolf at a pose where
//! the linearisation has nothing to say.

use arrix_core::Id;
use arrix_sketch::Draft;
use arrix_sketch::{
    CandidateVerdict, Constraint, ConstraintId, Diagnostics, Entity, EntityId, Point, PointId,
    SketchStatus,
};

struct Rect {
    sketch: Draft,
    corners: [PointId; 4],
    sides: [EntityId; 4],
    width: ConstraintId,
    height: ConstraintId,
}

/// A 40 × 20 mm rectangle, one corner fixed at the origin, sides H/V,
/// width and height dimensioned: fully constrained.
fn dimensioned_rectangle() -> Rect {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(0.04, 0.0));
    let c = s.add_point(Point::new(0.04, 0.02));
    let d = s.add_point(Point::new(0.0, 0.02));
    let corners = [a, b, c, d];
    let mut sides = [EntityId::from(Id(0)); 4];
    for i in 0..4 {
        sides[i] = s.add_entity(Entity::Line {
            start: corners[i],
            end: corners[(i + 1) % 4],
        });
    }
    s.add_constraint(Constraint::Horizontal { line: sides[0] });
    s.add_constraint(Constraint::Vertical { line: sides[1] });
    s.add_constraint(Constraint::Horizontal { line: sides[2] });
    s.add_constraint(Constraint::Vertical { line: sides[3] });
    let width = s.add_constraint(Constraint::Distance { a, b, value: 0.04 });
    let height = s.add_constraint(Constraint::Distance {
        a: b,
        b: c,
        value: 0.02,
    });
    let (_, d) = Diagnostics::evaluate(&mut s);
    assert_eq!(d.status, SketchStatus::FullyConstrained, "{}", d.message);
    Rect {
        sketch: s,
        corners,
        sides,
        width,
        height,
    }
}

/// The diagonal of a fully dimensioned rectangle, at what it measures: a
/// redundant dimension, and the verdict names the width and the height it
/// follows from.
#[test]
fn a_measured_dimension_on_rigid_geometry_is_redundant_and_names_why() {
    let r = dimensioned_rectangle();
    let before = r.sketch.clone();
    let diagonal = Constraint::Distance {
        a: r.corners[0],
        b: r.corners[2],
        value: 0.04_f64.hypot(0.02),
    };
    let CandidateVerdict::Redundant(ids) = r.sketch.check_candidate(&diagonal) else {
        panic!("{:?}", r.sketch.check_candidate(&diagonal));
    };
    assert!(ids.contains(&r.width), "{ids:?}");
    assert!(ids.contains(&r.height), "{ids:?}");
    assert_eq!(r.sketch, before, "the check left the sketch alone");
}

/// The same diagonal at a length the rectangle cannot have: a conflict.
#[test]
fn a_wrong_dimension_on_rigid_geometry_conflicts() {
    let r = dimensioned_rectangle();
    let diagonal = Constraint::Distance {
        a: r.corners[0],
        b: r.corners[2],
        value: 0.06,
    };
    let CandidateVerdict::Conflicting(ids) = r.sketch.check_candidate(&diagonal) else {
        panic!("{:?}", r.sketch.check_candidate(&diagonal));
    };
    assert!(!ids.is_empty(), "a conflict with nothing named");
}

/// A relation the dimensions already imply, and one they rule out.
#[test]
fn a_geometric_candidate_gets_the_same_two_verdicts() {
    let r = dimensioned_rectangle();
    let parallel = Constraint::Parallel {
        a: r.sides[0],
        b: r.sides[2],
    };
    assert!(
        matches!(
            r.sketch.check_candidate(&parallel),
            CandidateVerdict::Redundant(_)
        ),
        "{:?}",
        r.sketch.check_candidate(&parallel)
    );
    let square = Constraint::Equal {
        a: r.sides[0],
        b: r.sides[1],
    };
    assert!(
        matches!(
            r.sketch.check_candidate(&square),
            CandidateVerdict::Conflicting(_)
        ),
        "{:?}",
        r.sketch.check_candidate(&square)
    );
}

/// What removes a degree of freedom is `Ok`, satisfied as drawn or not.
#[test]
fn a_candidate_on_free_geometry_is_ok() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 0.03, 0.01);
    s.solve();
    let measured = Constraint::Distance {
        a,
        b,
        value: 0.03_f64.hypot(0.01),
    };
    assert_eq!(s.check_candidate(&measured), CandidateVerdict::Ok);
    let typed = Constraint::Distance { a, b, value: 0.05 };
    assert_eq!(s.check_candidate(&typed), CandidateVerdict::Ok);
    assert_eq!(
        s.check_candidate(&Constraint::Horizontal { line }),
        CandidateVerdict::Ok
    );

    // A second copy of a constraint already there is the simplest
    // redundancy, and names its twin.
    let first = s.add_constraint(Constraint::Horizontal { line });
    s.solve();
    assert_eq!(
        s.check_candidate(&Constraint::Horizontal { line }),
        CandidateVerdict::Redundant(vec![first])
    );
}

/// Geometry that cannot move over-determines a candidate with no constraint
/// to name — beside free geometry too, which is where `System::partition`
/// sets such a constraint aside.
#[test]
fn a_candidate_between_immovable_points_has_nothing_to_name() {
    for with_free_geometry in [false, true] {
        let mut s = Draft::seeded(1);
        let a = s.add_point(Point::fixed(0.0, 0.0));
        let b = s.add_point(Point::fixed(0.05, 0.0));
        if with_free_geometry {
            s.add_line(0.1, 0.1, 0.2, 0.1);
        }
        s.solve();
        assert_eq!(
            s.check_candidate(&Constraint::Distance { a, b, value: 0.05 }),
            CandidateVerdict::Redundant(Vec::new()),
            "free geometry: {with_free_geometry}"
        );
        assert_eq!(
            s.check_candidate(&Constraint::Distance { a, b, value: 0.10 }),
            CandidateVerdict::Conflicting(Vec::new()),
            "free geometry: {with_free_geometry}"
        );
    }
}

/// A line and an arc sharing an endpoint: the tangency there is a
/// first-order row, so the analysis reads it like any
/// other. The most common tangency a user adds is `Ok` before it holds and
/// after, and a second copy of it is redundant with the first.
#[test]
fn a_tangency_at_a_shared_endpoint_is_read_like_any_other() {
    let mut s = Draft::seeded(1);
    let start = s.add_point(Point::fixed(0.0, 0.0));
    let joint = s.add_point(Point::new(0.03, 0.002));
    let line = s.add_entity(Entity::Line { start, end: joint });
    let center = s.add_point(Point::new(0.03, 0.01));
    let end = s.add_point(Point::new(0.04, 0.01));
    let arc = s.add_entity(Entity::Arc {
        center,
        start: joint,
        end,
    });
    s.solve();
    let (_, d) = Diagnostics::evaluate(&mut s);
    let before = d.dof;
    let tangent = Constraint::Tangent { line, circle: arc };
    assert_eq!(s.check_candidate(&tangent), CandidateVerdict::Ok);

    s.add_constraint(tangent.clone());
    let (_, d) = Diagnostics::evaluate(&mut s);
    assert_ne!(d.status, SketchStatus::OverConstrained, "{}", d.message);
    assert_eq!(d.dof, before - 1, "{}", d.message);
    // A second copy is redundant, naming its twin.
    let verdict = s.check_candidate(&tangent);
    assert!(
        matches!(verdict, CandidateVerdict::Redundant(ref ids) if !ids.is_empty()),
        "{verdict:?}"
    );

    // Tangent as drawn, with no constraint saying so: adding it is `Ok`.
    let mut s = Draft::seeded(1);
    let start = s.add_point(Point::fixed(0.0, 0.0));
    let joint = s.add_point(Point::new(0.03, 0.0));
    let line = s.add_entity(Entity::Line { start, end: joint });
    let center = s.add_point(Point::new(0.03, 0.01));
    let end = s.add_point(Point::new(0.04, 0.01));
    let arc = s.add_entity(Entity::Arc {
        center,
        start: joint,
        end,
    });
    s.solve();
    let (_, d) = Diagnostics::evaluate(&mut s);
    let before = d.dof;
    let tangent = Constraint::Tangent { line, circle: arc };
    assert_eq!(s.check_candidate(&tangent), CandidateVerdict::Ok);
    // Exactly tangent is where `|dist| − r` had no gradient: the row now
    // takes a DoF, and a second copy names the first.
    s.add_constraint(tangent.clone());
    let (_, d) = Diagnostics::evaluate(&mut s);
    assert_eq!(d.dof, before - 1, "{}", d.message);
    assert!(d.redundant.is_empty(), "{:?}", d.redundant);
    assert!(
        matches!(s.check_candidate(&tangent), CandidateVerdict::Redundant(ref ids) if !ids.is_empty()),
        "a second copy"
    );
}

/// A contradiction no linearisation at the starting pose shows: a point on
/// a line 20 mm from a fixed point, asked to be 10 mm from it. The row is
/// independent where the sketch stands and dependent where the solver
/// ends — which is why the unsatisfied branch solves before it judges.
#[test]
fn a_conflict_only_the_solve_reveals_is_found() {
    let mut s = Draft::seeded(1);
    let origin = s.add_point(Point::fixed(0.0, 0.0));
    let a = s.add_point(Point::fixed(-0.05, 0.02));
    let b = s.add_point(Point::fixed(0.05, 0.02));
    let line = s.add_entity(Entity::Line { start: a, end: b });
    let p = s.add_point(Point::new(0.03, 0.02));
    s.add_constraint(Constraint::PointOnLine { point: p, line });
    s.solve();
    let near = Constraint::Distance {
        a: origin,
        b: p,
        value: 0.01,
    };
    assert!(
        matches!(s.check_candidate(&near), CandidateVerdict::Conflicting(_)),
        "{:?}",
        s.check_candidate(&near)
    );
    let reachable = Constraint::Distance {
        a: origin,
        b: p,
        value: 0.04,
    };
    assert_eq!(s.check_candidate(&reachable), CandidateVerdict::Ok);
}
