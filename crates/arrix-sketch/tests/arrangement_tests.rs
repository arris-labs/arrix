//! Closed-form curve intersection (`entity_intersections`): how many hits a
//! pair of curves has and where they are, degenerate cases included.
//!
//! Each case pins the *count* as tightly as
//! the positions — a tangency that answers with two hits a nanometre apart
//! would split a curve into a sliver piece.

use arrix_sketch::{Draft, Entity, Point, entity_intersections};

/// A millimetre of slack on positions: these are closed-form, so the error
/// is rounding, not method.
const EPS: f64 = 1.0e-9;

/// Asserts the hits are exactly `want`, in any order.
#[track_caller]
fn assert_hits(got: &[[f64; 2]], want: &[[f64; 2]]) {
    assert_eq!(
        got.len(),
        want.len(),
        "hit count: got {got:?}, want {want:?}"
    );
    for w in want {
        assert!(
            got.iter()
                .any(|g| (g[0] - w[0]).abs() <= EPS && (g[1] - w[1]).abs() <= EPS),
            "missing hit {w:?} in {got:?}"
        );
    }
}

/// The D-shaft's radius (20 mm) and the chord's height (10 mm).
const R: f64 = 0.02;
const CHORD_Y: f64 = 0.01;

// ---------------------------------------------------------------- line/line

#[test]
fn two_crossing_lines_meet_once() {
    let mut s = Draft::seeded(1);
    let (_, _, a) = s.add_line(-0.01, 0.0, 0.01, 0.0);
    let (_, _, b) = s.add_line(0.002, -0.01, 0.002, 0.01);

    assert_hits(&entity_intersections(&s, a, b), &[[0.002, 0.0]]);
    assert_hits(&entity_intersections(&s, b, a), &[[0.002, 0.0]]);
}

#[test]
fn lines_that_would_cross_beyond_their_ends_do_not_meet() {
    let mut s = Draft::seeded(1);
    let (_, _, a) = s.add_line(0.0, 0.0, 0.01, 0.0);
    let (_, _, b) = s.add_line(0.02, -0.01, 0.02, 0.01);

    assert!(entity_intersections(&s, a, b).is_empty());
}

#[test]
fn parallel_lines_never_meet() {
    let mut s = Draft::seeded(1);
    let (_, _, a) = s.add_line(0.0, 0.0, 0.01, 0.0);
    let (_, _, b) = s.add_line(0.0, 0.001, 0.01, 0.001);

    assert!(entity_intersections(&s, a, b).is_empty());
}

#[test]
fn a_shared_endpoint_is_a_hit() {
    // The corner of a rectangle: the splitter cuts nothing here, but the
    // arrangement has to see the vertex.
    let mut s = Draft::seeded(1);
    let corner = s.add_point(Point::new(0.01, 0.0));
    let p0 = s.add_point(Point::new(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.01, 0.01));
    let a = s.add_entity(Entity::Line {
        start: p0,
        end: corner,
    });
    let b = s.add_entity(Entity::Line {
        start: corner,
        end: p1,
    });

    assert_hits(&entity_intersections(&s, a, b), &[[0.01, 0.0]]);
}

#[test]
fn a_t_junction_hits_the_stem_end() {
    let mut s = Draft::seeded(1);
    let (_, _, bar) = s.add_line(0.0, 0.0, 0.02, 0.0);
    let (_, _, stem) = s.add_line(0.01, 0.0, 0.01, 0.01);

    assert_hits(&entity_intersections(&s, bar, stem), &[[0.01, 0.0]]);
}

#[test]
fn collinear_overlapping_lines_hit_at_the_overlap_ends() {
    // [0, 20] and [10, 30] on the X axis overlap over [10, 20]; those two
    // ends are the only places either can be cut.
    let mut s = Draft::seeded(1);
    let (_, _, a) = s.add_line(0.0, 0.0, 0.02, 0.0);
    let (_, _, b) = s.add_line(0.01, 0.0, 0.03, 0.0);

    assert_hits(&entity_intersections(&s, a, b), &[[0.01, 0.0], [0.02, 0.0]]);
}

#[test]
fn collinear_disjoint_lines_never_meet() {
    let mut s = Draft::seeded(1);
    let (_, _, a) = s.add_line(0.0, 0.0, 0.01, 0.0);
    let (_, _, b) = s.add_line(0.02, 0.0, 0.03, 0.0);

    assert!(entity_intersections(&s, a, b).is_empty());
}

#[test]
fn a_collinear_line_inside_another_hits_at_both_of_its_ends() {
    let mut s = Draft::seeded(1);
    let (_, _, outer) = s.add_line(0.0, 0.0, 0.03, 0.0);
    let (_, _, inner) = s.add_line(0.01, 0.0, 0.02, 0.0);

    assert_hits(
        &entity_intersections(&s, outer, inner),
        &[[0.01, 0.0], [0.02, 0.0]],
    );
}

// -------------------------------------------------------------- line/circle

#[test]
fn the_d_shafts_chord_crosses_its_circle_twice() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, chord) = s.add_line(-0.03, CHORD_Y, 0.03, CHORD_Y);

    let x = (R * R - CHORD_Y * CHORD_Y).sqrt();
    assert_hits(
        &entity_intersections(&s, circle, chord),
        &[[-x, CHORD_Y], [x, CHORD_Y]],
    );
}

#[test]
fn a_chord_through_the_centre_hits_both_poles() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, diameter) = s.add_line(-0.03, 0.0, 0.03, 0.0);

    assert_hits(
        &entity_intersections(&s, circle, diameter),
        &[[-R, 0.0], [R, 0.0]],
    );
}

#[test]
fn a_line_tangent_to_a_circle_hits_once() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, tangent) = s.add_line(-0.03, R, 0.03, R);

    assert_hits(&entity_intersections(&s, circle, tangent), &[[0.0, R]]);
}

#[test]
fn a_line_clear_of_a_circle_never_meets_it() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, line) = s.add_line(-0.03, R + 0.001, 0.03, R + 0.001);

    assert!(entity_intersections(&s, circle, line).is_empty());
}

#[test]
fn a_chord_that_stops_inside_the_circle_hits_once() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, stub) = s.add_line(0.0, CHORD_Y, 0.03, CHORD_Y);

    let x = (R * R - CHORD_Y * CHORD_Y).sqrt();
    assert_hits(&entity_intersections(&s, circle, stub), &[[x, CHORD_Y]]);
}

#[test]
fn a_line_ending_on_the_rim_hits_there() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, spoke) = s.add_line(0.0, 0.0, R, 0.0);

    assert_hits(&entity_intersections(&s, circle, spoke), &[[R, 0.0]]);
}

// ----------------------------------------------------------------- line/arc

/// The upper half of the circle of radius [`R`] about the origin, CCW from
/// `(R, 0)` to `(-R, 0)`.
fn upper_half_arc(s: &mut Draft) -> arrix_sketch::EntityId {
    let (_, _, _, arc) = s.add_arc(0.0, 0.0, R, 0.0, -R, 0.0);
    arc
}

#[test]
fn a_line_crossing_the_full_circle_hits_the_arc_only_where_the_arc_runs() {
    let mut s = Draft::seeded(1);
    let arc = upper_half_arc(&mut s);
    // A vertical line through x = 10 mm crosses the circle at ±sqrt(3)·10 mm;
    // only the upper hit is on the upper half.
    let (_, _, line) = s.add_line(0.01, -0.03, 0.01, 0.03);

    let y = (R * R - 0.01 * 0.01).sqrt();
    assert_hits(&entity_intersections(&s, arc, line), &[[0.01, y]]);
}

#[test]
fn a_line_through_an_arcs_own_endpoint_hits_there() {
    let mut s = Draft::seeded(1);
    let arc = upper_half_arc(&mut s);
    let (_, _, line) = s.add_line(R, -0.01, R, 0.01);

    assert_hits(&entity_intersections(&s, arc, line), &[[R, 0.0]]);
}

#[test]
fn a_line_crossing_only_the_missing_half_never_meets_the_arc() {
    let mut s = Draft::seeded(1);
    let arc = upper_half_arc(&mut s);
    let (_, _, line) = s.add_line(-0.03, -CHORD_Y, 0.03, -CHORD_Y);

    assert!(entity_intersections(&s, arc, line).is_empty());
}

#[test]
fn the_d_shafts_chord_crosses_its_major_arc_at_both_ends() {
    // The hand-drawn D: the major arc below y = 10 mm plus its chord. Both
    // hits are the arc's own endpoints — the pair is already split.
    let mut s = Draft::seeded(1);
    let x = (R * R - CHORD_Y * CHORD_Y).sqrt();
    let (_, _, _, arc) = s.add_arc(0.0, 0.0, x, CHORD_Y, -x, CHORD_Y);
    let (_, _, chord) = s.add_line(-x, CHORD_Y, x, CHORD_Y);

    assert_hits(
        &entity_intersections(&s, arc, chord),
        &[[-x, CHORD_Y], [x, CHORD_Y]],
    );
}

// ------------------------------------------------------------ circle/circle

#[test]
fn two_overlapping_circles_meet_twice() {
    let mut s = Draft::seeded(1);
    let (_, a) = s.add_circle(0.0, 0.0, 0.01);
    let (_, b) = s.add_circle(0.01, 0.0, 0.01);

    let y = (0.01f64 * 0.01 - 0.005 * 0.005).sqrt();
    assert_hits(&entity_intersections(&s, a, b), &[[0.005, -y], [0.005, y]]);
}

#[test]
fn externally_tangent_circles_meet_once() {
    let mut s = Draft::seeded(1);
    let (_, a) = s.add_circle(0.0, 0.0, 0.01);
    let (_, b) = s.add_circle(0.02, 0.0, 0.01);

    assert_hits(&entity_intersections(&s, a, b), &[[0.01, 0.0]]);
}

#[test]
fn internally_tangent_circles_meet_once() {
    // The small circle sits inside the big one, nudged towards +x, so they
    // touch at (R, 0) — on the centre line, past the far centre.
    let mut s = Draft::seeded(1);
    let (_, big) = s.add_circle(0.0, 0.0, R);
    let (_, small) = s.add_circle(0.005, 0.0, R - 0.005);

    assert_hits(&entity_intersections(&s, big, small), &[[R, 0.0]]);
    assert_hits(&entity_intersections(&s, small, big), &[[R, 0.0]]);
}

#[test]
fn a_circle_tangent_inside_a_larger_one_touches_behind_its_own_centre() {
    // The same pair with the radii the other way round: the containing
    // circle's centre is on the +x side, so the touch is at -r of the
    // smaller. The centre-line formula has to answer behind the centre.
    let mut s = Draft::seeded(1);
    let (_, small) = s.add_circle(0.0, 0.0, 0.005);
    let (_, big) = s.add_circle(0.015, 0.0, R);

    assert_hits(&entity_intersections(&s, small, big), &[[-0.005, 0.0]]);
    assert_hits(&entity_intersections(&s, big, small), &[[-0.005, 0.0]]);
}

#[test]
fn separate_circles_never_meet() {
    let mut s = Draft::seeded(1);
    let (_, a) = s.add_circle(0.0, 0.0, 0.01);
    let (_, b) = s.add_circle(0.05, 0.0, 0.01);

    assert!(entity_intersections(&s, a, b).is_empty());
}

#[test]
fn a_circle_nested_in_another_never_meets_it() {
    let mut s = Draft::seeded(1);
    let (_, outer) = s.add_circle(0.0, 0.0, R);
    let (_, inner) = s.add_circle(0.0, 0.0, 0.005);

    assert!(entity_intersections(&s, outer, inner).is_empty());
}

// --------------------------------------------------------------- arc/arc

#[test]
fn two_arcs_meet_only_where_both_of_them_run() {
    // The upper half of one circle and the upper half of a second, offset
    // circle: the underlying circles cross twice, both above the axis.
    let mut s = Draft::seeded(1);
    let upper = upper_half_arc(&mut s);
    let (_, _, _, other) = s.add_arc(0.01, 0.0, 0.01 + R, 0.0, 0.01 - R, 0.0);

    let x = 0.005;
    let y = (R * R - x * x).sqrt();
    assert_hits(&entity_intersections(&s, upper, other), &[[x, y]]);
}

#[test]
fn arcs_of_the_same_circle_hit_where_one_ends_inside_the_other() {
    // Two overlapping arcs of the same circle: the upper half and the right
    // half. They share the quarter from (R, 0) to (0, R), and the only
    // places either can be cut are those two ends.
    let mut s = Draft::seeded(1);
    let upper = upper_half_arc(&mut s);
    let (_, _, _, right) = s.add_arc(0.0, 0.0, 0.0, -R, 0.0, R);

    assert_hits(
        &entity_intersections(&s, upper, right),
        &[[R, 0.0], [0.0, R]],
    );
}

#[test]
fn disjoint_arcs_of_the_same_circle_never_meet() {
    let mut s = Draft::seeded(1);
    let (_, _, _, upper_right) = s.add_arc(0.0, 0.0, R, 0.0, 0.0, R);
    let (_, _, _, lower_left) = s.add_arc(0.0, 0.0, -R, 0.0, 0.0, -R);

    assert!(entity_intersections(&s, upper_right, lower_left).is_empty());
}

// ------------------------------------------------------------- what is not cut

#[test]
fn an_entity_never_intersects_itself() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);

    assert!(entity_intersections(&s, circle, circle).is_empty());
}

#[test]
fn a_point_is_not_a_curve() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let p = s.add_point(Point::new(R, 0.0));
    let point = s.add_entity(Entity::Point(p));

    assert!(entity_intersections(&s, circle, point).is_empty());
}

#[test]
fn a_degenerate_curve_is_not_a_curve() {
    let mut s = Draft::seeded(1);
    let (_, _, line) = s.add_line(-0.01, 0.0, 0.01, 0.0);
    let (_, dot) = s.add_circle(0.0, 0.0, 0.0);

    assert!(entity_intersections(&s, line, dot).is_empty());
}

// ------------------------------------------------------------------ pieces

use arrix_sketch::{EntityPiece, entity_pieces};

#[track_caller]
fn assert_at(got: [f64; 2], want: [f64; 2], what: &str) {
    assert!(
        (got[0] - want[0]).abs() <= EPS && (got[1] - want[1]).abs() <= EPS,
        "{what}: got {got:?}, want {want:?}"
    );
}

/// The indices of the pieces, in the order they came back.
fn indices(pieces: &[EntityPiece]) -> Vec<Option<u32>> {
    pieces.iter().map(|p| p.index).collect()
}

#[test]
fn an_uncrossed_line_is_one_unnumbered_piece() {
    let mut s = Draft::seeded(1);
    let (_, _, line) = s.add_line(0.0, 0.0, 0.02, 0.0);

    let pieces = entity_pieces(&s, line);
    assert_eq!(indices(&pieces), vec![None]);
    assert_at(pieces[0].start, [0.0, 0.0], "start");
    assert_at(pieces[0].end, [0.02, 0.0], "end");
}

#[test]
fn an_uncrossed_circle_is_one_unnumbered_piece() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);

    let pieces = entity_pieces(&s, circle);
    assert_eq!(indices(&pieces), vec![None]);
    // A whole circle starts and ends at its parameter origin, angle 0.
    assert_at(pieces[0].start, [R, 0.0], "start");
    assert_at(pieces[0].end, [R, 0.0], "end");
    assert_at(pieces[0].mid, [-R, 0.0], "mid");
}

#[test]
fn two_crossing_lines_each_split_in_two() {
    let mut s = Draft::seeded(1);
    let (_, _, across) = s.add_line(-0.01, 0.0, 0.01, 0.0);
    let (_, _, down) = s.add_line(0.0, -0.01, 0.0, 0.01);

    let a = entity_pieces(&s, across);
    assert_eq!(indices(&a), vec![Some(0), Some(1)]);
    assert_at(
        a[0].start,
        [-0.01, 0.0],
        "piece 0 starts at the line's start",
    );
    assert_at(a[0].end, [0.0, 0.0], "piece 0 ends at the crossing");
    assert_at(a[1].start, [0.0, 0.0], "piece 1 starts at the crossing");
    assert_at(a[1].end, [0.01, 0.0], "piece 1 ends at the line's end");

    assert_eq!(indices(&entity_pieces(&s, down)), vec![Some(0), Some(1)]);
}

#[test]
fn a_line_crossed_twice_splits_in_order_from_its_own_start() {
    let mut s = Draft::seeded(1);
    let (_, _, bar) = s.add_line(0.0, 0.0, 0.03, 0.0);
    // Added in the far-to-near order, so the pieces cannot be in the order
    // the crossings were found in.
    s.add_line(0.02, -0.01, 0.02, 0.01);
    s.add_line(0.01, -0.01, 0.01, 0.01);

    let pieces = entity_pieces(&s, bar);
    assert_eq!(indices(&pieces), vec![Some(0), Some(1), Some(2)]);
    assert_at(pieces[0].end, [0.01, 0.0], "first cut");
    assert_at(pieces[1].end, [0.02, 0.0], "second cut");
    assert_at(pieces[2].end, [0.03, 0.0], "the line's own end");
}

/// The D-shaft sketch: a circle of radius [`R`] and a chord
/// across it at `y`, drawn long enough to overhang both sides.
fn d_shaft(y: f64) -> (Draft, arrix_sketch::EntityId, arrix_sketch::EntityId) {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, chord) = s.add_line(-0.03, y, 0.03, y);
    (s, circle, chord)
}

#[test]
fn the_d_shafts_chord_and_circle_split_where_they_cross() {
    let (s, circle, chord) = d_shaft(CHORD_Y);
    let x = (R * R - CHORD_Y * CHORD_Y).sqrt();

    // The chord overhangs, so it is three pieces: a stub, the chord proper,
    // a stub. Only the middle one bounds anything.
    let chord_pieces = entity_pieces(&s, chord);
    assert_eq!(indices(&chord_pieces), vec![Some(0), Some(1), Some(2)]);
    assert_at(chord_pieces[1].start, [-x, CHORD_Y], "the chord's left end");
    assert_at(chord_pieces[1].end, [x, CHORD_Y], "the chord's right end");

    // The circle is the D and the cap, in that order: it is numbered from
    // the crossing that comes first along the chord, and the
    // chord is drawn left to right.
    let circle_pieces = entity_pieces(&s, circle);
    assert_eq!(indices(&circle_pieces), vec![Some(0), Some(1)]);
    assert_at(circle_pieces[0].start, [-x, CHORD_Y], "the D starts left");
    assert_at(
        circle_pieces[0].mid,
        [0.0, -R],
        "the D runs under the bottom",
    );
    assert_at(circle_pieces[1].start, [x, CHORD_Y], "the cap starts right");
    assert_at(circle_pieces[1].mid, [0.0, R], "the cap runs over the top");
}

#[test]
fn the_d_shafts_pieces_keep_their_order_when_the_chord_moves() {
    // The chord's driving dimension is what an edit moves; here the points
    // themselves stand in for it. Each piece has to stay the same *arc* — a
    // piece index names a side face, so a reshuffle
    // would re-bind a stored name to a different face.
    //
    // Numbered from the chord's first crossing, piece 0 is the arc
    // below the chord at every height, including heights where it is the cap
    // rather than the D: the chord is walked left to right, so the piece
    // leaving its left crossing runs CCW under the bottom pole. The pole is
    // the piece's own mid-parameter point whatever `y` is, which is the
    // stability being asserted — the old numbering, from angle 0, mirrored
    // it with the sign of `y` instead.
    for y in [0.015, 0.01, 0.005, -0.005, -0.01, -0.015] {
        let (s, circle, chord) = d_shaft(y);
        let x = (R * R - y * y).sqrt();

        assert_eq!(
            indices(&entity_pieces(&s, chord)),
            vec![Some(0), Some(1), Some(2)],
            "chord at y = {y}"
        );
        let pieces = entity_pieces(&s, circle);
        assert_eq!(indices(&pieces), vec![Some(0), Some(1)], "y = {y}");
        assert_at(
            pieces[0].start,
            [-x, y],
            "piece 0 starts at the left crossing",
        );
        assert_at(pieces[0].mid, [0.0, -R], "piece 0 runs under the bottom");
        assert_at(pieces[1].mid, [0.0, R], "piece 1 runs over the top");
    }
}

#[test]
fn a_cut_sweeping_past_the_parameter_origin_keeps_the_circles_numbering() {
    // The chord pivots about the
    // circle's centre through the horizontal, so at `k == 0` a crossing sits
    // exactly on the parameter origin, `(R, 0)`, and either side of that the
    // two crossings swap places in the sort by angle. Numbered from angle 0
    // the two arcs would swap indices there — a stored side-face name silently
    // re-bound to the other side of the circle by a dimension edit, which is
    // what `SEED.md` §8.2 forbids. Numbered from the chord's own first crossing
    // they do not: piece 0 is the arc under the chord throughout.
    for k in [0.01, 0.005, 0.0, -0.005, -0.01] {
        let mut s = Draft::seeded(1);
        let (_, circle) = s.add_circle(0.0, 0.0, R);
        s.add_line(-0.03, -k, 0.03, k);

        let pieces = entity_pieces(&s, circle);
        assert_eq!(indices(&pieces), vec![Some(0), Some(1)], "k = {k}");
        assert!(
            pieces[0].mid[1] < -0.9 * R,
            "piece 0 must stay the arc under the chord, k = {k}: {:?}",
            pieces[0].mid
        );
        assert!(
            pieces[1].mid[1] > 0.9 * R,
            "piece 1 must stay the arc over it, k = {k}: {:?}",
            pieces[1].mid
        );
    }
}

#[test]
fn two_circles_cutting_each_other_are_the_residue_the_rule_cannot_remove() {
    // The accepted limit of numbering from the first crossing. A closed
    // curve is numbered from the first crossing along the entity that cuts
    // it — and when that entity is itself closed, "first" is read off *its*
    // parameter origin, which is a coordinate again. Rotating the second
    // circle about the first sweeps a crossing past that origin, and the two
    // arcs of the first swap indices with no change to the crossing set.
    // Pinned rather than fixed: a circle cut by a circle is not the D-shaft
    // scenario, and closing it needs an identity a coordinate cannot give.
    let cut_by_circle_at = |alpha_deg: f64| {
        let alpha = alpha_deg.to_radians();
        let (cx, cy) = (0.03 * alpha.cos(), 0.03 * alpha.sin());
        let mut s = Draft::seeded(1);
        let (_, circle) = s.add_circle(0.0, 0.0, R);
        s.add_circle(cx, cy, R);
        // The crossings, mirrored about the line of centres: `plus` is the
        // one to its left.
        let h = (R * R - 0.015 * 0.015).sqrt();
        let n = [-cy / 0.03, cx / 0.03];
        let m = [cx / 2.0, cy / 2.0];
        let plus = [m[0] + h * n[0], m[1] + h * n[1]];
        let minus = [m[0] - h * n[0], m[1] - h * n[1]];
        (entity_pieces(&s, circle), plus, minus)
    };

    let (wide, plus, _) = cut_by_circle_at(225.0);
    assert_eq!(indices(&wide), vec![Some(0), Some(1)]);
    assert_at(
        wide[0].start,
        plus,
        "at 225° piece 0 starts at one crossing",
    );

    let (turned, _, minus) = cut_by_circle_at(215.0);
    assert_eq!(indices(&turned), vec![Some(0), Some(1)]);
    assert_at(
        turned[0].start,
        minus,
        "ten degrees round, it starts at the other one",
    );
}

#[test]
fn a_chord_flush_with_the_rim_leaves_itself_whole() {
    // The D drawn by hand: the chord's own ends are the crossings, so it is
    // cut at neither of them, and the circle is still the cap and the D.
    let x = (R * R - CHORD_Y * CHORD_Y).sqrt();
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, chord) = s.add_line(-x, CHORD_Y, x, CHORD_Y);

    assert_eq!(indices(&entity_pieces(&s, chord)), vec![None]);
    let circle_pieces = entity_pieces(&s, circle);
    assert_eq!(indices(&circle_pieces), vec![Some(0), Some(1)]);
    assert_at(circle_pieces[0].mid, [0.0, -R], "the D is still piece 0");
}

#[test]
fn a_tangent_line_gains_a_vertex_without_splitting_the_circle() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, tangent) = s.add_line(-0.03, R, 0.03, R);

    // One touch is one cut: the circle is still a single piece, so its side
    // face keeps the name it has today — but the piece now starts at the
    // touch, which is the vertex the walk needs.
    let circle_pieces = entity_pieces(&s, circle);
    assert_eq!(indices(&circle_pieces), vec![None]);
    assert_at(circle_pieces[0].start, [0.0, R], "the touch");
    assert_at(circle_pieces[0].end, [0.0, R], "back to the touch");
    assert_at(circle_pieces[0].mid, [0.0, -R], "a full turn round");

    // The line is cut there, because the touch is in its interior.
    assert_eq!(indices(&entity_pieces(&s, tangent)), vec![Some(0), Some(1)]);
}

#[test]
fn a_shared_corner_splits_neither_line() {
    let mut s = Draft::seeded(1);
    let corner = s.add_point(Point::new(0.01, 0.0));
    let p0 = s.add_point(Point::new(0.0, 0.0));
    let p1 = s.add_point(Point::new(0.01, 0.01));
    let a = s.add_entity(Entity::Line {
        start: p0,
        end: corner,
    });
    let b = s.add_entity(Entity::Line {
        start: corner,
        end: p1,
    });

    assert_eq!(indices(&entity_pieces(&s, a)), vec![None]);
    assert_eq!(indices(&entity_pieces(&s, b)), vec![None]);
}

#[test]
fn a_t_junction_splits_only_the_bar() {
    let mut s = Draft::seeded(1);
    let (_, _, bar) = s.add_line(0.0, 0.0, 0.02, 0.0);
    let (_, _, stem) = s.add_line(0.01, 0.0, 0.01, 0.01);

    assert_eq!(indices(&entity_pieces(&s, bar)), vec![Some(0), Some(1)]);
    assert_eq!(indices(&entity_pieces(&s, stem)), vec![None]);
}

#[test]
fn construction_geometry_cuts_nothing_and_is_not_cut() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, R);
    let (_, _, chord) = s.add_line(-0.03, CHORD_Y, 0.03, CHORD_Y);
    s.set_construction(chord, true);

    assert_eq!(indices(&entity_pieces(&s, circle)), vec![None]);
    assert!(entity_pieces(&s, chord).is_empty());
}

#[test]
fn a_point_has_no_pieces() {
    let mut s = Draft::seeded(1);
    let p = s.add_point(Point::new(0.0, 0.0));
    let point = s.add_entity(Entity::Point(p));

    assert!(entity_pieces(&s, point).is_empty());
}

#[test]
fn an_arc_is_cut_along_its_own_direction() {
    // The upper half of the circle, crossed by two verticals. The pieces run
    // CCW from (R, 0), which is the arc's start — not from the left.
    let mut s = Draft::seeded(1);
    let arc = upper_half_arc(&mut s);
    s.add_line(0.01, 0.0, 0.01, 0.03);
    s.add_line(-0.01, 0.0, -0.01, 0.03);

    let pieces = entity_pieces(&s, arc);
    assert_eq!(indices(&pieces), vec![Some(0), Some(1), Some(2)]);
    let y = (R * R - 0.01 * 0.01).sqrt();
    assert_at(pieces[0].start, [R, 0.0], "the arc's own start");
    assert_at(pieces[0].end, [0.01, y], "the right crossing");
    assert_at(pieces[1].end, [-0.01, y], "the left crossing");
    assert_at(pieces[2].end, [-R, 0.0], "the arc's own end");
}

// ------------------------------------------------------------------- walk

// The half-edge walk (`arrangement_loops`): which faces the arrangement's
// pieces bound, and that a walk never takes the wrong turn at an arc
//.

use std::f64::consts::PI;

use arrix_sketch::{ArrangementLoop, arrangement_loops};

/// Areas are exact here — an arc contributes its closed form, not a sampled
/// rim — so the slack is rounding.
#[track_caller]
fn assert_area(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-9 * want.abs().max(1e-6),
        "{what}: got {got}, want {want}"
    );
}

/// The one edge of `lp` on `entity`'s piece `piece`, or a failure naming the
/// loop.
#[track_caller]
fn edge_on(lp: &ArrangementLoop, entity: arrix_sketch::EntityId, piece: Option<u32>) -> bool {
    lp.edges
        .iter()
        .any(|e| e.entity == entity && e.piece == piece)
}

#[test]
fn a_square_is_its_own_face_and_its_outside() {
    let mut s = Draft::seeded(1);
    let corners: Vec<_> = [(0.0, 0.0), (0.02, 0.0), (0.02, 0.02), (0.0, 0.02)]
        .into_iter()
        .map(|(x, y)| s.add_point(Point::new(x, y)))
        .collect();
    for i in 0..4 {
        s.add_entity(Entity::Line {
            start: corners[i],
            end: corners[(i + 1) % 4],
        });
    }

    let loops = arrangement_loops(&s);
    assert_eq!(loops.len(), 2, "{loops:#?}");
    assert_area(loops[0].signed_area, 0.02 * 0.02, "the square");
    assert_area(loops[1].signed_area, -0.02 * 0.02, "its outside");
    assert_eq!(loops[0].edges.len(), 4);
    assert_eq!(loops[1].edges.len(), 4);
}

#[test]
fn a_lone_circle_is_a_disc_and_its_outside() {
    let mut s = Draft::seeded(1);
    s.add_circle(0.0, 0.0, R);

    let loops = arrangement_loops(&s);
    assert_eq!(loops.len(), 2, "{loops:#?}");
    assert_area(loops[0].signed_area, PI * R * R, "the disc");
    assert_area(loops[1].signed_area, -PI * R * R, "its outside");
    // Uncut, so it is one unnumbered piece traversed each way.
    assert_eq!(loops[0].edges.len(), 1);
    assert_eq!(loops[0].edges[0].piece, None);
    assert!(loops[0].edges[0].forward);
    assert!(!loops[1].edges[0].forward);
}

#[test]
fn a_plate_crossed_by_a_line_walks_into_two_faces() {
    // The line's ends rest on the plate's edges without sharing a point, so
    // this is the case the coincidence walk could not see at all.
    let mut s = Draft::seeded(1);
    let corners: Vec<_> = [(0.0, 0.0), (0.1, 0.0), (0.1, 0.06), (0.0, 0.06)]
        .into_iter()
        .map(|(x, y)| s.add_point(Point::new(x, y)))
        .collect();
    for i in 0..4 {
        s.add_entity(Entity::Line {
            start: corners[i],
            end: corners[(i + 1) % 4],
        });
    }
    s.add_line(0.05, 0.0, 0.05, 0.06);

    let loops = arrangement_loops(&s);
    assert_eq!(loops.len(), 3, "{loops:#?}");
    assert_area(loops[0].signed_area, 0.05 * 0.06, "the left half");
    assert_area(loops[1].signed_area, 0.05 * 0.06, "the right half");
    assert_area(loops[2].signed_area, -0.1 * 0.06, "the outside");
    for half in &loops[..2] {
        assert_eq!(half.edges.len(), 4, "{half:#?}");
    }
}

#[test]
fn the_d_shafts_two_regions_are_the_cap_and_the_d() {
    // The D-shaft scenario.
    // At either crossing the cap and the D leave along the *same chord* —
    // both run to the other crossing — so a chord-ordered walk has a
    // three-way tie with the chord piece itself and cannot tell which arc
    // continues which face. Their tangents are mirror images about the
    // chord, so the tangent order has no tie at all.
    let (s, circle, chord) = d_shaft(CHORD_Y);
    let theta = 2.0 * (CHORD_Y / R).acos();
    let cap_area = 0.5 * R * R * (theta - theta.sin());
    let d_area = PI * R * R - cap_area;

    let loops = arrangement_loops(&s);
    assert_eq!(loops.len(), 3, "{loops:#?}");

    let d = &loops[0];
    assert_area(d.signed_area, d_area, "the D");
    assert_eq!(d.edges.len(), 2, "a two-edge loop: the arc and its chord");
    assert!(edge_on(d, circle, Some(0)), "the D arc: {d:#?}");
    assert!(edge_on(d, chord, Some(1)), "the chord proper: {d:#?}");
    assert_at(
        d.edges.iter().find(|e| e.entity == circle).unwrap().mid,
        [0.0, -R],
        "the D runs under the bottom",
    );

    let cap = &loops[1];
    assert_area(cap.signed_area, cap_area, "the cap");
    assert_eq!(cap.edges.len(), 2);
    assert!(edge_on(cap, circle, Some(1)), "the cap arc: {cap:#?}");
    assert_at(
        cap.edges.iter().find(|e| e.entity == circle).unwrap().mid,
        [0.0, R],
        "the cap runs over the top",
    );

    // The chord proper bounds both faces, once each way.
    let d_forward = d.edges.iter().find(|e| e.entity == chord).unwrap().forward;
    let cap_forward = cap
        .edges
        .iter()
        .find(|e| e.entity == chord)
        .unwrap()
        .forward;
    assert_ne!(d_forward, cap_forward, "the chord bounds both faces");

    // The outside is the rim the other way plus each overhanging stub, out
    // and back, which encloses nothing.
    let outside = &loops[2];
    assert_area(outside.signed_area, -PI * R * R, "the outside");
    assert_eq!(outside.edges.len(), 6, "{outside:#?}");
}

#[test]
fn a_lens_of_two_arcs_is_a_face() {
    // Two arcs sharing both endpoints: the other awkward shape, and
    // the other chord-order tie — the two arcs have the same chord.
    let (rad, c) = (R, 0.006);
    let h = (rad * rad - c * c).sqrt();
    let mut s = Draft::seeded(1);
    s.add_arc(-c, 0.0, 0.0, -h, 0.0, h);
    s.add_arc(c, 0.0, 0.0, h, 0.0, -h);

    let theta = 2.0 * h.atan2(c);
    let lens = rad * rad * (theta - theta.sin());

    let loops = arrangement_loops(&s);
    assert_eq!(loops.len(), 2, "{loops:#?}");
    assert_area(loops[0].signed_area, lens, "the lens");
    assert_area(loops[1].signed_area, -lens, "its outside");
    assert_eq!(loops[0].edges.len(), 2);
    assert_eq!(loops[1].edges.len(), 2);
}

#[test]
fn a_circle_cut_by_two_chords_walks_into_four_quadrants() {
    // Four rim vertices where an arc meets a line, and one in the middle
    // where two lines cross: every turn has to be the sharpest left one.
    let mut s = Draft::seeded(1);
    s.add_circle(0.0, 0.0, R);
    s.add_line(-R, 0.0, R, 0.0);
    s.add_line(0.0, -R, 0.0, R);

    let loops = arrangement_loops(&s);
    assert_eq!(loops.len(), 5, "{loops:#?}");
    for quadrant in &loops[..4] {
        assert_area(quadrant.signed_area, PI * R * R / 4.0, "a quadrant");
        assert_eq!(quadrant.edges.len(), 3, "{quadrant:#?}");
    }
    assert_area(loops[4].signed_area, -PI * R * R, "the outside");
}
