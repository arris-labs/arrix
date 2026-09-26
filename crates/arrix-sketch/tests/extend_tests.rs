//! `modify::extend` and `modify::break_at`: an end runs on to the nearest crossing
//! along a line's ray or round an arc's circle and is held there; with
//! nothing to reach the sketch is untouched; a break splits a curve at a
//! crossing, keeping both pieces joined and the constraints carried by the
//! transfer table.

use arrix_sketch::Draft;
use arrix_sketch::modify::{self, ModifyError, TrimEdit};
use arrix_sketch::{Constraint, Diagnostics, Entity, EntityId, Point, PointId, Sketch};

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

fn ends(s: &Sketch, id: EntityId) -> (PointId, PointId) {
    match s.entity(id) {
        Some(Entity::Line { start, end } | Entity::Arc { start, end, .. }) => (*start, *end),
        other => panic!("{id} has no ends: {other:?}"),
    }
}

fn end_pos(s: &Sketch, id: EntityId) -> ([f64; 2], [f64; 2]) {
    let (a, b) = ends(s, id);
    (pos(s, a), pos(s, b))
}

fn holds(s: &Sketch, point: PointId, on: EntityId) -> bool {
    s.constraints().values().any(|r| {
        r.constraint == Constraint::PointOnLine { point, line: on }
            || r.constraint == Constraint::PointOnCircle { point, circle: on }
    })
}

fn assert_solve_moves_nothing(s: &mut Draft) {
    let before = s.clone();
    assert!(s.solve().converged);
    for (id, p) in before.points() {
        assert!(near(p.pos(), pos(s, *id)), "{id} moved");
    }
}

fn dof(s: &mut Draft) -> i32 {
    let (r, d) = Diagnostics::evaluate(s);
    assert!(r.converged, "{}", d.message);
    d.dof
}

fn extend(s: &mut Draft, id: EntityId, at: [f64; 2]) -> TrimEdit {
    modify::extend(s, id, mm(at[0], at[1])).unwrap()
}

fn break_at(s: &mut Draft, id: EntityId, at: [f64; 2]) -> TrimEdit {
    modify::break_at(s, id, mm(at[0], at[1])).unwrap()
}

#[test]
fn a_line_extends_to_the_nearest_crossing_along_its_ray() {
    // `0 → 5` along x; walls at x = 8 and x = 12 ahead, x = -3 behind.
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [0.0, 0.0], [5.0, 0.0]);
    let near_wall = line(&mut s, [8.0, -5.0], [8.0, 5.0]);
    line(&mut s, [12.0, -5.0], [12.0, 5.0]);
    let behind = line(&mut s, [-3.0, -5.0], [-3.0, 5.0]);
    let (start, old_end) = ends(&s, l);

    let edit = extend(&mut s, l, [4.0, 0.0]);
    assert!(!edit.deleted && edit.split_off.is_none());
    let (a, b) = end_pos(&s, l);
    assert!(near(a, [0.0, 0.0]) && near(b, mm(8.0, 0.0)), "{b:?}");
    assert_eq!(ends(&s, l).0, start, "the far end is untouched");
    assert!(holds(&s, edit.ends[0], near_wall), "held on the wall");
    assert_eq!(edit.removed_points, vec![old_end], "the old end went");
    assert!(s.point(old_end).is_none());
    assert_solve_moves_nothing(&mut s);

    // The start end runs back along the ray the other way.
    let edit = extend(&mut s, l, [1.0, 0.0]);
    let (a, b) = end_pos(&s, l);
    assert!(near(a, mm(-3.0, 0.0)) && near(b, mm(8.0, 0.0)));
    assert!(holds(&s, edit.ends[0], behind));
    assert_solve_moves_nothing(&mut s);
}

#[test]
fn a_line_extends_onto_a_circle_or_a_cutters_own_end() {
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [0.0, 0.0], [5.0, 0.0]);
    let (_, circle) = s.add_circle(0.020, 0.0, 0.004);
    let edit = extend(&mut s, l, [5.0, 0.0]);
    let (_, b) = end_pos(&s, l);
    assert!(near(b, mm(16.0, 0.0)), "the near side of the rim: {b:?}");
    assert!(holds(&s, edit.ends[0], circle));
    assert_solve_moves_nothing(&mut s);

    // A stub whose end lies on the ray: the extended end *is* that end, so
    // the two share it and nothing needs holding.
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [0.0, 0.0], [5.0, 0.0]);
    let (_, foot, _) = s.add_line(0.008, 0.005, 0.008, 0.0);
    let edit = extend(&mut s, l, [5.0, 0.0]);
    assert_eq!(edit.ends, vec![foot]);
    assert!(edit.held.is_empty());
    assert_eq!(ends(&s, l).1, foot);
    assert_solve_moves_nothing(&mut s);
}

#[test]
fn a_shared_end_leaves_its_neighbour_where_it_was() {
    // An L corner: extending the bottom leg past the corner to a wall must
    // not drag the upright's foot with it.
    let mut s = Draft::seeded(1);
    let bottom_id = line(&mut s, [0.0, 0.0], [5.0, 0.0]);
    let corner = ends(&s, bottom_id).1;
    let top = s.add_point(Point::new(0.005, 0.005));
    let upright = s.add_entity(Entity::Line {
        start: corner,
        end: top,
    });
    let wall = line(&mut s, [8.0, -5.0], [8.0, 5.0]);

    let edit = extend(&mut s, bottom_id, [5.0, 0.0]);
    assert!(edit.removed_points.is_empty(), "the corner is still used");
    assert_eq!(ends(&s, upright).0, corner);
    assert!(near(pos(&s, corner), mm(5.0, 0.0)));
    assert!(near(end_pos(&s, bottom_id).1, mm(8.0, 0.0)));
    assert!(holds(&s, edit.ends[0], wall));
    assert_solve_moves_nothing(&mut s);
}

#[test]
fn an_arc_extends_round_its_own_circle() {
    // The upper half circle of radius 5, CCW from (5, 0) to (-5, 0), and a
    // chord along y = -3 below it.
    let setup = || {
        let mut s = Draft::seeded(1);
        let (center, _, _, arc) = s.add_arc(0.0, 0.0, 0.005, 0.0, -0.005, 0.0);
        let chord = line(&mut s, [-10.0, -3.0], [10.0, -3.0]);
        (s, center, arc, chord)
    };

    // Its end runs on CCW, down the left side, to (-4, -3).
    let (mut s, center, arc, chord) = setup();
    let edit = extend(&mut s, arc, [-5.0, 0.5]);
    let (a, b) = end_pos(&s, arc);
    assert!(near(a, mm(5.0, 0.0)) && near(b, mm(-4.0, -3.0)), "{b:?}");
    assert!(matches!(s.entity(arc), Some(Entity::Arc { center: c, .. }) if *c == center));
    assert!(holds(&s, edit.ends[0], chord));
    assert_solve_moves_nothing(&mut s);

    // Its start runs back CW, down the right side, to (4, -3).
    let (mut s, _, arc, chord) = setup();
    let edit = extend(&mut s, arc, [5.0, 0.5]);
    let (a, b) = end_pos(&s, arc);
    assert!(near(a, mm(4.0, -3.0)) && near(b, mm(-5.0, 0.0)), "{a:?}");
    assert!(holds(&s, edit.ends[0], chord));
    assert_solve_moves_nothing(&mut s);
}

#[test]
fn nothing_to_reach_is_an_error_and_the_sketch_is_untouched() {
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [0.0, 0.0], [5.0, 0.0]);
    // Beside the ray, parallel to it, and behind the other end.
    line(&mut s, [8.0, 1.0], [8.0, 5.0]);
    line(&mut s, [0.0, 2.0], [20.0, 2.0]);
    let (_, arc_only) = {
        let (c, _, _, arc) = s.add_arc(0.0, 0.030, 0.005, 0.0, -0.005, 0.030);
        (c, arc)
    };
    let (_, circle) = s.add_circle(0.0, -0.030, 0.002);
    let before = s.clone();

    assert_eq!(
        modify::extend(&mut s, l, mm(5.0, 0.0)),
        Err(ModifyError::NoCrossing)
    );
    assert_eq!(
        modify::extend(&mut s, arc_only, mm(-5.0, 30.0)),
        Err(ModifyError::NoCrossing)
    );
    assert_eq!(
        modify::extend(&mut s, circle, mm(0.0, -28.0)),
        Err(ModifyError::Closed)
    );
    s.set_construction(l, true);
    let before_construction = s.clone();
    assert_eq!(
        modify::extend(&mut s, l, mm(5.0, 0.0)),
        Err(ModifyError::NotTrimmable)
    );
    assert_eq!(s, before_construction);
    s.set_construction(l, false);
    assert_eq!(s, before);
}

#[test]
fn an_extended_line_keeps_its_carrier_and_drops_its_length() {
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [0.0, 0.0], [5.0, 0.0]);
    let wall = line(&mut s, [8.0, -5.0], [8.0, 5.0]);
    let (a, b) = ends(&s, l);
    let horizontal = s.add_constraint(Constraint::Horizontal { line: l });
    let length = s.add_constraint(Constraint::Distance { a, b, value: 0.005 });

    let edit = extend(&mut s, l, [5.0, 0.0]);
    assert!(
        s.get_constraint(horizontal).is_some(),
        "the carrier is kept"
    );
    assert_eq!(
        edit.dropped,
        vec![length],
        "the length measured a moved end"
    );
    assert!(s.get_constraint(length).is_none());
    assert!(holds(&s, edit.ends[0], wall));
    assert_solve_moves_nothing(&mut s);
}

#[test]
fn a_break_keeps_both_pieces_joined_at_the_crossing() {
    // `-10 → 10` along x, crossed at x = -3 and x = 3; the break is at the
    // crossing nearest the pick.
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [-10.0, 0.0], [10.0, 0.0]);
    line(&mut s, [-3.0, -5.0], [-3.0, 5.0]);
    let cutter = line(&mut s, [3.0, -5.0], [3.0, 5.0]);
    let (start, end) = ends(&s, l);
    s.add_constraint(Constraint::Horizontal { line: l });
    let length = s.add_constraint(Constraint::Distance {
        a: start,
        b: end,
        value: 0.020,
    });
    let midpoint = s.add_point(Point::new(0.0, 0.0));
    let mid = s.add_constraint(Constraint::Midpoint {
        point: midpoint,
        line: l,
    });
    let dof_before = dof(&mut s);
    let before = s.sketch.clone();

    let edit = break_at(&mut s, l, [2.0, 0.5]);

    let far = edit.split_off.expect("a break splits");
    assert!(!before.contains_id(far), "the far piece has a fresh id");
    assert!(edit.removed_points.is_empty() && edit.dropped.is_empty());
    let joint = edit.ends[0];
    assert_eq!(ends(&s, l), (start, joint), "the start keeps the id");
    assert_eq!(ends(&s, far), (joint, end), "the pieces share the joint");
    assert!(near(pos(&s, joint), mm(3.0, 0.0)));
    assert!(holds(&s, joint, cutter));
    // The far piece is held on the kept piece's line at its free end; at the
    // joint it already is.
    assert_eq!(edit.joined.len(), 1);
    // The table: the length still measures its two surviving ends, and the
    // midpoint is respelled over them.
    assert!(s.get_constraint(length).is_some());
    assert_eq!(edit.retargeted, vec![mid]);
    assert_eq!(
        s.get_constraint(mid),
        Some(&Constraint::SymmetricPoints {
            a: start,
            b: end,
            center: midpoint
        })
    );
    assert_solve_moves_nothing(&mut s);
    // A new joint point (2), held on its cutter (−1), and the far piece's
    // join (−1): the count is where it was.
    assert_eq!(dof(&mut s), dof_before);
}

#[test]
fn an_arc_breaks_into_two_arcs_about_one_centre() {
    let mut s = Draft::seeded(1);
    let (center, _, _, arc) = s.add_arc(0.0, 0.0, 0.005, 0.0, -0.005, 0.0);
    let cutter = line(&mut s, [0.0, -1.0], [0.0, 10.0]);
    let (start, end) = ends(&s, arc);
    let dof_before = dof(&mut s);

    let edit = break_at(&mut s, arc, [1.0, 5.0]);

    let far = edit.split_off.unwrap();
    let joint = edit.ends[0];
    assert!(near(pos(&s, joint), mm(0.0, 5.0)));
    assert_eq!(ends(&s, arc), (start, joint));
    assert_eq!(ends(&s, far), (joint, end));
    assert!(matches!(s.entity(far), Some(Entity::Arc { center: c, .. }) if *c == center));
    assert!(holds(&s, joint, cutter));
    assert_solve_moves_nothing(&mut s);
    assert_eq!(dof(&mut s), dof_before);
}

#[test]
fn a_break_needs_a_crossing_on_an_open_curve() {
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [0.0, 0.0], [10.0, 0.0]);
    let (_, circle) = s.add_circle(0.0, 0.0, 0.003);
    line(&mut s, [20.0, -5.0], [20.0, 5.0]);
    let lone = line(&mut s, [0.0, 30.0], [10.0, 30.0]);
    let before = s.clone();

    assert_eq!(
        modify::break_at(&mut s, lone, mm(5.0, 30.0)),
        Err(ModifyError::NoCrossing)
    );
    assert_eq!(
        modify::break_at(&mut s, circle, mm(3.0, 0.0)),
        Err(ModifyError::Closed)
    );
    // The line starts at the circle's centre and crosses its rim at x = 3.
    assert_eq!(s, before);
    let edit = break_at(&mut s, l, [4.0, 0.0]);
    assert!(near(pos(&s, edit.ends[0]), mm(3.0, 0.0)));
    assert!(holds(&s, edit.ends[0], circle));
}

/// The hover previews come from the edits' own pieces and first hit: what
/// a trim removes, an extension adds and a break splits off, at a pick —
/// and a pick lands on the nearest drawn curve, never on construction.
#[test]
fn the_previews_show_what_the_edits_touch() {
    let mut s = Draft::seeded(1);
    let l = line(&mut s, [-10.0, 0.0], [5.0, 0.0]);
    line(&mut s, [0.0, -5.0], [0.0, 5.0]);
    line(&mut s, [8.0, -5.0], [8.0, 5.0]);
    let guide = line(&mut s, [-10.0, 1.0], [10.0, 1.0]);
    s.set_construction(guide, true);

    assert_eq!(modify::pick_curve(&s, mm(-5.0, 0.6), 1e-3), Some(l));
    assert_eq!(modify::pick_curve(&s, mm(-5.0, 20.0), 1e-3), None);

    let ends = |p: Vec<[f64; 2]>| (p[0], *p.last().unwrap());
    let (a, b) = ends(modify::trim_preview(&s, l, mm(3.0, 0.0)).unwrap());
    assert!(near(a, mm(0.0, 0.0)) && near(b, mm(5.0, 0.0)));
    let (a, b) = ends(modify::extend_preview(&s, l, mm(4.0, 0.0)).unwrap());
    assert!(near(a, mm(5.0, 0.0)) && near(b, mm(8.0, 0.0)));
    let (a, b) = ends(modify::break_preview(&s, l, mm(1.0, 0.0)).unwrap());
    assert!(near(a, mm(0.0, 0.0)) && near(b, mm(5.0, 0.0)));
    // The start runs back to nothing, and the edit agrees.
    assert!(modify::extend_preview(&s, l, mm(-9.0, 0.0)).is_none());
    assert_eq!(
        modify::extend(&mut s.clone(), l, mm(-9.0, 0.0)),
        Err(ModifyError::NoCrossing)
    );

    // An arc's extension back past its start runs CW to the chord.
    let mut s = Draft::seeded(1);
    let (_, _, _, arc) = s.add_arc(0.0, 0.0, 0.005, 0.0, -0.005, 0.0);
    line(&mut s, [-10.0, -3.0], [10.0, -3.0]);
    let p = modify::extend_preview(&s, arc, mm(5.0, 0.5)).unwrap();
    let (a, b) = ends(p);
    assert!(
        near(a, mm(4.0, -3.0)) && near(b, mm(5.0, 0.0)),
        "{a:?} {b:?}"
    );
}
