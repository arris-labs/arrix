//! The transfer table and its corpus: one row per constraint kind a
//! document can hold, on a trimmed or split entity. Each row pins the
//! verdict — kept, retargeted, copied or dropped — then checks that a solve after the trim
//! moves nothing and that the sketch's DoF is no higher than the table
//! allows: what it had, plus the rows of what was dropped.

use std::f64::consts::{FRAC_PI_2, PI};

use arrix_sketch::Draft;
use arrix_sketch::modify::{self, Cut, TrimEdit, Verdict};
use arrix_sketch::{
    Constraint, ConstraintId, DatumEntity, Diagnostics, Entity, EntityId, Point, PointId, Sketch,
};

fn mm(x: f64, y: f64) -> [f64; 2] {
    [x * 1e-3, y * 1e-3]
}

fn free(s: &mut Draft, x: f64, y: f64) -> PointId {
    s.add_point(Point::new(x * 1e-3, y * 1e-3))
}

fn fix(s: &mut Draft, p: PointId) {
    let [x, y] = s.point(p).unwrap().pos();
    s.add_constraint(Constraint::Fix { point: p, x, y });
}

/// A line whose ends are pinned where they are drawn.
fn fixed_line(s: &mut Draft, a: [f64; 2], b: [f64; 2]) -> EntityId {
    let (p, q, l) = s.add_line(a[0] * 1e-3, a[1] * 1e-3, b[0] * 1e-3, b[1] * 1e-3);
    fix(s, p);
    fix(s, q);
    l
}

/// A circle pinned in place and size.
fn fixed_circle(s: &mut Draft, c: [f64; 2], r: f64) -> EntityId {
    let (center, circle) = s.add_circle(c[0] * 1e-3, c[1] * 1e-3, r * 1e-3);
    fix(s, center);
    s.add_constraint(Constraint::Radius {
        target: circle,
        value: r * 1e-3,
    });
    circle
}

/// The curve a row constrains, free but for what the row adds.
struct Target {
    id: EntityId,
    start: PointId,
    end: PointId,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fixture {
    /// `(-10, 0) → (10, 0)`, cut by pinned verticals at `x = ±3`.
    Line,
    /// The same, turned upright: `(0, -10) → (0, 10)`, cut at `y = ±3`.
    Upright,
    /// The upper half circle of radius 5 about the origin, CCW from `(5, 0)`
    /// to `(-5, 0)`, cut by pinned verticals at `x = ±2`.
    Arc,
    /// The circle of radius 5 about the origin, cut by the pinned x axis.
    Circle,
}

impl Fixture {
    fn build(self) -> (Draft, Target) {
        let mut s = Draft::seeded(1);
        let target = match self {
            Self::Line | Self::Upright => {
                let t = |x: f64, y: f64| if self == Self::Line { [x, y] } else { [y, x] };
                let (a, b) = (t(-10.0, 0.0), t(10.0, 0.0));
                let (start, end, id) =
                    s.add_line(a[0] * 1e-3, a[1] * 1e-3, b[0] * 1e-3, b[1] * 1e-3);
                for x in [-3.0, 3.0] {
                    fixed_line(&mut s, t(x, -5.0), t(x, 5.0));
                }
                Target { id, start, end }
            }
            Self::Arc => {
                let (_, start, end, id) = s.add_arc(0.0, 0.0, 0.005, 0.0, -0.005, 0.0);
                for x in [-2.0, 2.0] {
                    fixed_line(&mut s, [x, 1.0], [x, 10.0]);
                }
                Target { id, start, end }
            }
            Self::Circle => {
                let (center, id) = s.add_circle(0.0, 0.0, 0.005);
                fixed_line(&mut s, [-10.0, 0.0], [10.0, 0.0]);
                Target {
                    id,
                    start: center,
                    end: center,
                }
            }
        };
        (s, target)
    }

    /// Where to pick for each cut this fixture can make, in mm.
    fn pick(self, cut: Cut) -> [f64; 2] {
        match (self, cut) {
            (Self::Line, Cut::Shortened) => [8.0, 0.0],
            (Self::Line, Cut::Split) => [0.0, 0.0],
            (Self::Upright, Cut::Shortened) => [0.0, 8.0],
            (Self::Upright, Cut::Split) => [0.0, 0.0],
            // The first piece, from (5, 0) to the cut at x = 2.
            (Self::Arc, Cut::Shortened) => [4.5, 2.2],
            (Self::Arc, Cut::Split) => [0.0, 5.0],
            (Self::Circle, Cut::Opened) => [0.0, 5.0],
            other => panic!("no pick for {other:?}"),
        }
    }
}

struct Row {
    kind: K,
    fixture: Fixture,
    build: fn(&mut Draft, &Target) -> Constraint,
    expect: &'static [(Cut, Verdict)],
}

use Cut::{Opened, Shortened, Split};
use Verdict::{Copied, Dropped, Kept, Retargeted};

/// A constraint on the removed end goes with it; on a split both ends
/// survive.
const POINT: &[(Cut, Verdict)] = &[(Shortened, Dropped), (Split, Kept)];
/// A kind that measures the carrier.
const CARRIER: &[(Cut, Verdict)] = &[(Shortened, Kept), (Split, Kept)];
const CIRCLE: &[(Cut, Verdict)] = &[(Opened, Kept)];

macro_rules! row {
    ($kind:expr, $fixture:expr, $expect:expr, |$s:ident, $t:ident| $body:expr) => {
        Row {
            kind: $kind,
            fixture: $fixture,
            build: |$s: &mut Draft, $t: &Target| $body,
            expect: $expect,
        }
    };
}

fn corpus() -> Vec<Row> {
    use Fixture::{Arc, Circle, Line, Upright};
    vec![
        // Point kinds, on the line's `end` at (10, 0).
        row!(K::Coincident, Line, POINT, |s, t| Constraint::Coincident {
            a: t.end,
            b: free(s, 10.0, 0.0)
        }),
        row!(K::HorizontalPoints, Line, POINT, |_s, t| {
            Constraint::HorizontalPoints {
                a: t.start,
                b: t.end,
            }
        }),
        row!(K::VerticalPoints, Line, POINT, |s, t| {
            Constraint::VerticalPoints {
                a: t.end,
                b: free(s, 10.0, 4.0),
            }
        }),
        row!(K::Distance, Line, POINT, |_s, t| Constraint::Distance {
            a: t.start,
            b: t.end,
            value: 0.02
        }),
        row!(K::HorizontalDistance, Line, POINT, |_s, t| {
            Constraint::HorizontalDistance {
                a: t.start,
                b: t.end,
                value: 0.02,
            }
        }),
        row!(K::VerticalDistance, Line, POINT, |s, t| {
            Constraint::VerticalDistance {
                a: t.end,
                b: free(s, 10.0, 4.0),
                value: 0.004,
            }
        }),
        row!(K::Fix, Line, POINT, |_s, t| Constraint::Fix {
            point: t.end,
            x: 0.01,
            y: 0.0
        }),
        row!(K::SymmetricPoints, Line, POINT, |s, t| {
            Constraint::SymmetricPoints {
                a: t.start,
                b: t.end,
                center: free(s, 0.0, 0.0),
            }
        }),
        row!(K::PointOnPerpBisector, Line, POINT, |s, t| {
            Constraint::PointOnPerpBisector {
                point: free(s, 0.0, 4.0),
                a: t.start,
                b: t.end,
            }
        }),
        row!(K::AnglePoints, Line, POINT, |s, t| {
            Constraint::AnglePoints {
                a: t.start,
                vertex: t.end,
                b: free(s, 10.0, 4.0),
                value: FRAC_PI_2,
            }
        }),
        row!(K::DistanceToAxisX, Line, POINT, |_s, t| {
            Constraint::DistanceToAxisX {
                point: t.end,
                value: 0.01,
            }
        }),
        row!(K::DistanceToAxisY, Line, POINT, |_s, t| {
            Constraint::DistanceToAxisY {
                point: t.end,
                value: 0.0,
            }
        }),
        row!(K::PointOnDatum, Line, POINT, |_s, t| {
            Constraint::PointOnDatum {
                point: t.end,
                datum: DatumEntity::AxisX,
            }
        }),
        row!(K::DistanceToDatum, Line, POINT, |_s, t| {
            Constraint::DistanceToDatum {
                point: t.end,
                datum: DatumEntity::AxisY,
                value: 0.01,
            }
        }),
        row!(K::SymmetricAcrossDatum, Line, POINT, |_s, t| {
            Constraint::SymmetricAcrossDatum {
                a: t.start,
                b: t.end,
                datum: DatumEntity::AxisY,
            }
        }),
        row!(K::CoincidentToDatum, Line, POINT, |_s, t| {
            Constraint::CoincidentToDatum {
                point: t.end,
                datum: DatumEntity::AxisX,
            }
        }),
        // Line kinds.
        row!(K::Horizontal, Line, CARRIER, |_s, t| {
            Constraint::Horizontal { line: t.id }
        }),
        row!(K::Vertical, Upright, CARRIER, |_s, t| {
            Constraint::Vertical { line: t.id }
        }),
        row!(K::Parallel, Line, CARRIER, |s, t| Constraint::Parallel {
            a: t.id,
            b: fixed_line(s, [-10.0, 6.0], [10.0, 6.0])
        }),
        row!(K::Perpendicular, Line, CARRIER, |s, t| {
            Constraint::Perpendicular {
                a: t.id,
                b: fixed_line(s, [-10.0, 6.0], [-10.0, 12.0]),
            }
        }),
        row!(K::Angle, Line, CARRIER, |s, t| Constraint::Angle {
            a: t.id,
            b: fixed_line(s, [-10.0, 6.0], [-10.0, 12.0]),
            value: FRAC_PI_2
        }),
        row!(K::AngleWithDatum, Upright, CARRIER, |_s, t| {
            Constraint::AngleWithDatum {
                line: t.id,
                datum: DatumEntity::AxisX,
                value: FRAC_PI_2,
            }
        }),
        row!(K::PointOnLine, Line, CARRIER, |s, t| {
            Constraint::PointOnLine {
                point: free(s, -6.0, 0.0),
                line: t.id,
            }
        }),
        row!(K::DistancePointLine, Line, CARRIER, |s, t| {
            Constraint::DistancePointLine {
                point: free(s, 0.0, 4.0),
                line: t.id,
                value: 0.004,
            }
        }),
        row!(K::DistanceParallelLines, Line, CARRIER, |s, t| {
            Constraint::DistanceParallelLines {
                a: t.id,
                b: fixed_line(s, [-10.0, 6.0], [10.0, 6.0]),
                value: 0.006,
            }
        }),
        row!(K::Symmetric, Line, CARRIER, |s, t| Constraint::Symmetric {
            a: free(s, -6.0, 2.0),
            b: free(s, -6.0, -2.0),
            mirror: t.id
        }),
        // A line's tangency to a circle it does not share an end with: the
        // circle sits below, touching the line at x = -7.
        row!(K::Tangent, Line, CARRIER, |s, t| Constraint::Tangent {
            line: t.id,
            circle: fixed_circle(s, [-7.0, -3.0], 3.0)
        }),
        row!(
            K::Equal,
            Line,
            &[(Shortened, Dropped), (Split, Dropped)],
            |s, t| {
                Constraint::Equal {
                    a: t.id,
                    b: fixed_line(s, [-10.0, 6.0], [10.0, 6.0]),
                }
            }
        ),
        row!(
            K::Midpoint,
            Line,
            &[(Shortened, Dropped), (Split, Retargeted)],
            |s, t| {
                Constraint::Midpoint {
                    point: free(s, 0.0, 0.0),
                    line: t.id,
                }
            }
        ),
        row!(
            K::Block,
            Line,
            &[(Shortened, Kept), (Split, Copied)],
            |_s, t| { Constraint::Block { entity: t.id } }
        ),
        // Arc and circle kinds.
        row!(K::Radius, Arc, CARRIER, |_s, t| Constraint::Radius {
            target: t.id,
            value: 0.005
        }),
        row!(K::Radius, Circle, CIRCLE, |_s, t| Constraint::Radius {
            target: t.id,
            value: 0.005
        }),
        row!(K::Diameter, Arc, CARRIER, |_s, t| Constraint::Diameter {
            target: t.id,
            value: 0.01
        }),
        row!(K::Diameter, Circle, CIRCLE, |_s, t| Constraint::Diameter {
            target: t.id,
            value: 0.01
        }),
        row!(K::Concentric, Arc, CARRIER, |s, t| Constraint::Concentric {
            a: t.id,
            b: fixed_circle(s, [0.0, 0.0], 8.0)
        }),
        row!(K::Concentric, Circle, CIRCLE, |s, t| {
            Constraint::Concentric {
                a: t.id,
                b: fixed_circle(s, [0.0, 0.0], 8.0),
            }
        }),
        row!(K::TangentCircles, Arc, CARRIER, |s, t| {
            Constraint::TangentCircles {
                a: t.id,
                b: fixed_circle(s, [0.0, -8.0], 3.0),
            }
        }),
        row!(K::Equal, Arc, CARRIER, |s, t| Constraint::Equal {
            a: t.id,
            b: fixed_circle(s, [20.0, 0.0], 5.0)
        }),
        row!(K::Equal, Circle, CIRCLE, |s, t| Constraint::Equal {
            a: t.id,
            b: fixed_circle(s, [20.0, 0.0], 5.0)
        }),
        row!(K::PointOnCircle, Arc, CARRIER, |s, t| {
            Constraint::PointOnCircle {
                point: free(s, -3.0, -4.0),
                circle: t.id,
            }
        }),
        row!(K::DistanceCircleCircle, Arc, CARRIER, |s, t| {
            Constraint::DistanceCircleCircle {
                a: t.id,
                b: fixed_circle(s, [20.0, 0.0], 5.0),
                value: 0.01,
            }
        }),
        row!(K::DistancePointCircle, Arc, CARRIER, |s, t| {
            Constraint::DistancePointCircle {
                point: free(s, 0.0, -8.0),
                circle: t.id,
                value: 0.003,
            }
        }),
        row!(
            K::ArcLength,
            Arc,
            &[(Shortened, Dropped), (Split, Dropped)],
            |_s, t| {
                Constraint::ArcLength {
                    arc: t.id,
                    value: 0.005 * PI,
                }
            }
        ),
        // An arc's tangency to a line below it, touching the carrier at
        // (0, -5), off the arc.
        row!(K::Tangent, Arc, CARRIER, |s, t| Constraint::Tangent {
            line: fixed_line(s, [-10.0, -5.0], [10.0, -5.0]),
            circle: t.id
        }),
        row!(
            K::Block,
            Arc,
            &[(Shortened, Kept), (Split, Copied)],
            |_s, t| { Constraint::Block { entity: t.id } }
        ),
        row!(K::Block, Circle, CIRCLE, |_s, t| Constraint::Block {
            entity: t.id
        }),
    ]
}

/// The kinds a document can hold: every kind a default build has, the
/// `conics` and `snells-law` ones gated off (docs/DATA-MODEL.md §Sketches).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum K {
    Coincident,
    Horizontal,
    Vertical,
    HorizontalPoints,
    VerticalPoints,
    Parallel,
    Perpendicular,
    Equal,
    Concentric,
    Tangent,
    TangentCircles,
    Symmetric,
    Distance,
    HorizontalDistance,
    VerticalDistance,
    DistancePointLine,
    DistanceParallelLines,
    Angle,
    Radius,
    Diameter,
    PointOnLine,
    PointOnCircle,
    Midpoint,
    Fix,
    SymmetricPoints,
    PointOnPerpBisector,
    ArcLength,
    Block,
    AnglePoints,
    DistanceToAxisX,
    DistanceToAxisY,
    DistanceCircleCircle,
    DistancePointCircle,
    PointOnDatum,
    DistanceToDatum,
    AngleWithDatum,
    SymmetricAcrossDatum,
    CoincidentToDatum,
}

/// A constraint's kind. The match has no wildcard on a default build, so a
/// new kind fails to compile here until it has a row.
fn kind_of(c: &Constraint) -> K {
    match c {
        Constraint::Coincident { .. } => K::Coincident,
        Constraint::Horizontal { .. } => K::Horizontal,
        Constraint::Vertical { .. } => K::Vertical,
        Constraint::HorizontalPoints { .. } => K::HorizontalPoints,
        Constraint::VerticalPoints { .. } => K::VerticalPoints,
        Constraint::Parallel { .. } => K::Parallel,
        Constraint::Perpendicular { .. } => K::Perpendicular,
        Constraint::Equal { .. } => K::Equal,
        Constraint::Concentric { .. } => K::Concentric,
        Constraint::Tangent { .. } => K::Tangent,
        Constraint::TangentCircles { .. } => K::TangentCircles,
        Constraint::Symmetric { .. } => K::Symmetric,
        Constraint::Distance { .. } => K::Distance,
        Constraint::HorizontalDistance { .. } => K::HorizontalDistance,
        Constraint::VerticalDistance { .. } => K::VerticalDistance,
        Constraint::DistancePointLine { .. } => K::DistancePointLine,
        Constraint::DistanceParallelLines { .. } => K::DistanceParallelLines,
        Constraint::Angle { .. } => K::Angle,
        Constraint::Radius { .. } => K::Radius,
        Constraint::Diameter { .. } => K::Diameter,
        Constraint::PointOnLine { .. } => K::PointOnLine,
        Constraint::PointOnCircle { .. } => K::PointOnCircle,
        Constraint::Midpoint { .. } => K::Midpoint,
        Constraint::Fix { .. } => K::Fix,
        Constraint::SymmetricPoints { .. } => K::SymmetricPoints,
        Constraint::PointOnPerpBisector { .. } => K::PointOnPerpBisector,
        Constraint::ArcLength { .. } => K::ArcLength,
        Constraint::Block { .. } => K::Block,
        Constraint::AnglePoints { .. } => K::AnglePoints,
        Constraint::DistanceToAxisX { .. } => K::DistanceToAxisX,
        Constraint::DistanceToAxisY { .. } => K::DistanceToAxisY,
        Constraint::DistanceCircleCircle { .. } => K::DistanceCircleCircle,
        Constraint::DistancePointCircle { .. } => K::DistancePointCircle,
        Constraint::PointOnDatum { .. } => K::PointOnDatum,
        Constraint::DistanceToDatum { .. } => K::DistanceToDatum,
        Constraint::AngleWithDatum { .. } => K::AngleWithDatum,
        Constraint::SymmetricAcrossDatum { .. } => K::SymmetricAcrossDatum,
        Constraint::CoincidentToDatum { .. } => K::CoincidentToDatum,
        #[cfg(any(feature = "conics", feature = "snells-law"))]
        _ => unreachable!("a gated kind has no row"),
    }
}

const KIND_COUNT: usize = 38;

#[test]
fn the_corpus_has_a_row_for_every_kind() {
    let mut kinds: Vec<K> = corpus().iter().map(|r| r.kind).collect();
    kinds.sort();
    kinds.dedup();
    assert_eq!(kinds.len(), KIND_COUNT, "{kinds:?}");
}

fn dof(s: &mut Draft, what: &str) -> i32 {
    let (r, d) = Diagnostics::evaluate(s);
    assert!(r.converged, "{what}: {}", d.message);
    d.dof
}

/// Solves `s` and asserts that no point moved.
fn assert_solve_moves_nothing(s: &mut Draft, what: &str) -> i32 {
    let before = s.clone();
    let dof = dof(s, what);
    for (id, p) in before.points() {
        let q = s.point(*id).unwrap().pos();
        let d = (p.pos()[0] - q[0]).hypot(p.pos()[1] - q[1]);
        assert!(d <= 1e-9, "{what}: {id} moved {d}");
    }
    dof
}

/// What the trim did to the row's constraint `cid`, read off the sketch and
/// the edit.
fn observed(s: &Sketch, edit: &TrimEdit, cid: ConstraintId, was: &Constraint) -> Verdict {
    match s.constraints().get(&cid) {
        None => {
            assert!(edit.dropped.contains(&cid), "{cid} vanished unreported");
            Dropped
        }
        Some(r) if r.constraint != *was => {
            assert!(edit.retargeted.contains(&cid), "{cid} rewritten unreported");
            Retargeted
        }
        Some(_) if !edit.copied.is_empty() => {
            let far = edit.split_off.expect("a copy goes onto a far piece");
            for copy in &edit.copied {
                let c = &s.constraints()[copy].constraint;
                assert_eq!(kind_of(c), kind_of(was));
                assert!(c.uses_entity(far), "{c:?} is not on the far piece");
            }
            Copied
        }
        Some(_) => Kept,
    }
}

#[test]
fn every_row_carries_as_the_table_says() {
    for row in corpus() {
        for &(cut, expect) in row.expect {
            let what = format!("{:?} on {:?}, {cut:?}", row.kind, row.fixture);
            let (mut s, t) = row.fixture.build();
            let c = (row.build)(&mut s, &t);
            assert_eq!(kind_of(&c), row.kind, "{what}: the row builds its kind");
            let cid = s.add_constraint(c.clone());
            let before = assert_solve_moves_nothing(&mut s, &format!("{what}, before"));

            let line = row.fixture == Fixture::Line || row.fixture == Fixture::Upright;
            if c.uses_entity(t.id) {
                assert_eq!(modify::verdict(&c, cut, line), expect, "{what}: table");
            }
            let at = row.fixture.pick(cut);
            let edit = modify::trim(&mut s, t.id, mm(at[0], at[1])).unwrap();
            let cut_seen = match (edit.split_off, s.entity(t.id)) {
                (Some(_), _) => Split,
                (None, Some(Entity::Arc { .. })) if row.fixture == Fixture::Circle => Opened,
                _ => Shortened,
            };
            assert_eq!(cut_seen, cut, "{what}: the fixture cut");
            assert_eq!(observed(&s, &edit, cid, &c), expect, "{what}");

            let allowance: usize = if expect == Dropped {
                c.residual_count()
            } else {
                0
            };
            let after = assert_solve_moves_nothing(&mut s, &format!("{what}, after"));
            assert!(
                after <= before + allowance as i32,
                "{what}: dof {before} → {after}, allowance {allowance}"
            );
        }
    }
}

#[test]
fn a_split_line_is_joined_into_one_carrier() {
    let (mut s, t) = Fixture::Line.build();
    s.add_constraint(Constraint::Horizontal { line: t.id });
    let before = dof(&mut s, "before");
    let edit = modify::trim(&mut s, t.id, mm(0.0, 0.0)).unwrap();
    let far = edit.split_off.unwrap();
    assert_eq!(edit.joined.len(), 2);
    for cid in &edit.joined {
        assert!(matches!(
            s.constraints()[cid].constraint,
            Constraint::PointOnLine { line, .. } if line == t.id
        ));
    }
    assert_eq!(dof(&mut s, "after"), before, "the join restores the DoF");
    // The far piece is horizontal because it is on the kept piece's line,
    // not because the constraint was copied.
    assert!(
        !s.constraints()
            .values()
            .any(|r| r.constraint.uses_entity(far)
                && matches!(r.constraint, Constraint::Horizontal { .. }))
    );
}

#[test]
fn a_split_arc_is_joined_onto_its_circle() {
    let (mut s, t) = Fixture::Arc.build();
    s.add_constraint(Constraint::Radius {
        target: t.id,
        value: 0.005,
    });
    let before = dof(&mut s, "before");
    let edit = modify::trim(&mut s, t.id, mm(0.0, 5.0)).unwrap();
    assert_eq!(edit.joined.len(), 1);
    assert!(matches!(
        s.constraints()[&edit.joined[0]].constraint,
        Constraint::PointOnCircle { circle, .. } if circle == t.id
    ));
    assert_eq!(dof(&mut s, "after"), before);
}

/// A midpoint over a split line keeps measuring between the original ends.
#[test]
fn a_midpoint_over_a_split_line_becomes_symmetric_points() {
    let (mut s, t) = Fixture::Line.build();
    let m = free(&mut s, 0.0, 0.0);
    let cid = s.add_constraint(Constraint::Midpoint {
        point: m,
        line: t.id,
    });
    modify::trim(&mut s, t.id, mm(0.0, 0.0)).unwrap();
    assert_eq!(
        s.constraints()[&cid].constraint,
        Constraint::SymmetricPoints {
            a: t.start,
            b: t.end,
            center: m
        }
    );
}

/// A `Tangent` at a shared end follows the piece that holds the joint, so
/// it stays first order and the DoF does not rise.
#[test]
fn a_tangent_at_a_joint_follows_the_far_piece() {
    // A line whose end is an arc's start, the arc turning up from it.
    let (mut s, t) = Fixture::Line.build();
    let c = free(&mut s, 10.0, 5.0);
    fix(&mut s, c);
    let e = free(&mut s, 15.0, 5.0);
    let arc = s.add_entity(Entity::Arc {
        center: c,
        start: t.end,
        end: e,
    });
    let cid = s.add_constraint(Constraint::Tangent {
        line: t.id,
        circle: arc,
    });
    let before = dof(&mut s, "line, before");
    let edit = modify::trim(&mut s, t.id, mm(0.0, 0.0)).unwrap();
    let far = edit.split_off.unwrap();
    assert_eq!(edit.retargeted, vec![cid]);
    assert_eq!(
        s.constraints()[&cid].constraint,
        Constraint::Tangent {
            line: far,
            circle: arc
        }
    );
    assert!(dof(&mut s, "line, after") <= before);

    // The arc split instead: a line leaves its end (-5, 0) straight down.
    let (mut s, t) = Fixture::Arc.build();
    let foot = free(&mut s, -5.0, -10.0);
    fix(&mut s, foot);
    let leg = s.add_entity(Entity::Line {
        start: t.end,
        end: foot,
    });
    let cid = s.add_constraint(Constraint::Tangent {
        line: leg,
        circle: t.id,
    });
    let before = dof(&mut s, "arc, before");
    let edit = modify::trim(&mut s, t.id, mm(0.0, 5.0)).unwrap();
    let far = edit.split_off.unwrap();
    assert_eq!(
        s.constraints()[&cid].constraint,
        Constraint::Tangent {
            line: leg,
            circle: far
        }
    );
    assert!(dof(&mut s, "arc, after") <= before);
}

/// A curve with no crossing goes whole, and every constraint on it or its
/// points is reported dropped.
#[test]
fn a_deleted_curve_reports_everything_it_took() {
    let mut s = Draft::seeded(1);
    let (a, b, l) = s.add_line(0.0, 0.0, 0.01, 0.0);
    let mut on = [
        s.add_constraint(Constraint::Horizontal { line: l }),
        s.add_constraint(Constraint::Distance { a, b, value: 0.01 }),
        s.add_constraint(Constraint::Fix {
            point: a,
            x: 0.0,
            y: 0.0,
        }),
    ];
    let edit = modify::trim(&mut s, l, mm(5.0, 0.0)).unwrap();
    assert!(edit.deleted);
    let mut dropped = edit.dropped.clone();
    dropped.sort();
    on.sort();
    assert_eq!(dropped, on.to_vec());
    assert!(s.constraints().is_empty());
    assert_eq!(
        modify::verdict(&Constraint::Horizontal { line: l }, Cut::Deleted, true),
        Dropped
    );
}
