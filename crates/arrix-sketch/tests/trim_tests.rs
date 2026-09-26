//! `modify::trim` and its id rules: the picked piece goes, what is left
//! keeps the entity's id, a circle becomes an arc under its own id, a curve
//! with no crossing is deleted, and every new end is held on the curve that
//! cut it.

use arrix_core::Id;
use arrix_sketch::Draft;
use arrix_sketch::modify::{self, ModifyError, TrimEdit};
use arrix_sketch::{Constraint, Entity, EntityId, Point, PointId, Sketch};

const TOL: f64 = 1e-9;

fn mm(x: f64, y: f64) -> [f64; 2] {
    [x * 1e-3, y * 1e-3]
}

fn line(s: &mut Draft, a: [f64; 2], b: [f64; 2]) -> EntityId {
    s.add_line(a[0] * 1e-3, a[1] * 1e-3, b[0] * 1e-3, b[1] * 1e-3)
        .2
}

fn pos(s: &Sketch, p: PointId) -> [f64; 2] {
    s.point(p).unwrap().pos()
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) <= TOL
}

fn line_ends(s: &Sketch, id: EntityId) -> ([f64; 2], [f64; 2]) {
    match s.entity(id) {
        Some(Entity::Line { start, end }) => (pos(s, *start), pos(s, *end)),
        other => panic!("{id} is not a line: {other:?}"),
    }
}

/// An arc's centre, start and end positions.
fn arc_ends(s: &Sketch, id: EntityId) -> ([f64; 2], [f64; 2], [f64; 2]) {
    match s.entity(id) {
        Some(Entity::Arc { center, start, end }) => (pos(s, *center), pos(s, *start), pos(s, *end)),
        other => panic!("{id} is not an arc: {other:?}"),
    }
}

fn holds(s: &Sketch, point: PointId, on: EntityId) -> bool {
    s.constraints().values().any(|r| {
        r.constraint == Constraint::PointOnLine { point, line: on }
            || r.constraint == Constraint::PointOnCircle { point, circle: on }
    })
}

/// A trim must leave a sketch the solver already agrees with: every new end
/// sits on its cutter, so nothing moves.
fn assert_solve_moves_nothing(s: &mut Draft) {
    let before = s.clone();
    assert!(s.solve().converged);
    for (id, p) in before.points() {
        assert!(near(p.pos(), pos(s, *id)), "{id} moved");
    }
}

/// `-10 → 10` along x, cut by a vertical line at each of `xs`.
fn crossed(xs: &[f64]) -> (Draft, EntityId, Vec<EntityId>) {
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [-10.0, 0.0], [10.0, 0.0]);
    let cutters = xs
        .iter()
        .map(|&x| line(&mut s, [x, -5.0], [x, 5.0]))
        .collect();
    (s, l, cutters)
}

fn trim(s: &mut Draft, id: EntityId, at: [f64; 2]) -> TrimEdit {
    modify::trim(s, id, mm(at[0], at[1])).unwrap()
}

#[test]
fn a_line_crossed_once_loses_the_picked_end() {
    for at in [[5.0, 0.0], [-5.0, 0.0]] {
        let (mut s, l, cutters) = crossed(&[0.0]);
        let points_before = s.points().len();
        let edit = trim(&mut s, l, at);

        assert!(!edit.deleted && edit.split_off.is_none());
        let (a, b) = line_ends(&s, l);
        // The id stays on what is left, with its direction.
        if at[0] > 0.0 {
            assert!(near(a, mm(-10.0, 0.0)) && near(b, mm(0.0, 0.0)));
        } else {
            assert!(near(a, mm(0.0, 0.0)) && near(b, mm(10.0, 0.0)));
        }
        assert_eq!(edit.ends.len(), 1);
        assert!(holds(&s, edit.ends[0], cutters[0]), "held on its cutter");
        assert_eq!(edit.held.len(), 1);
        // One end made, one lost.
        assert_eq!(edit.removed_points.len(), 1);
        assert_eq!(s.points().len(), points_before);
        assert_solve_moves_nothing(&mut s);
    }
}

#[test]
fn a_line_crossed_twice_splits_or_shortens() {
    // The middle piece: the start keeps the id, the far piece is new.
    let (mut s, l, cutters) = crossed(&[-3.0, 3.0]);
    let before = s.sketch.clone();
    let edit = trim(&mut s, l, [0.5, 0.0]);
    let far = edit.split_off.expect("a middle trim splits");
    assert!(!before.contains_id(far), "the far piece has a fresh id");
    let (a, b) = line_ends(&s, l);
    assert!(near(a, mm(-10.0, 0.0)) && near(b, mm(-3.0, 0.0)));
    let (c, d) = line_ends(&s, far);
    assert!(near(c, mm(3.0, 0.0)) && near(d, mm(10.0, 0.0)));
    assert!(
        edit.removed_points.is_empty(),
        "the old end moved to the far piece"
    );
    assert!(holds(&s, edit.ends[0], cutters[0]));
    assert!(holds(&s, edit.ends[1], cutters[1]));
    assert_solve_moves_nothing(&mut s);

    // An end piece of a line crossed twice only shortens it.
    let (mut s, l, cutters) = crossed(&[-3.0, 3.0]);
    let edit = trim(&mut s, l, [-8.0, 0.1]);
    assert!(edit.split_off.is_none());
    let (a, b) = line_ends(&s, l);
    assert!(near(a, mm(-3.0, 0.0)) && near(b, mm(10.0, 0.0)));
    assert!(holds(&s, edit.ends[0], cutters[0]));
}

#[test]
fn a_circle_crossed_twice_becomes_an_arc_with_the_same_id() {
    let mut s = Draft::seeded(1);
    let (center, circle) = s.add_circle(0.0, 0.0, 0.005);
    let cutter = line(&mut s, [-10.0, 0.0], [10.0, 0.0]);

    let edit = trim(&mut s, circle, [0.0, 5.0]);

    assert!(!edit.deleted);
    let (c, a, b) = arc_ends(&s, circle);
    assert!(near(c, [0.0, 0.0]));
    // The lower half is left: CCW from (-5, 0) through (0, -5) to (5, 0).
    assert!(
        near(a, mm(-5.0, 0.0)) && near(b, mm(5.0, 0.0)),
        "{a:?} → {b:?}"
    );
    assert!(matches!(s.entity(circle), Some(Entity::Arc { center: k, .. }) if *k == center));
    assert_eq!(edit.ends.len(), 2);
    for end in &edit.ends {
        assert!(holds(&s, *end, cutter));
    }
    assert!(edit.removed_points.is_empty(), "the centre stays");
    assert_solve_moves_nothing(&mut s);
}

#[test]
fn an_arc_is_trimmed_like_a_line_and_its_pieces_share_a_centre() {
    // The upper half circle of radius 5, CCW from (5, 0) to (-5, 0).
    let upper = |s: &mut Draft| s.add_arc(0.0, 0.0, 0.005, 0.0, -0.005, 0.0);

    // One crossing at the top: the first piece goes, the start moves there.
    let mut s = Draft::seeded(1);
    let (_, _, _, arc) = upper(&mut s);
    let cutter = line(&mut s, [0.0, -1.0], [0.0, 10.0]);
    let edit = trim(&mut s, arc, [3.0, 4.0]);
    let (c, a, b) = arc_ends(&s, arc);
    assert!(near(c, [0.0, 0.0]) && near(a, mm(0.0, 5.0)) && near(b, mm(-5.0, 0.0)));
    assert!(holds(&s, edit.ends[0], cutter));
    assert_solve_moves_nothing(&mut s);

    // Two crossings, the middle piece: two arcs about one centre point.
    let mut s = Draft::seeded(1);
    let (center, _, _, arc) = upper(&mut s);
    line(&mut s, [-2.0, 0.0], [-2.0, 10.0]);
    line(&mut s, [2.0, 0.0], [2.0, 10.0]);
    let edit = trim(&mut s, arc, [0.0, 5.0]);
    let far = edit.split_off.unwrap();
    let r = |x: f64| (25.0 - x * x).sqrt();
    let (_, a, b) = arc_ends(&s, arc);
    assert!(near(a, mm(5.0, 0.0)) && near(b, mm(2.0, r(2.0))));
    let (_, a, b) = arc_ends(&s, far);
    assert!(near(a, mm(-2.0, r(2.0))) && near(b, mm(-5.0, 0.0)));
    assert!(matches!(s.entity(far), Some(Entity::Arc { center: k, .. }) if *k == center));
    assert_solve_moves_nothing(&mut s);
}

#[test]
fn a_curve_with_no_crossing_is_deleted_whole() {
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    let (_, circle) = s.add_circle(0.0, 0.03, 0.005);
    let spare = s.add_point(Point::new(0.1, 0.1));

    let edit = trim(&mut s, l, [5.0, 0.0]);
    assert!(edit.deleted && edit.ends.is_empty());
    assert!(s.entity(l).is_none());
    assert_eq!(edit.removed_points.len(), 2);

    let edit = trim(&mut s, circle, [0.0, 35.0]);
    assert!(edit.deleted && s.entity(circle).is_none());
    assert_eq!(edit.removed_points.len(), 1, "its centre");
    assert_eq!(s.points().keys().copied().collect::<Vec<_>>(), vec![spare]);
}

#[test]
fn new_ends_are_held_on_a_circle_or_share_a_cutters_end() {
    // A line leaving a circle: the outside piece goes, the new end is held
    // on the rim.
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, 0.005);
    let l = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    let edit = trim(&mut s, l, [8.0, 0.0]);
    let (_, b) = line_ends(&s, l);
    assert!(near(b, mm(5.0, 0.0)));
    assert!(holds(&s, edit.ends[0], circle));
    assert_solve_moves_nothing(&mut s);

    // A T: the stem's end rests on the bar. Trimming the bar's right half
    // ends the bar *on the stem's own point*, so the two share it and no
    // hold is needed.
    let mut s = Draft::seeded(1);
    let bar = line(&mut s, [-10.0, 0.0], [10.0, 0.0]);
    let (foot, _, _) = s.add_line(0.0, 0.0, 0.0, 0.005);
    let edit = trim(&mut s, bar, [5.0, 0.0]);
    assert_eq!(edit.ends, vec![foot]);
    assert!(edit.held.is_empty());
    assert!(matches!(s.entity(bar), Some(Entity::Line { end, .. }) if *end == foot));
    assert_solve_moves_nothing(&mut s);
}

#[test]
fn construction_and_missing_entities_are_not_trimmed() {
    let (mut s, l, _) = crossed(&[0.0]);
    s.set_construction(l, true);
    let before = s.clone();
    assert_eq!(
        modify::trim(&mut s, l, mm(5.0, 0.0)),
        Err(ModifyError::NotTrimmable)
    );
    assert_eq!(
        modify::trim(&mut s, EntityId::from(Id(9999)), mm(5.0, 0.0)),
        Err(ModifyError::NotTrimmable)
    );
    assert_eq!(s, before);
}
