//! `modify::mirror`: which points are copied and which shared, the symmetry rows that tie the
//! copy, and what that leaves: no new degree of freedom, a solve that moves
//! nothing, a copy that follows its original, and twice the area.

use arrix_sketch::Draft;
use arrix_sketch::modify::{self, MirrorAxis, MirrorEdit, ModifyError};
use arrix_sketch::{
    Constraint, DatumEntity, Diagnostics, Entity, EntityId, Point, PointId, Sketch, SketchStatus,
    find_profiles,
};

const TOL: f64 = 1e-9;
const AXIS_Y: MirrorAxis = MirrorAxis::Datum(DatumEntity::AxisY);

fn pos(s: &Sketch, p: PointId) -> [f64; 2] {
    s.point(p).unwrap().pos()
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn near(a: [f64; 2], b: [f64; 2]) -> bool {
    dist(a, b) <= TOL
}

fn pt(s: &mut Draft, x: f64, y: f64) -> PointId {
    s.add_point(Point::new(x, y))
}

fn line(s: &mut Draft, a: PointId, b: PointId) -> EntityId {
    s.add_entity(Entity::Line { start: a, end: b })
}

fn flip_x(p: [f64; 2]) -> [f64; 2] {
    [-p[0], p[1]]
}

struct Outcome {
    dof: i32,
    redundant: usize,
    conflicting: usize,
    status: SketchStatus,
    moved: f64,
}

fn solve(s: &mut Draft) -> Outcome {
    let before: Vec<(PointId, [f64; 2])> = s.points().iter().map(|(i, p)| (*i, p.pos())).collect();
    let (r, d) = Diagnostics::evaluate(s);
    assert!(r.converged, "{}", d.message);
    Outcome {
        dof: d.dof,
        redundant: d.redundant.len(),
        conflicting: d.conflicting.len(),
        status: d.status,
        moved: before
            .iter()
            .map(|(id, p)| dist(*p, pos(s, *id)))
            .fold(0.0, f64::max),
    }
}

/// Mirrors `selection` and checks what every mirror promises: the original
/// untouched, no new freedom, no redundant or conflicting row, and a solve
/// that moves nothing.
fn assert_mirror_holds(s: &mut Draft, selection: &[EntityId], axis: MirrorAxis) -> MirrorEdit {
    let before = solve(s);
    assert_eq!(
        (before.redundant, before.conflicting),
        (0, 0),
        "the original"
    );
    let original = s.clone();
    let edit = modify::mirror(s, selection, axis).unwrap();
    for (id, p) in original.points() {
        assert_eq!(s.point(*id), Some(p), "an original point moved");
    }
    for (id, e) in original.entities() {
        assert_eq!(s.entity(*id), Some(e), "an original entity changed");
    }
    let after = solve(s);
    assert!(after.dof <= before.dof, "{} > {}", after.dof, before.dof);
    assert_eq!(
        (after.redundant, after.conflicting),
        (0, 0),
        "{:?}",
        edit.dropped
    );
    assert!(after.moved <= TOL, "a solve moves nothing: {}", after.moved);
    edit
}

fn set_value(s: &mut Draft, of: impl Fn(&Constraint) -> bool, to: f64) {
    let id = *s
        .constraints()
        .iter()
        .find(|(_, r)| of(&r.constraint))
        .map(|(id, _)| id)
        .unwrap();
    match s.get_constraint_mut(id).unwrap() {
        Constraint::Distance { value, .. } | Constraint::Diameter { value, .. } => *value = to,
        other => panic!("no value on {other:?}"),
    }
}

fn copy_of(edit: &MirrorEdit, p: PointId) -> PointId {
    edit.points
        .iter()
        .find(|(o, _)| *o == p)
        .map(|(_, c)| *c)
        .unwrap_or_else(|| panic!("{p} was not copied"))
}

/// A 30 mm line off the Y axis, fixed at one end, horizontal and dimensioned.
#[test]
fn a_line_is_copied_across_a_datum_axis_and_follows_its_original() {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.010, 0.005));
    let b = pt(&mut s, 0.040, 0.005);
    let l = line(&mut s, a, b);
    s.add_constraint(Constraint::Horizontal { line: l });
    s.add_constraint(Constraint::Distance { a, b, value: 0.030 });

    let edit = assert_mirror_holds(&mut s, &[l], AXIS_Y);
    assert_eq!(edit.pairs.len(), 1);
    assert_eq!(edit.points.len(), 2);
    assert!(edit.on_axis.is_empty());
    assert_eq!(edit.constraints.len(), 2, "one symmetry row per point");
    for id in &edit.constraints {
        assert!(matches!(
            s.get_constraint(*id),
            Some(Constraint::SymmetricAcrossDatum {
                datum: DatumEntity::AxisY,
                ..
            })
        ));
    }
    let copy = edit.pairs[0].1;
    let Some(Entity::Line { start, end }) = s.entity(copy).cloned() else {
        panic!("the copy is a line");
    };
    assert!(near(pos(&s, start), [-0.010, 0.005]));
    assert!(near(pos(&s, end), [-0.040, 0.005]));
    assert_eq!(solve(&mut s).status, SketchStatus::FullyConstrained);

    // Moving the original moves the copy.
    set_value(&mut s, |c| matches!(c, Constraint::Distance { .. }), 0.050);
    solve(&mut s);
    assert!(near(pos(&s, end), [-0.060, 0.005]), "{:?}", pos(&s, end));
}

#[test]
fn a_circle_keeps_its_radius_by_an_equal_row() {
    let mut s = Draft::seeded(1);
    let c = s.add_point(Point::fixed(0.020, 0.010));
    let circle = s.add_entity(Entity::Circle {
        center: c,
        radius: 0.005,
    });
    s.add_constraint(Constraint::Diameter {
        target: circle,
        value: 0.010,
    });

    let edit = assert_mirror_holds(&mut s, &[circle], AXIS_Y);
    let copy = edit.pairs[0].1;
    let equal = edit
        .constraints
        .iter()
        .filter(|id| matches!(s.get_constraint(**id), Some(Constraint::Equal { .. })))
        .count();
    assert_eq!(equal, 1);
    assert_eq!(solve(&mut s).status, SketchStatus::FullyConstrained);

    set_value(&mut s, |c| matches!(c, Constraint::Diameter { .. }), 0.016);
    solve(&mut s);
    let Some(Entity::Circle { center, radius }) = s.entity(copy).cloned() else {
        panic!("the copy is a circle");
    };
    assert!((radius - 0.008).abs() <= TOL, "{radius}");
    assert!(near(pos(&s, center), [-0.020, 0.010]));
}

/// A quarter arc about a fixed centre, counter-clockwise from 0° to 90°.
#[test]
fn an_arc_swaps_its_ends_so_its_sense_and_sweep_survive() {
    let mut s = Draft::seeded(1);
    let c = s.add_point(Point::fixed(0.020, 0.010));
    let a = pt(&mut s, 0.030, 0.010);
    let b = pt(&mut s, 0.020, 0.020);
    let arc = s.add_entity(Entity::Arc {
        center: c,
        start: a,
        end: b,
    });
    s.add_constraint(Constraint::HorizontalPoints { a, b: c });
    s.add_constraint(Constraint::VerticalPoints { a: b, b: c });
    s.add_constraint(Constraint::Radius {
        target: arc,
        value: 0.010,
    });

    let edit = assert_mirror_holds(&mut s, &[arc], AXIS_Y);
    let Some(Entity::Arc { center, start, end }) = s.entity(edit.pairs[0].1).cloned() else {
        panic!("the copy is an arc");
    };
    // The copy's start is the mirror of the original's end.
    assert_eq!(start, copy_of(&edit, b));
    assert_eq!(end, copy_of(&edit, a));
    let angle = |p: PointId| {
        let (q, o) = (pos(&s, p), pos(&s, center));
        (q[1] - o[1]).atan2(q[0] - o[0])
    };
    let sweep = (angle(end) - angle(start)).rem_euclid(std::f64::consts::TAU);
    assert!(
        (sweep - std::f64::consts::FRAC_PI_2).abs() <= 1e-9,
        "{sweep}"
    );
    assert!(near(pos(&s, start), [-0.020, 0.020]));
    assert!(near(pos(&s, end), [-0.030, 0.010]));
    assert_eq!(solve(&mut s).status, SketchStatus::FullyConstrained);
}

#[test]
fn a_polyline_shares_its_mirrored_corners() {
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.010, 0.0), (0.030, 0.0), (0.030, 0.020), (0.050, 0.020)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let l: Vec<EntityId> = (0..3).map(|i| line(&mut s, p[i], p[i + 1])).collect();
    let edit = assert_mirror_holds(&mut s, &l, AXIS_Y);
    assert_eq!(edit.points.len(), 4, "each corner once");
    let ends = |e: EntityId| s.entity(e).and_then(|e| e.line_ends()).unwrap();
    for w in edit.pairs.windows(2) {
        assert_eq!(ends(w[0].1).1, ends(w[1].1).0, "copies share a corner");
    }
    for &(o, c) in &edit.points {
        assert!(near(pos(&s, c), flip_x(pos(&s, o))));
    }
}

/// Half a bracket: a U open toward the Y axis, both ends on it. `held` says
/// whether the top end is held on the axis by a row, or only by position.
fn bracket_half(held: bool) -> (Draft, [PointId; 4], Vec<EntityId>) {
    let mut s = Draft::seeded(1);
    let a = s.add_point(Point::fixed(0.0, 0.0));
    let b = pt(&mut s, 0.030, 0.0);
    let c = pt(&mut s, 0.030, 0.020);
    let d = pt(&mut s, 0.0, 0.020);
    let l = vec![line(&mut s, a, b), line(&mut s, b, c), line(&mut s, c, d)];
    s.add_constraint(Constraint::Horizontal { line: l[0] });
    s.add_constraint(Constraint::Vertical { line: l[1] });
    s.add_constraint(Constraint::Horizontal { line: l[2] });
    s.add_constraint(Constraint::Distance { a, b, value: 0.030 });
    s.add_constraint(Constraint::Distance {
        a: b,
        b: c,
        value: 0.020,
    });
    if held {
        s.add_constraint(Constraint::PointOnDatum {
            point: d,
            datum: DatumEntity::AxisY,
        });
    }
    (s, [a, b, c, d], l)
}

#[test]
fn geometry_touching_the_axis_shares_its_point_and_holds_it_there() {
    // Held by a row already, or fixed: shared, and nothing new holds it.
    let (mut s, [a, _, _, d], l) = bracket_half(true);
    let edit = assert_mirror_holds(&mut s, &l, AXIS_Y);
    assert_eq!(edit.on_axis, [a, d]);
    assert_eq!(edit.points.len(), 2, "only the two off-axis corners");
    assert_eq!(edit.dropped.len(), 2, "{:?}", edit.dropped);
    assert_eq!(edit.constraints.len(), 2);
    assert_eq!(solve(&mut s).status, SketchStatus::FullyConstrained);

    // On the axis by position only: shared, and held there by a new row,
    // which takes the one freedom it had (sliding along the top line).
    let (mut s, [a, _, _, d], l) = bracket_half(false);
    assert_eq!(solve(&mut s).dof, 1);
    let edit = assert_mirror_holds(&mut s, &l, AXIS_Y);
    assert_eq!(edit.on_axis, [a, d]);
    let holds: Vec<&Constraint> = edit
        .constraints
        .iter()
        .filter_map(|id| s.get_constraint(*id))
        .filter(|c| matches!(c, Constraint::PointOnDatum { .. }))
        .collect();
    assert_eq!(
        holds,
        [&Constraint::PointOnDatum {
            point: d,
            datum: DatumEntity::AxisY
        }]
    );
    assert_eq!(solve(&mut s).status, SketchStatus::FullyConstrained);
}

#[test]
fn a_half_and_its_mirror_make_twice_the_area() {
    // Open to the axis: the mirror closes it into one region of twice the
    // area, and a dimension on the half moves both sides.
    let (mut s, _, l) = bracket_half(true);
    assert!(find_profiles(&s).is_empty(), "an open half has no region");
    assert_mirror_holds(&mut s, &l, AXIS_Y);
    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 1);
    let area = profiles[0].outer.signed_area;
    assert!((area - 2.0 * 0.030 * 0.020).abs() <= 1e-12, "{area}");
    set_value(
        &mut s,
        |c| matches!(c, Constraint::Distance { value, .. } if *value == 0.030),
        0.040,
    );
    solve(&mut s);
    let area = find_profiles(&s)[0].outer.signed_area;
    assert!((area - 2.0 * 0.040 * 0.020).abs() <= 1e-12, "{area}");

    // Closed and clear of the axis: two regions, each the half's.
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.010, 0.0), (0.030, 0.0), (0.030, 0.020), (0.010, 0.020)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let l: Vec<EntityId> = (0..4).map(|i| line(&mut s, p[i], p[(i + 1) % 4])).collect();
    let half: f64 = find_profiles(&s).iter().map(|p| p.outer.signed_area).sum();
    assert_mirror_holds(&mut s, &l, AXIS_Y);
    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 2);
    let whole: f64 = profiles.iter().map(|p| p.outer.signed_area).sum();
    assert!((whole - 2.0 * half).abs() <= 1e-12, "{whole} vs {half}");
}

#[test]
fn a_line_axis_in_the_selection_is_dropped_from_it() {
    let mut s = Draft::seeded(1);
    // A slanted construction axis, both ends fixed: y = x - 0.010.
    let (u, v) = (
        s.add_point(Point::fixed(0.0, -0.010)),
        s.add_point(Point::fixed(0.020, 0.010)),
    );
    let axis = line(&mut s, u, v);
    s.set_construction(axis, true);
    let a = s.add_point(Point::fixed(0.0, 0.010));
    let b = pt(&mut s, 0.0, 0.030);
    let l = line(&mut s, a, b);
    s.add_constraint(Constraint::Vertical { line: l });
    s.add_constraint(Constraint::Distance { a, b, value: 0.020 });

    let edit = assert_mirror_holds(&mut s, &[axis, l], MirrorAxis::Line(axis));
    assert_eq!(edit.skipped, [axis]);
    assert_eq!(edit.pairs.len(), 1);
    assert!(
        edit.constraints
            .iter()
            .all(|id| matches!(s.get_constraint(*id), Some(Constraint::Symmetric { .. })))
    );
    // Reflecting across y = x - 0.010 sends (x, y) to (y + 0.010, x - 0.010).
    for &(o, c) in &edit.points {
        let p = pos(&s, o);
        assert!(near(pos(&s, c), [p[1] + 0.010, p[0] - 0.010]));
    }
    assert!(!s.is_construction(edit.pairs[0].1), "the copy is drawn");
    assert_eq!(solve(&mut s).status, SketchStatus::FullyConstrained);
}

#[test]
fn a_construction_curve_is_copied_as_construction() {
    let mut s = Draft::seeded(1);
    let (_, _, l) = s.add_line(0.010, 0.0, 0.020, 0.010);
    s.set_construction(l, true);
    let edit = modify::mirror(&mut s, &[l], AXIS_Y).unwrap();
    assert!(s.is_construction(edit.pairs[0].1));
}

#[test]
fn nothing_to_mirror_or_no_axis_leaves_the_sketch_untouched() {
    let mut s = Draft::seeded(1);
    let (_, _, l) = s.add_line(0.010, 0.0, 0.020, 0.010);
    let (_, _, on_axis) = s.add_line(0.0, 0.0, 0.0, 0.010);
    let (_, circle) = s.add_circle(0.0, 0.020, 0.005);
    let before = s.clone();

    assert_eq!(
        modify::mirror(&mut s, &[], AXIS_Y),
        Err(ModifyError::NothingToMirror)
    );
    // A line and a circle on the axis are their own mirror.
    assert_eq!(
        modify::mirror(&mut s, &[on_axis, circle], AXIS_Y),
        Err(ModifyError::NothingToMirror)
    );
    // The axis alone is nothing either.
    assert_eq!(
        modify::mirror(&mut s, &[l], MirrorAxis::Line(l)),
        Err(ModifyError::NothingToMirror)
    );
    assert_eq!(
        modify::mirror(&mut s, &[l], MirrorAxis::Datum(DatumEntity::Origin)),
        Err(ModifyError::NoAxis)
    );
    assert_eq!(
        modify::mirror(&mut s, &[l], MirrorAxis::Line(circle)),
        Err(ModifyError::NoAxis)
    );
    assert_eq!(s, before);
}
