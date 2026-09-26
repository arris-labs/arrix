//! The drag policy (docs/UI-RENDERING.md §Sketch mode). Each `wanted_*`
//! test pins what a drag must do where an earlier weighted drag, which
//! weighed the pin against the constraints, did otherwise.
//!
//! The sketches are at millimetre scale, one frame each. The one exception
//! is the `Angle` side, also held on a metre-long arm: the weighted drag
//! flipped it only from 800 mm up, because a radian outweighed a
//! millimetre a thousand to one.
//!
//! `DragSession` follows the constraints instead of weighing the pin
//! against them, so a frame can only reach geometry connected to where the
//! drag started. That retired the `Angle`, `Tangent` and `TangentCircles`
//! flips: each needed a jump across geometry that satisfies nothing. The
//! arc sweep does not (its end slides round the circle and through the
//! start), and neither do an arm walked through its fixed end or a circle
//! shrunk through its tangent line over many frames: those are the branch
//! guard's (`solver/guard.rs`).

use std::f64::consts::{PI, TAU};

use arrix_sketch::{
    Constraint, Draft, DragSession, Entity, EntityId, Point, PointId, Sketch, SolveResult,
};

const MM: f64 = 1e-3;
/// A driving dimension "holds" within a micrometre.
const HOLDS: f64 = 1e-6;

fn xy(s: &Sketch, p: PointId) -> (f64, f64) {
    let p = s.point(p).expect("the point exists");
    (p.x, p.y)
}

fn dist(s: &Sketch, a: PointId, b: PointId) -> f64 {
    let (a, b) = (xy(s, a), xy(s, b));
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn fix(s: &mut Draft, p: PointId) {
    s.point_mut(p).expect("the point exists").fixed = true;
}

fn positions(s: &Sketch) -> Vec<(f64, f64)> {
    s.points().values().map(|p| (p.x, p.y)).collect()
}

// (a) An unreachable target and a driving `Distance`.

/// A rod: `a` fixed at the origin, `b` 50 mm along +x, the length driven.
/// The drag target is 120 mm out along the rod — 70 mm past its reach.
fn rod() -> (Draft, PointId, PointId) {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(50.0 * MM, 0.0));
    s.add_constraint(Constraint::Distance {
        a,
        b,
        value: 50.0 * MM,
    });
    (s, a, b)
}

fn drag_rod_out_of_reach() -> (Draft, PointId, PointId, SolveResult) {
    let (mut s, a, b) = rod();
    let res = s.solve_with_drag(b, 120.0 * MM, 0.0);
    (s, a, b, res)
}

/// Real constraints first, the pin second: the rod keeps its length and the
/// point stops where the geometry can no longer follow — fully extended
/// toward the cursor.
#[test]
fn wanted_an_unreachable_target_never_stretches_a_driving_distance() {
    let (s, a, b, _) = drag_rod_out_of_reach();
    assert!((dist(&s, a, b) - 50.0 * MM).abs() < HOLDS);
    let (x, y) = xy(&s, b);
    assert!(
        (x - 50.0 * MM).abs() < HOLDS && y.abs() < HOLDS,
        "b stops at the rod's reach, toward the target: ({}, {}) mm",
        x / MM,
        y / MM
    );
}

/// One session, stepped frame by frame as the app will hold it: the rod
/// swings a quarter turn under the cursor, its length held on every frame.
#[test]
fn a_session_swings_the_rod_frame_by_frame() {
    let (mut s, a, b) = rod();
    let mut drag = DragSession::new(&s, b);
    for i in 1..=9 {
        let angle = f64::from(i) * PI / 18.0;
        let res = drag.step(&mut s, 50.0 * MM * angle.cos(), 50.0 * MM * angle.sin());
        assert!(res.converged);
        assert!((dist(&s, a, b) - 50.0 * MM).abs() < HOLDS);
    }
    let (x, y) = xy(&s, b);
    assert!(x.abs() < HOLDS && (y - 50.0 * MM).abs() < HOLDS);
}

// (b) Branch flips under a large drag.

/// Two lines from the origin, in units of `u` metres: `base` fixed along
/// +x, `arm` 50 units long and 30° above it with a free far end `tip`, the
/// angle driven at 30°. The drag target is the tip's mirror image across
/// the base line, where the mirrored arm satisfies `|θ| − value` exactly.
fn drag_the_arm_across_the_base(u: f64) -> (Draft, PointId, SolveResult) {
    let mut s = Draft::seeded(1);
    let (a0, a1, base) = s.add_line(0.0, 0.0, 50.0 * u, 0.0);
    let (b0, tip, arm) = s.add_line(0.0, 0.0, 43.301_27 * u, 25.0 * u);
    fix(&mut s, a0);
    fix(&mut s, a1);
    fix(&mut s, b0);
    s.add_constraint(Constraint::Angle {
        a: base,
        b: arm,
        value: PI / 6.0,
    });
    let res = s.solve_with_drag(tip, 43.301_27 * u, -25.0 * u);
    (s, tip, res)
}

/// The arm's direction angle, signed: positive above the base line.
fn arm_angle(s: &Sketch, tip: PointId) -> f64 {
    let (x, y) = xy(s, tip);
    y.atan2(x)
}

/// At either scale the arm keeps its side and its angle, and the tip stops
/// at the point of its ray nearest the target — 25 units out. The old drag
/// flipped the metre arm and, on the millimetre one, let the driven angle
/// give a milliradian.
fn assert_the_arm_kept_its_side(u: f64) {
    let (s, tip, _) = drag_the_arm_across_the_base(u);
    let theta = arm_angle(&s, tip);
    assert!(
        (theta - PI / 6.0).abs() < 1e-6,
        "the arm stays 30° above the base: {}°",
        theta.to_degrees()
    );
    let (x, y) = xy(&s, tip);
    assert!(
        (x.hypot(y) - 25.0 * u).abs() < 1e-3 * u,
        "the tip is the ray's nearest point to the target: {} units out",
        x.hypot(y) / u
    );
}

#[test]
fn wanted_a_large_drag_keeps_the_angle_side_at_metre_scale() {
    assert_the_arm_kept_its_side(20.0 * MM);
}

#[test]
fn wanted_at_mm_scale_an_angle_holds_its_side_and_its_value() {
    assert_the_arm_kept_its_side(MM);
}

/// A fixed line along the x axis and a 20 mm circle sitting on top of it,
/// tangent, its radius driven.
fn circle_on_a_line() -> (Draft, PointId) {
    let mut s = Draft::seeded(1);
    let (l0, l1, line) = s.add_line(-100.0 * MM, 0.0, 100.0 * MM, 0.0);
    fix(&mut s, l0);
    fix(&mut s, l1);
    let (center, circle) = s.add_circle(0.0, 20.0 * MM, 20.0 * MM);
    s.add_constraint(Constraint::Radius {
        target: circle,
        value: 20.0 * MM,
    });
    s.add_constraint(Constraint::Tangent { line, circle });
    (s, center)
}

#[test]
fn wanted_a_large_drag_keeps_the_tangent_side() {
    let (mut s, center) = circle_on_a_line();
    s.solve_with_drag(center, 10.0 * MM, -20.0 * MM);
    let (_, y) = xy(&s, center);
    assert!(
        (y - 20.0 * MM).abs() < HOLDS,
        "the circle stays on top of the line: y = {} mm",
        y / MM
    );
}

/// A fixed 30 mm circle at the origin and a 10 mm circle touching it from
/// outside, both radii driven: centres 40 mm apart. The small circle's
/// centre is dragged to 15 mm from the big one's — well inside it.
struct Touching {
    /// Distance between the centres.
    d: f64,
    big_radius: f64,
    small_radius: f64,
}

fn drag_the_small_circle_inside() -> Touching {
    let mut s = Draft::seeded(1);
    let (big_center, big) = s.add_circle(0.0, 0.0, 30.0 * MM);
    fix(&mut s, big_center);
    let (small_center, small) = s.add_circle(40.0 * MM, 0.0, 10.0 * MM);
    s.add_constraint(Constraint::Radius {
        target: big,
        value: 30.0 * MM,
    });
    s.add_constraint(Constraint::Radius {
        target: small,
        value: 10.0 * MM,
    });
    s.add_constraint(Constraint::TangentCircles { a: big, b: small });
    s.solve_with_drag(small_center, 15.0 * MM, 0.0);
    let radius = |e: EntityId| match s.entity(e) {
        Some(Entity::Circle { radius, .. }) => *radius,
        other => panic!("not a circle: {other:?}"),
    };
    Touching {
        d: dist(&s, big_center, small_center),
        big_radius: radius(big),
        small_radius: radius(small),
    }
}

#[test]
fn wanted_a_large_drag_keeps_tangent_circles_external() {
    let t = drag_the_small_circle_inside();
    assert!(
        (t.d - 40.0 * MM).abs() < HOLDS,
        "the small circle still touches from outside: centres {} mm apart",
        t.d / MM
    );
    assert!((t.big_radius - 30.0 * MM).abs() < HOLDS);
    assert!((t.small_radius - 10.0 * MM).abs() < HOLDS);
}

/// A 20 mm arc about the fixed origin, counter-clockwise from a fixed start
/// on +x to a free end 60° round.
fn arc_60() -> (Draft, PointId, PointId, PointId) {
    let mut s = Draft::seeded(1);
    let (c, start, end, _) = s.add_arc(0.0, 0.0, 20.0 * MM, 0.0, 10.0 * MM, 17.320_508 * MM);
    fix(&mut s, c);
    fix(&mut s, start);
    (s, c, start, end)
}

/// The arc's sweep, counter-clockwise from start to end, in `(0, 2π]` — the
/// convention `region.rs` samples it by.
fn sweep(s: &Sketch, c: PointId, start: PointId, end: PointId) -> f64 {
    let (c, a, b) = (xy(s, c), xy(s, start), xy(s, end));
    let sweep = (b.1 - c.1).atan2(b.0 - c.0) - (a.1 - c.1).atan2(a.0 - c.0);
    if sweep <= 0.0 { sweep + TAU } else { sweep }
}

/// The target is on the arc's circle 30° the other side of the start. The
/// end follows the circle toward it and stops at the start instead of
/// passing through it — the old drag wrapped the 60° arc into its 330°
/// complement in one converged frame.
#[test]
fn wanted_a_large_drag_keeps_an_arc_sweep() {
    let (mut s, c, start, end) = arc_60();
    assert!((sweep(&s, c, start, end) - PI / 3.0).abs() < 1e-6);
    s.solve_with_drag(end, 17.320_508 * MM, -10.0 * MM);
    let after = sweep(&s, c, start, end);
    assert!(
        after <= PI / 3.0 + 1e-6,
        "the end stops short of the start instead of wrapping: {}°",
        after.to_degrees()
    );
    assert!((dist(&s, c, end) - 20.0 * MM).abs() < HOLDS);
}

// (b′) Flips along a connected path. A frame follows the
// constraints, so it cannot jump branches; these drags walk each
// orientation through the degenerate state between its readings instead,
// over many frames, as a hand would. Without the branch guard the first
// three flipped.

/// `session` stepped from where `p` stands along straight legs through
/// `path`, `frames` frames a leg.
fn walk(s: &mut Draft, p: PointId, path: &[(f64, f64)], frames: u32) {
    let mut drag = DragSession::new(s, p);
    let mut from = xy(s, p);
    for &to in path {
        for i in 1..=frames {
            let t = f64::from(i) / f64::from(frames);
            drag.step(
                s,
                from.0 + t * (to.0 - from.0),
                from.1 + t * (to.1 - from.1),
            );
        }
        from = to;
    }
}

/// The 30° arm of [`drag_the_arm_across_the_base`], its tip walked in to
/// the arm's fixed end and out to its mirror image. The arm cannot pass
/// through zero length, so the tip stops short of the end, then follows the
/// cursor back out along its own ray to the point nearest the target.
#[test]
fn wanted_an_arm_walked_through_its_fixed_end_keeps_its_side() {
    let mut s = Draft::seeded(1);
    let (a0, a1, base) = s.add_line(0.0, 0.0, 50.0 * MM, 0.0);
    let (b0, tip, arm) = s.add_line(0.0, 0.0, 43.301_27 * MM, 25.0 * MM);
    fix(&mut s, a0);
    fix(&mut s, a1);
    fix(&mut s, b0);
    s.add_constraint(Constraint::Angle {
        a: base,
        b: arm,
        value: PI / 6.0,
    });
    walk(&mut s, tip, &[(0.0, 0.0), (43.301_27 * MM, -25.0 * MM)], 20);
    assert!(
        (arm_angle(&s, tip) - PI / 6.0).abs() < 1e-6,
        "the arm stays 30° above the base: {}°",
        arm_angle(&s, tip).to_degrees()
    );
    assert!((dist(&s, b0, tip) - 25.0 * MM).abs() < 1e-3 * MM);
}

/// The same flip by three points: `AnglePoints` at a fixed vertex, the
/// free arm's end walked through the vertex to the mirror side.
#[test]
fn wanted_an_angle_by_points_walked_through_its_vertex_keeps_its_side() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(50.0 * MM, 0.0));
    let vertex = s.add_point(Point::fixed(0.0, 0.0));
    let tip = s.add_point(Point::new(43.301_27 * MM, 25.0 * MM));
    s.add_constraint(Constraint::AnglePoints {
        a,
        vertex,
        b: tip,
        value: PI / 6.0,
    });
    walk(&mut s, tip, &[(0.0, 0.0), (43.301_27 * MM, -25.0 * MM)], 20);
    assert!(
        (arm_angle(&s, tip) - PI / 6.0).abs() < 1e-6,
        "the arm stays 30° above: {}°",
        arm_angle(&s, tip).to_degrees()
    );
}

/// A circle tangent to a fixed line, its radius free, its centre walked
/// down through the line. The radius shrinks to its floor and the centre
/// stops just above the line instead of crossing to touch it from below.
#[test]
fn wanted_a_circle_shrunk_through_its_tangent_line_keeps_its_side() {
    let mut s = Draft::seeded(1);
    let (l0, l1, line) = s.add_line(-100.0 * MM, 0.0, 100.0 * MM, 0.0);
    fix(&mut s, l0);
    fix(&mut s, l1);
    let (center, circle) = s.add_circle(0.0, 20.0 * MM, 20.0 * MM);
    s.add_constraint(Constraint::Tangent { line, circle });
    walk(&mut s, center, &[(0.0, -20.0 * MM)], 40);
    let (_, y) = xy(&s, center);
    assert!(
        y > 0.0,
        "the centre stays above the line: y = {} mm",
        y / MM
    );
    let Some(Entity::Circle { radius, .. }) = s.entity(circle) else {
        panic!("the circle exists");
    };
    assert!(
        (radius - y).abs() < HOLDS,
        "and the circle still touches it"
    );
}

/// The arc's free end walked round its circle, 3° a frame, from 60° to
/// −30°: through the start, which it stops at.
#[test]
fn wanted_an_arc_end_walked_round_its_start_keeps_the_sweep() {
    let (mut s, c, start, end) = arc_60();
    let mut drag = DragSession::new(&s, end);
    for i in 0..=30 {
        let a = (60.0 - 3.0 * f64::from(i)).to_radians();
        drag.step(&mut s, 20.0 * MM * a.cos(), 20.0 * MM * a.sin());
    }
    let after = sweep(&s, c, start, end);
    assert!(
        after < 1e-3,
        "the end waits at the start: sweep {}°",
        after.to_degrees()
    );
    assert!((dist(&s, c, end) - 20.0 * MM).abs() < HOLDS);
}

/// A 10 mm circle outside a fixed 30 mm one, its radius free, walked in
/// toward the big circle's centre. Its radius shrinks to the floor as it
/// reaches the big circle, and it stops there: the tie between external
/// and internal is never reached, so `TangentCircles` needs no guard.
#[test]
fn a_circle_rolled_into_a_tangent_circle_stays_outside() {
    let mut s = Draft::seeded(1);
    let (big_center, big) = s.add_circle(0.0, 0.0, 30.0 * MM);
    fix(&mut s, big_center);
    s.add_constraint(Constraint::Radius {
        target: big,
        value: 30.0 * MM,
    });
    let (small_center, small) = s.add_circle(40.0 * MM, 0.0, 10.0 * MM);
    s.add_constraint(Constraint::TangentCircles { a: big, b: small });
    walk(&mut s, small_center, &[(15.0 * MM, 0.0)], 40);
    let Some(Entity::Circle { radius, .. }) = s.entity(small) else {
        panic!("the circle exists");
    };
    let d = dist(&s, big_center, small_center);
    assert!(d >= 30.0 * MM, "the centre stays outside: {} mm", d / MM);
    assert!(
        (d - 30.0 * MM - radius).abs() < HOLDS,
        "touching from outside"
    );
}

// (c) A frame that does not converge.

/// A two-bar linkage: `a` fixed at the origin, two driven 50 mm bars, the
/// free end `c` dragged 200 mm out — twice the linkage's reach.
fn linkage() -> (Draft, [PointId; 3]) {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = s.add_point(Point::new(50.0 * MM, 0.0));
    let c = s.add_point(Point::new(50.0 * MM, 50.0 * MM));
    s.add_constraint(Constraint::Distance {
        a,
        b,
        value: 50.0 * MM,
    });
    s.add_constraint(Constraint::Distance {
        a: b,
        b: c,
        value: 50.0 * MM,
    });
    (s, [a, b, c])
}

/// The over-reaching linkage is a converged frame: it stops
/// fully extended, both bars held.
#[test]
fn wanted_an_unreachable_linkage_frame_converges_at_full_reach() {
    let (mut s, [a, b, c]) = linkage();
    let frame = DragSession::new(&s, c).step(&mut s, 200.0 * MM, 0.0);
    assert!(frame.converged && !frame.blocked, "{frame:?}");
    assert!((dist(&s, a, b) - 50.0 * MM).abs() < HOLDS);
    assert!((dist(&s, b, c) - 50.0 * MM).abs() < HOLDS);
}

/// The rod with its length driven twice, at 50 and 60 mm: inconsistent
/// before any drag, so no frame can get onto the constraints.
fn contradicted_rod() -> (Draft, PointId) {
    let (mut s, a, b) = rod();
    s.add_constraint(Constraint::Distance {
        a,
        b,
        value: 60.0 * MM,
    });
    (s, b)
}

/// A frame that does not converge puts the last good state back and says
/// so: the cursor moves on, the geometry stays. The old drag left
/// its last iterate in the sketch.
#[test]
fn wanted_an_unconverged_frame_keeps_the_last_good_state() {
    let (mut s, b) = contradicted_rod();
    let before = positions(&s);
    let mut drag = DragSession::new(&s, b);
    for i in 1..=3 {
        let frame = drag.step(&mut s, 50.0 * MM, f64::from(i) * 10.0 * MM);
        assert!(frame.blocked, "frame {i}: {frame:?}");
        assert!(
            !frame.converged,
            "the state put back is the inconsistent one"
        );
        assert_eq!(positions(&s), before, "frame {i} moved nothing");
    }
    let (mut s, b) = contradicted_rod();
    assert!(!s.solve_with_drag(b, 50.0 * MM, 10.0 * MM).converged);
    assert_eq!(positions(&s), before, "nor does a one-frame drag");
}

// A drag once held its point with a per-frame `Fix`, a record per frame.

/// Every id the sketch holds.
fn ids(s: &Sketch) -> Vec<arrix_sketch::SketchEntityId> {
    let mut ids: Vec<_> = s.points().keys().copied().collect();
    ids.extend(s.entities().keys());
    ids.extend(s.constraints().keys());
    ids
}

#[test]
fn wanted_a_drag_adds_no_record() {
    let (mut s, _, b) = rod();
    let start = ids(&s);
    for i in 0..10 {
        s.solve_with_drag(b, 50.0 * MM, f64::from(i) * MM);
    }
    assert_eq!(ids(&s), start);
}

// (e) Entity drag.

fn radius_of(s: &Sketch, circle: EntityId) -> f64 {
    match s.entity(circle) {
        Some(Entity::Circle { radius, .. }) => *radius,
        other => panic!("{circle} is not a circle: {other:?}"),
    }
}

/// A free 40 mm line dragged by its body: both ends travel with the cursor,
/// so it keeps its length and its direction.
#[test]
fn a_line_dragged_by_its_body_translates() {
    let mut s = Draft::seeded(1);
    let (a, b, line) = s.add_line(0.0, 0.0, 40.0 * MM, 0.0);
    let grab = [20.0 * MM, 0.0];
    let mut drag = DragSession::new_entity(&s, line, grab);
    for i in 1..=5 {
        let t = f64::from(i) / 5.0;
        let frame = drag.step(&mut s, grab[0] + 10.0 * MM * t, grab[1] + 15.0 * MM * t);
        assert!(frame.converged && !frame.blocked, "{frame:?}");
    }
    let ((ax, ay), (bx, by)) = (xy(&s, a), xy(&s, b));
    assert!((ax - 10.0 * MM).abs() < HOLDS && (ay - 15.0 * MM).abs() < HOLDS);
    assert!((bx - 50.0 * MM).abs() < HOLDS && (by - 15.0 * MM).abs() < HOLDS);
    assert!((dist(&s, a, b) - 40.0 * MM).abs() < HOLDS);
}

/// An arc dragged by its body carries its centre and both ends: it keeps
/// its radius and its sweep.
#[test]
fn an_arc_dragged_by_its_body_translates() {
    // `arc_60`'s shape, nothing fixed.
    let mut s = Draft::seeded(1);
    let (c, start, end, arc) = s.add_arc(0.0, 0.0, 20.0 * MM, 0.0, 10.0 * MM, 17.320_508 * MM);
    let before = sweep(&s, c, start, end);
    let grab = [10.0 * MM, 17.0 * MM];
    let mut drag = DragSession::new_entity(&s, arc, grab);
    let frame = drag.step(&mut s, grab[0] - 5.0 * MM, grab[1] + 8.0 * MM);
    assert!(frame.converged);
    let (cx, cy) = xy(&s, c);
    assert!((cx + 5.0 * MM).abs() < HOLDS && (cy - 8.0 * MM).abs() < HOLDS);
    assert!((dist(&s, c, start) - 20.0 * MM).abs() < HOLDS);
    assert!((sweep(&s, c, start, end) - before).abs() < 1e-6);
}

/// A circle's rim changes only its radius: the centre stays where it was.
/// With a driving `Radius` it does not change at all.
#[test]
fn a_circle_rim_changes_only_the_radius_unless_it_is_driven() {
    let mut s = Draft::seeded(1);
    let (center, circle) = s.add_circle(10.0 * MM, 5.0 * MM, 20.0 * MM);
    let mut drag = DragSession::new_entity(&s, circle, [30.0 * MM, 5.0 * MM]);
    let frame = drag.step(&mut s, 10.0 * MM, 40.0 * MM);
    assert!(frame.converged && !frame.blocked);
    assert!((radius_of(&s, circle) - 35.0 * MM).abs() < HOLDS);
    let (x, y) = xy(&s, center);
    assert!((x - 10.0 * MM).abs() < HOLDS && (y - 5.0 * MM).abs() < HOLDS);

    s.add_constraint(Constraint::Radius {
        target: circle,
        value: 35.0 * MM,
    });
    let before = positions(&s);
    let mut drag = DragSession::new_entity(&s, circle, [45.0 * MM, 5.0 * MM]);
    let frame = drag.step(&mut s, 60.0 * MM, 5.0 * MM);
    assert!(frame.converged, "held, not failed: {frame:?}");
    assert!((radius_of(&s, circle) - 35.0 * MM).abs() < HOLDS);
    assert_eq!(positions(&s), before, "nothing else moved either");
}

/// The branch guard holds for a body drag too: a horizontal line tangent
/// to the top of a fixed circle, dragged down through it, stays on top —
/// sliding along with the cursor's sideways travel only.
#[test]
fn a_line_dragged_through_its_tangent_circle_keeps_its_side() {
    let mut s = Draft::seeded(1);
    let (center, circle) = s.add_circle(0.0, 0.0, 20.0 * MM);
    fix(&mut s, center);
    s.add_constraint(Constraint::Radius {
        target: circle,
        value: 20.0 * MM,
    });
    let (a, b, line) = s.add_line(-30.0 * MM, 20.0 * MM, 30.0 * MM, 20.0 * MM);
    s.add_constraint(Constraint::Horizontal { line });
    s.add_constraint(Constraint::Tangent { line, circle });
    let grab = [0.0, 20.0 * MM];
    let mut drag = DragSession::new_entity(&s, line, grab);
    for i in 1..=20 {
        let t = f64::from(i) / 20.0;
        drag.step(&mut s, 5.0 * MM * t, 20.0 * MM - 60.0 * MM * t);
    }
    let ((ax, ay), (_, by)) = (xy(&s, a), xy(&s, b));
    assert!(
        (ay - 20.0 * MM).abs() < HOLDS && (by - 20.0 * MM).abs() < HOLDS,
        "the line stays on top: y = {} mm",
        ay / MM
    );
    assert!(
        (ax + 25.0 * MM).abs() < 1e-5,
        "it slid with the cursor: {} mm",
        ax / MM
    );
}
