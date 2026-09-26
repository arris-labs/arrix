//! `modify::find_chain` and `modify::offset`: which curves make a chain,
//! where the copy's corners go, what fails soft, and the rows that tie the
//! copy to its original.

use arrix_core::Id;
use arrix_sketch::Draft;
use arrix_sketch::modify::{self, Chain, ModifyError, OffsetEdit};
use arrix_sketch::{Entity, EntityId, Point, PointId, Sketch};

const TOL: f64 = 1e-9;

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

/// A closed polyline through `corners`, one line per side, `lᵢ` from corner
/// `i` to `i+1`.
fn polygon(s: &mut Draft, corners: &[(f64, f64)]) -> (Vec<PointId>, Vec<EntityId>) {
    let p: Vec<PointId> = corners.iter().map(|&(x, y)| pt(s, x, y)).collect();
    let l = (0..p.len())
        .map(|i| line(s, p[i], p[(i + 1) % p.len()]))
        .collect();
    (p, l)
}

fn rectangle(s: &mut Draft) -> (Vec<PointId>, Vec<EntityId>) {
    polygon(s, &[(0.0, 0.0), (0.040, 0.0), (0.040, 0.030), (0.0, 0.030)])
}

fn ends(s: &Sketch, e: EntityId) -> (PointId, PointId) {
    match s.entity(e) {
        Some(Entity::Line { start, end }) | Some(Entity::Arc { start, end, .. }) => (*start, *end),
        other => panic!("{e} has no ends: {other:?}"),
    }
}

fn line_pos(s: &Sketch, e: EntityId) -> ([f64; 2], [f64; 2]) {
    let (a, b) = ends(s, e);
    (pos(s, a), pos(s, b))
}

/// `(centre, radius)` of a copy arc or circle, the radius read the way the
/// solver reads it.
fn round(s: &Sketch, e: EntityId) -> ([f64; 2], f64) {
    match s.entity(e) {
        Some(Entity::Circle { center, radius }) => (pos(s, *center), *radius),
        Some(Entity::Arc { center, start, .. }) => {
            let c = pos(s, *center);
            (c, dist(c, pos(s, *start)))
        }
        other => panic!("{e} is not round: {other:?}"),
    }
}

fn entities_of(chain: &Chain) -> Vec<EntityId> {
    chain.entities().collect()
}

// ---- chain discovery ------------------------------------------------------

#[test]
fn a_chain_stops_at_a_branch() {
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [
        (0.0, 0.0),
        (10.0, 0.0),
        (20.0, 0.0),
        (30.0, 0.0),
        (20.0, 10.0),
    ]
    .iter()
    .map(|&(x, y)| pt(&mut s, x, y))
    .collect();
    let a = line(&mut s, p[0], p[1]);
    let b = line(&mut s, p[1], p[2]);
    let c = line(&mut s, p[2], p[3]);
    let d = line(&mut s, p[2], p[4]);

    // Three curves meet at p2, so the chain from `a` ends there.
    let chain = modify::find_chain(&s, a).unwrap();
    assert_eq!(entities_of(&chain), [a, b]);
    assert!(!chain.closed);
    assert_eq!(chain.links[1].to, Some(p[2]));
    // Each branch is a chain of its own.
    assert_eq!(entities_of(&modify::find_chain(&s, c).unwrap()), [c]);
    assert_eq!(entities_of(&modify::find_chain(&s, d).unwrap()), [d]);
}

#[test]
fn a_chain_closes_a_loop_in_the_picked_curves_direction() {
    let mut s = Draft::seeded(1);
    let (p, l) = rectangle(&mut s);
    let chain = modify::find_chain(&s, l[2]).unwrap();
    assert!(chain.closed);
    assert_eq!(entities_of(&chain), [l[2], l[3], l[0], l[1]]);
    assert!(chain.links.iter().all(|k| !k.reversed));
    assert_eq!(chain.links[0].from, Some(p[2]));
    assert_eq!(chain.links[0].to, Some(p[3]));
    // Every link leaves where the last one arrived, all the way round.
    for i in 0..4 {
        assert_eq!(chain.links[i].to, chain.links[(i + 1) % 4].from);
    }
}

#[test]
fn a_curve_drawn_end_first_is_reversed() {
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let a = line(&mut s, p[0], p[1]);
    let b = line(&mut s, p[2], p[1]); // drawn against the loop
    let c = line(&mut s, p[2], p[3]);
    let d = line(&mut s, p[3], p[0]);
    let chain = modify::find_chain(&s, a).unwrap();
    assert!(chain.closed);
    assert_eq!(entities_of(&chain), [a, b, c, d]);
    assert_eq!(
        chain.links.iter().map(|k| k.reversed).collect::<Vec<_>>(),
        [false, true, false, false]
    );
    assert_eq!(chain.links[1].from, Some(p[1]));
    assert_eq!(chain.links[1].to, Some(p[2]));
}

#[test]
fn a_chain_picked_from_its_middle_runs_the_picked_curves_way() {
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (20.0, 10.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let a = line(&mut s, p[0], p[1]);
    let b = line(&mut s, p[1], p[2]);
    let c = line(&mut s, p[3], p[2]); // reversed against `b`
    let chain = modify::find_chain(&s, b).unwrap();
    assert!(!chain.closed);
    assert_eq!(entities_of(&chain), [a, b, c]);
    assert_eq!(chain.links[0].from, Some(p[0]));
    assert!(chain.links[2].reversed);
    assert_eq!(chain.links[2].to, Some(p[3]));
}

#[test]
fn construction_geometry_never_joins_a_chain() {
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.0, 0.0), (10.0, 0.0), (20.0, 0.0), (10.0, 10.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let a = line(&mut s, p[0], p[1]);
    let b = line(&mut s, p[1], p[2]);
    // A construction line leaves the joint too. It is not a link, and it
    // does not make the joint a branch.
    let c = line(&mut s, p[1], p[3]);
    s.set_construction(c, true);
    let chain = modify::find_chain(&s, a).unwrap();
    assert_eq!(entities_of(&chain), [a, b]);

    // A construction curve is a chain of construction curves only.
    let chain = modify::find_chain(&s, c).unwrap();
    assert_eq!(entities_of(&chain), [c]);

    // A point and a missing id are not chains.
    let dot = s.add_entity(Entity::Point(p[0]));
    assert_eq!(modify::find_chain(&s, dot), Err(ModifyError::NotAChain));
    assert_eq!(
        modify::find_chain(&s, EntityId::from(Id(9999))),
        Err(ModifyError::NotAChain)
    );
}

#[test]
fn a_circle_is_a_closed_chain_of_one() {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, 0.010);
    let chain = modify::find_chain(&s, circle).unwrap();
    assert!(chain.closed);
    assert_eq!(entities_of(&chain), [circle]);
    assert_eq!((chain.links[0].from, chain.links[0].to), (None, None));
}

// ---- lines ---------------------------------------------------------------

/// Everything the sketch held is still held, unchanged; and everything the
/// edit added has an id nothing held before.
fn assert_original_untouched(before: &Sketch, after: &Sketch) {
    for (id, p) in before.points() {
        assert_eq!(after.point(*id), Some(p), "{id} changed");
    }
    for (id, e) in before.entities() {
        assert_eq!(after.entity(*id), Some(e), "{id} changed");
    }
    // The copy's rows are added; none of the original's changes.
    for (id, c) in before.constraints() {
        assert_eq!(after.constraints().get(id), Some(c), "{id} changed");
    }
    assert!(before.construction().is_subset(after.construction()));
}

fn assert_fresh(before: &Sketch, edit: &OffsetEdit) {
    for p in edit
        .corners
        .iter()
        .chain(edit.centers.iter().map(|(_, p)| p))
    {
        assert!(before.point(*p).is_none(), "{p} is not fresh");
    }
    for (orig, copy) in &edit.pairs {
        assert!(before.entity(*orig).is_some());
        assert!(before.entity(*copy).is_none(), "{copy} is not fresh");
    }
}

#[test]
fn a_closed_rectangle_offsets_outward_for_a_positive_distance() {
    for clockwise in [false, true] {
        let mut s = Draft::seeded(1);
        let (_, l) = if clockwise {
            polygon(
                &mut s,
                &[(0.0, 0.0), (0.0, 0.030), (0.040, 0.030), (0.040, 0.0)],
            )
        } else {
            rectangle(&mut s)
        };
        let before = s.clone();
        let edit = modify::offset(&mut s, l[0], 0.003, None).unwrap();
        assert_original_untouched(&before, &s);
        assert_fresh(&before, &edit);
        assert!(edit.chain.closed);
        // The four corners are new points, each shared by two copies.
        assert_eq!(edit.corners.len(), 4);
        assert_eq!(s.points().len(), before.points().len() + 4);
        assert_eq!(s.entities().len(), before.entities().len() + 4);
        let corners: Vec<[f64; 2]> = edit.corners.iter().map(|c| pos(&s, *c)).collect();
        for want in [
            (-0.003, -0.003),
            (-0.003, 0.033),
            (0.043, -0.003),
            (0.043, 0.033),
        ] {
            assert!(
                corners.iter().any(|c| near(*c, [want.0, want.1])),
                "no corner at {want:?} in {corners:?}"
            );
        }
        // Each copy line runs the way its original does.
        for (orig, copy) in &edit.pairs {
            let (a, b) = line_pos(&s, *orig);
            let (a2, b2) = line_pos(&s, *copy);
            let d = [b[0] - a[0], b[1] - a[1]];
            let d2 = [b2[0] - a2[0], b2[1] - a2[1]];
            assert!(d[0] * d2[0] + d[1] * d2[1] > 0.0);
        }
    }
}

#[test]
fn a_negative_distance_goes_inward() {
    let mut s = Draft::seeded(1);
    let (_, l) = rectangle(&mut s);
    let edit = modify::offset(&mut s, l[0], -0.003, None).unwrap();
    let inner: Vec<[f64; 2]> = edit.corners.iter().map(|c| pos(&s, *c)).collect();
    let want = [
        [0.003, 0.003],
        [0.037, 0.003],
        [0.037, 0.027],
        [0.003, 0.027],
    ];
    for (got, want) in inner.iter().zip(want) {
        assert!(near(*got, want), "{got:?} vs {want:?}");
    }
}

#[test]
fn an_open_chain_offsets_to_the_left_and_pushes_its_free_ends_perpendicular() {
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.0, 0.0), (0.040, 0.0), (0.040, 0.030)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let a = line(&mut s, p[0], p[1]);
    let b = line(&mut s, p[1], p[2]);

    let mut left = s.clone();
    let edit = modify::offset(&mut left, a, 0.003, None).unwrap();
    assert!(!edit.chain.closed);
    assert_eq!(edit.corners.len(), 3, "free start, corner, free end");
    let got: Vec<[f64; 2]> = edit.corners.iter().map(|c| pos(&left, *c)).collect();
    let want = [[0.0, 0.003], [0.037, 0.003], [0.037, 0.030]];
    for (got, want) in got.iter().zip(want) {
        assert!(near(*got, want), "{got:?} vs {want:?}");
    }
    assert_eq!(edit.pairs.len(), 2);
    let (a2, b2) = (edit.pairs[0].1, edit.pairs[1].1);
    let (_, end_a) = ends(&left, a2);
    let (start_b, _) = ends(&left, b2);
    assert_eq!(end_a, start_b, "the copies share their corner");

    // The other side.
    let mut right = s.clone();
    let edit = modify::offset(&mut right, b, -0.003, None).unwrap();
    let got: Vec<[f64; 2]> = edit.corners.iter().map(|c| pos(&right, *c)).collect();
    let want = [[0.0, -0.003], [0.043, -0.003], [0.043, 0.030]];
    for (got, want) in got.iter().zip(want) {
        assert!(near(*got, want), "{got:?} vs {want:?}");
    }
}

// ---- arcs ----------------------------------------------------------------

/// A line along +x from the origin to (30, 0), then a counter-clockwise arc
/// about (30, 10), radius 10, that starts where the line ends — a tangent
/// joint — and sweeps 60° to `(30 + 10 cos 30°, 10 − 10 sin 30°)`.
fn line_then_tangent_arc(s: &mut Draft) -> (EntityId, EntityId, [PointId; 4]) {
    let p0 = pt(s, 0.0, 0.0);
    let p1 = pt(s, 0.030, 0.0);
    let c = pt(s, 0.030, 0.010);
    let p2 = pt(s, 0.030 + 0.010 * 30f64.to_radians().cos(), 0.005);
    let l = line(s, p0, p1);
    let a = s.add_entity(Entity::Arc {
        center: c,
        start: p1,
        end: p2,
    });
    (l, a, [p0, p1, c, p2])
}

#[test]
fn a_tangent_joint_stays_a_tangent_joint() {
    let mut s = Draft::seeded(1);
    let (l, a, _) = line_then_tangent_arc(&mut s);
    let before = s.clone();
    // Left of the chain is toward the arc's centre.
    let edit = modify::offset(&mut s, l, 0.003, None).unwrap();
    assert_original_untouched(&before, &s);
    assert_fresh(&before, &edit);
    let (l2, a2) = (edit.pairs[0].1, edit.pairs[1].1);
    assert_eq!(edit.pairs[1].0, a);

    let (c, r) = round(&s, a2);
    assert!(near(c, [0.030, 0.010]), "the arc keeps its centre");
    assert!((r - 0.007).abs() <= TOL, "radius {r}");
    assert_eq!(edit.centers.len(), 1);
    let (_, end_l) = line_pos(&s, l2);
    let (start_a, _) = ends(&s, a2);
    assert_eq!(ends(&s, l2).1, start_a, "one shared point at the joint");
    assert!(near(end_l, [0.030, 0.003]));
    // Tangent: the radius to the joint is perpendicular to the line.
    assert!((end_l[0] - c[0]).abs() <= TOL);
    // The free start is pushed straight up; the arc's free end runs radially.
    assert!(near(pos(&s, edit.corners[0]), [0.0, 0.003]));
    let end = pos(&s, *edit.corners.last().unwrap());
    let want = [
        0.030 + 0.007 * 30f64.to_radians().cos(),
        0.010 - 0.007 * 30f64.to_radians().sin(),
    ];
    assert!(near(end, want), "{end:?} vs {want:?}");
}

#[test]
fn the_other_side_of_an_arc_grows_it() {
    let mut s = Draft::seeded(1);
    let (l, _, _) = line_then_tangent_arc(&mut s);
    let edit = modify::offset(&mut s, l, -0.003, None).unwrap();
    let (c, r) = round(&s, edit.pairs[1].1);
    assert!(near(c, [0.030, 0.010]));
    assert!((r - 0.013).abs() <= TOL, "radius {r}");
    let joint = pos(&s, ends(&s, edit.pairs[1].1).0);
    assert!(near(joint, [0.030, -0.003]));
}

#[test]
fn a_chain_walked_against_an_arcs_direction_offsets_the_same_curve() {
    // The same shape, picked from the arc: it is met against its sense
    // about the centre from the line's side, so which way is "left" flips.
    let mut s = Draft::seeded(1);
    let (l, a, _) = line_then_tangent_arc(&mut s);
    let from_line = {
        let mut t = s.clone();
        let e = modify::offset(&mut t, l, 0.003, None).unwrap();
        e.corners.iter().map(|c| pos(&t, *c)).collect::<Vec<_>>()
    };
    let from_arc = {
        let mut t = s.clone();
        let e = modify::offset(&mut t, a, 0.003, None).unwrap();
        assert_eq!(entities_of(&e.chain), [l, a]);
        e.corners.iter().map(|c| pos(&t, *c)).collect::<Vec<_>>()
    };
    assert_eq!(from_line.len(), from_arc.len());
    for (x, y) in from_line.iter().zip(&from_arc) {
        assert!(near(*x, *y));
    }
}

#[test]
fn a_line_to_arc_corner_takes_the_crossing_nearest_its_own_image() {
    // A line along +x to (20, 0), then a counter-clockwise arc about
    // (26, 8), radius 10, from (20, 0): the joint turns 36.87° right.
    let mut s = Draft::seeded(1);
    let p0 = pt(&mut s, 0.0, 0.0);
    let p1 = pt(&mut s, 0.020, 0.0);
    let c = pt(&mut s, 0.026, 0.008);
    let p2 = pt(&mut s, 0.034, 0.002);
    let l = line(&mut s, p0, p1);
    let a = s.add_entity(Entity::Arc {
        center: c,
        start: p1,
        end: p2,
    });
    for left in [0.002, -0.002] {
        let mut t = s.clone();
        let edit = modify::offset(&mut t, l, left, None).unwrap();
        let x = pos(&t, edit.corners[1]);
        // On the offset line and on the offset circle.
        assert!((x[1] - left).abs() <= TOL, "{x:?}");
        let r = 0.010 - left; // counter-clockwise: the left is inward
        assert!((dist(x, [0.026, 0.008]) - r).abs() <= TOL, "{x:?}");
        // And it is the crossing beside the corner, not the far one.
        assert!(dist(x, [0.020, 0.0]) < 0.006, "{x:?}");
        let _ = a;
    }
}

#[test]
fn an_arc_to_arc_corner_lies_on_both_offset_circles() {
    // Arc 1: about the origin, radius 10, from (10, 0) to (0, 10). Arc 2:
    // about (5, 15), from (0, 10) to (10, 10). Their joint at (0, 10) turns.
    let mut s = Draft::seeded(1);
    let c1 = pt(&mut s, 0.0, 0.0);
    let (a1, b1) = (pt(&mut s, 0.010, 0.0), pt(&mut s, 0.0, 0.010));
    let c2 = pt(&mut s, 0.005, 0.015);
    let b2 = pt(&mut s, 0.010, 0.010);
    let arc1 = s.add_entity(Entity::Arc {
        center: c1,
        start: a1,
        end: b1,
    });
    let arc2 = s.add_entity(Entity::Arc {
        center: c2,
        start: b1,
        end: b2,
    });
    let r2 = dist([0.005, 0.015], [0.0, 0.010]);

    // Right of the chain: both radii grow by 1 mm and the circles cross twice.
    let mut t = s.clone();
    let edit = modify::offset(&mut t, arc1, -0.001, None).unwrap();
    assert_eq!(entities_of(&edit.chain), [arc1, arc2]);
    let x = pos(&t, edit.corners[1]);
    assert!((dist(x, [0.0, 0.0]) - 0.011).abs() <= TOL, "{x:?}");
    assert!(
        (dist(x, [0.005, 0.015]) - (r2 + 0.001)).abs() <= TOL,
        "{x:?}"
    );
    assert!(dist(x, [0.0, 0.010]) < 0.005, "the nearer crossing: {x:?}");

    // Left: the offset circles no longer meet (15.8 > 9 + 6.07).
    let mut t = s.clone();
    assert_eq!(
        modify::offset(&mut t, arc1, 0.001, None),
        Err(ModifyError::TooLarge)
    );
}

// ---- circles -------------------------------------------------------------

#[test]
fn a_circle_takes_radius_r_plus_the_distance() {
    let mut s = Draft::seeded(1);
    let (c, circle) = s.add_circle(0.020, 0.015, 0.010);
    let before = s.clone();
    let edit = modify::offset(&mut s, circle, 0.003, None).unwrap();
    assert_original_untouched(&before, &s);
    assert_fresh(&before, &edit);
    assert!(edit.corners.is_empty());
    let copy = edit.pairs[0].1;
    let (centre, r) = round(&s, copy);
    assert!(near(centre, [0.020, 0.015]) && (r - 0.013).abs() <= TOL);
    assert_eq!(
        edit.centers,
        [(
            copy,
            match s.entity(copy) {
                Some(Entity::Circle { center, .. }) => *center,
                _ => unreachable!(),
            }
        )]
    );
    assert_ne!(edit.centers[0].1, c, "the copy's centre is its own point");

    let edit = modify::offset(&mut s, circle, -0.004, None).unwrap();
    assert!((round(&s, edit.pairs[0].1).1 - 0.006).abs() <= TOL);
}

// ---- construction ---------------------------------------------------------

#[test]
fn a_copy_is_construction_iff_its_original_is() {
    let mut s = Draft::seeded(1);
    let (_, l) = rectangle(&mut s);
    let edit = modify::offset(&mut s, l[0], 0.003, None).unwrap();
    assert!(edit.pairs.iter().all(|(_, c)| !s.is_construction(*c)));

    let mut s = Draft::seeded(1);
    let (_, l) = rectangle(&mut s);
    for e in &l {
        s.set_construction(*e, true);
    }
    let edit = modify::offset(&mut s, l[0], 0.003, None).unwrap();
    assert_eq!(edit.pairs.len(), 4);
    assert!(edit.pairs.iter().all(|(_, c)| s.is_construction(*c)));
}

// ---- failing soft --------------------------------------------------------

#[test]
fn a_distance_that_collapses_a_curve_is_too_large_and_writes_nothing() {
    // A 30 mm side, 16 mm in: the two long sides cross and the short ones
    // turn inside out.
    let mut s = Draft::seeded(1);
    let (_, l) = rectangle(&mut s);
    let before = s.clone();
    assert_eq!(
        modify::offset(&mut s, l[0], -0.016, None),
        Err(ModifyError::TooLarge)
    );
    assert_eq!(s, before);

    // An open chain whose first line would run backwards.
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.0, 0.0), (0.040, 0.0), (0.040, 0.030)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let a = line(&mut s, p[0], p[1]);
    line(&mut s, p[1], p[2]);
    let before = s.clone();
    assert_eq!(
        modify::offset(&mut s, a, 0.045, None),
        Err(ModifyError::TooLarge)
    );
    assert_eq!(s, before);
    // ...but 5 mm is fine.
    assert!(modify::offset(&mut s, a, 0.005, None).is_ok());
}

#[test]
fn an_arc_offset_past_its_centre_and_a_circle_past_nothing_are_too_large() {
    let mut s = Draft::seeded(1);
    let (l, _, _) = line_then_tangent_arc(&mut s);
    let before = s.clone();
    // Left is toward the centre, 10 mm away: 12 mm crosses it.
    assert_eq!(
        modify::offset(&mut s, l, 0.012, None),
        Err(ModifyError::TooLarge)
    );
    assert_eq!(s, before);

    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, 0.010);
    let before = s.clone();
    assert_eq!(
        modify::offset(&mut s, circle, -0.010, None),
        Err(ModifyError::TooLarge)
    );
    assert_eq!(
        modify::offset(&mut s, circle, -0.012, None),
        Err(ModifyError::TooLarge)
    );
    assert_eq!(s, before);
}

#[test]
fn a_zero_or_missing_distance_and_a_fold_are_refused() {
    let mut s = Draft::seeded(1);
    let (_, l) = rectangle(&mut s);
    for d in [0.0, f64::NAN, f64::INFINITY] {
        assert_eq!(
            modify::offset(&mut s, l[0], d, None),
            Err(ModifyError::NotPositive)
        );
    }
    // A chain that doubles back on itself has no offset.
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.0, 0.0), (0.020, 0.0), (0.010, 0.0)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let a = line(&mut s, p[0], p[1]);
    line(&mut s, p[1], p[2]);
    assert_eq!(
        modify::offset(&mut s, a, 0.001, None),
        Err(ModifyError::NotAChain)
    );
}

// ---- the constraints that tie a copy to its original ----------------------

use arrix_sketch::{Constraint, ConstraintId, Diagnostics, SketchStatus};

const W: f64 = 0.040;
const H: f64 = 0.030;
const R: f64 = 0.010;
const D: f64 = 0.003;

/// A 40 × 30 rectangle from a fixed origin: four H/V lines and two typed
/// sizes. Fully constrained. Returns its bottom line.
fn constrained_rectangle() -> (Draft, EntityId) {
    let mut s = Draft::seeded(1);
    let p: Vec<PointId> = [(0.0, 0.0), (W, 0.0), (W, H), (0.0, H)]
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            s.add_point(if i == 0 {
                Point::fixed(x, y)
            } else {
                Point::new(x, y)
            })
        })
        .collect();
    let l: Vec<EntityId> = (0..4).map(|i| line(&mut s, p[i], p[(i + 1) % 4])).collect();
    for (i, &line) in l.iter().enumerate() {
        s.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
    }
    for (a, b, value) in [(0, 1, W), (1, 2, H)] {
        s.add_constraint(Constraint::Distance {
            a: p[a],
            b: p[b],
            value,
        });
    }
    (s, l[0])
}

/// The rectangle with R10 corners: four lines, four arcs, `Tangent` at all
/// eight joints, four radii, a fixed first centre and the centre spacing.
/// Fully constrained. Returns its bottom line.
fn constrained_rounded() -> (Draft, EntityId) {
    let mut s = Draft::seeded(1);
    let centres = [(R, R), (W - R, R), (W - R, H - R), (R, H - R)];
    let c: Vec<PointId> = centres
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            s.add_point(if i == 0 {
                Point::fixed(x, y)
            } else {
                Point::new(x, y)
            })
        })
        .collect();
    let t = [
        (R, 0.0),
        (W - R, 0.0),
        (W, R),
        (W, H - R),
        (W - R, H),
        (R, H),
        (0.0, H - R),
        (0.0, R),
    ];
    let tp: Vec<PointId> = t.iter().map(|&(x, y)| pt(&mut s, x, y)).collect();
    let lines: Vec<EntityId> = [(0, 1), (2, 3), (4, 5), (6, 7)]
        .iter()
        .map(|&(a, b)| line(&mut s, tp[a], tp[b]))
        .collect();
    let arcs: Vec<EntityId> = [(1, 1, 2), (2, 3, 4), (3, 5, 6), (0, 7, 0)]
        .iter()
        .map(|&(centre, a, b)| {
            s.add_entity(Entity::Arc {
                center: c[centre],
                start: tp[a],
                end: tp[b],
            })
        })
        .collect();
    for (i, &line) in lines.iter().enumerate() {
        s.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
        for circle in [arcs[(i + 3) % 4], arcs[i]] {
            s.add_constraint(Constraint::Tangent { line, circle });
        }
    }
    for &target in &arcs {
        s.add_constraint(Constraint::Radius { target, value: R });
    }
    s.add_constraint(Constraint::HorizontalDistance {
        a: c[0],
        b: c[1],
        value: W - 2.0 * R,
    });
    s.add_constraint(Constraint::VerticalDistance {
        a: c[1],
        b: c[2],
        value: H - 2.0 * R,
    });
    (s, lines[0])
}

/// Two lines from a fixed origin, 40 along and 30 up. Fully constrained.
fn constrained_two_lines() -> (Draft, EntityId) {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let (p1, p2) = (pt(&mut s, W, 0.0), pt(&mut s, W, H));
    let (a, b) = (line(&mut s, p0, p1), line(&mut s, p1, p2));
    s.add_constraint(Constraint::Horizontal { line: a });
    s.add_constraint(Constraint::Vertical { line: b });
    for (x, y, value) in [(p0, p1, W), (p1, p2, H)] {
        s.add_constraint(Constraint::Distance { a: x, b: y, value });
    }
    (s, a)
}

/// A 30 mm line, then a tangent arc of R10 sweeping 60°. Fully constrained.
fn constrained_line_and_arc() -> (Draft, EntityId) {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::fixed(0.0, 0.0));
    let p1 = pt(&mut s, 0.030, 0.0);
    let c = pt(&mut s, 0.030, R);
    let end = (
        0.030 + R * 30f64.to_radians().cos(),
        R - R * 30f64.to_radians().sin(),
    );
    let p2 = pt(&mut s, end.0, end.1);
    let l = line(&mut s, p0, p1);
    let arc = s.add_entity(Entity::Arc {
        center: c,
        start: p1,
        end: p2,
    });
    s.add_constraint(Constraint::Horizontal { line: l });
    s.add_constraint(Constraint::Distance {
        a: p0,
        b: p1,
        value: 0.030,
    });
    s.add_constraint(Constraint::Radius {
        target: arc,
        value: R,
    });
    s.add_constraint(Constraint::Tangent {
        line: l,
        circle: arc,
    });
    s.add_constraint(Constraint::DistanceToAxisX {
        point: p2,
        value: end.0,
    });
    (s, l)
}

/// A slot on the X axis in the app's template: two `Horizontal` lines and
/// each joint straight over its centre, with no `Tangent` anywhere.
fn constrained_slot() -> (Draft, EntityId) {
    let mut s = Draft::seeded(1);
    let c0 = s.add_point(Point::fixed(0.0, 0.0));
    let c1 = pt(&mut s, 0.030, 0.0);
    let j: Vec<PointId> = [(0.0, -R), (0.030, -R), (0.030, R), (0.0, R)]
        .iter()
        .map(|&(x, y)| pt(&mut s, x, y))
        .collect();
    let lower = line(&mut s, j[0], j[1]);
    let upper = line(&mut s, j[2], j[3]);
    let right = s.add_entity(Entity::Arc {
        center: c1,
        start: j[1],
        end: j[2],
    });
    s.add_entity(Entity::Arc {
        center: c0,
        start: j[3],
        end: j[0],
    });
    for line in [lower, upper] {
        s.add_constraint(Constraint::Horizontal { line });
    }
    for (point, centre) in [(j[0], c0), (j[3], c0), (j[1], c1), (j[2], c1)] {
        s.add_constraint(Constraint::VerticalPoints {
            a: point,
            b: centre,
        });
    }
    s.add_constraint(Constraint::HorizontalPoints { a: c0, b: c1 });
    s.add_constraint(Constraint::Distance {
        a: c0,
        b: c1,
        value: 0.030,
    });
    s.add_constraint(Constraint::Radius {
        target: right,
        value: R,
    });
    (s, lower)
}

/// A circle about a fixed centre with a diameter.
fn constrained_circle() -> (Draft, EntityId) {
    let mut s = Draft::seeded(1);
    let c = s.add_point(Point::fixed(0.020, 0.015));
    let circle = s.add_entity(Entity::Circle {
        center: c,
        radius: R,
    });
    s.add_constraint(Constraint::Diameter {
        target: circle,
        value: 2.0 * R,
    });
    (s, circle)
}

struct Outcome {
    dof: i32,
    redundant: usize,
    conflicting: usize,
    status: SketchStatus,
    /// The farthest any point moved in the solve.
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

/// The offset of `start` in a fully constrained sketch leaves it fully
/// constrained, redundancy-free, and a solve moves nothing.
fn assert_offset_stays_constrained(mut s: Draft, start: EntityId, distance: f64) -> OffsetEdit {
    let before = s.clone();
    let base = solve(&mut s);
    assert_eq!((base.dof, base.redundant), (0, 0), "the original");
    let edit = modify::offset(&mut s, start, distance, None).unwrap();
    assert_original_untouched(&before, &s);
    let after = solve(&mut s);
    assert_eq!(after.dof, 0, "the copy adds no freedom");
    assert_eq!(
        (after.redundant, after.conflicting),
        (0, 0),
        "{:?}",
        edit.dropped
    );
    assert_eq!(after.status, SketchStatus::FullyConstrained);
    assert!(after.moved <= TOL, "a solve moves nothing: {}", after.moved);
    edit
}

fn kinds(s: &Sketch, ids: &[ConstraintId]) -> Vec<&'static str> {
    ids.iter()
        .map(|id| match s.get_constraint(*id).unwrap() {
            Constraint::Concentric { .. } => "Concentric",
            Constraint::DistanceCircleCircle { .. } => "DistanceCircleCircle",
            Constraint::Tangent { .. } => "Tangent",
            Constraint::Parallel { .. } => "Parallel",
            Constraint::DistanceParallelLines { .. } => "DistanceParallelLines",
            Constraint::Perpendicular { .. } => "Perpendicular",
            Constraint::PointOnLine { .. } => "PointOnLine",
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

fn count(kinds: &[&str], of: &str) -> usize {
    kinds.iter().filter(|k| **k == of).count()
}

#[test]
fn a_closed_offset_adds_no_freedom_to_a_fully_constrained_original() {
    let (s, start) = constrained_rectangle();
    let mut t = s.clone();
    let edit = assert_offset_stays_constrained(s, start, D);
    let k = {
        // Re-run to read the rows off the sketch: the helper consumed it.
        let e = modify::offset(&mut t, start, D, None).unwrap();
        kinds(&t, &e.constraints)
    };
    assert_eq!(count(&k, "Parallel"), 4);
    assert_eq!(count(&k, "DistanceParallelLines"), 4);
    assert_eq!(k.len(), 8, "a closed loop needs nothing else");
    assert!(edit.anchors.is_empty() && edit.dropped.is_empty());
    assert_eq!(edit.distances.len(), 4);
}

#[test]
fn a_tangent_joint_on_the_original_is_a_tangent_joint_on_the_copy() {
    let (s, start) = constrained_rounded();
    for distance in [D, -D] {
        let mut t = s.clone();
        let edit = assert_offset_stays_constrained(s.clone(), start, distance);
        let e = modify::offset(&mut t, start, distance, None).unwrap();
        let k = kinds(&t, &e.constraints);
        assert_eq!(count(&k, "Tangent"), 8, "one per joint");
        // Every tangent names a copy line and a copy arc.
        for id in &e.constraints {
            if let Constraint::Tangent { line, circle } = t.get_constraint(*id).unwrap() {
                let copies: Vec<EntityId> = e.pairs.iter().map(|p| p.1).collect();
                assert!(copies.contains(line) && copies.contains(circle));
            }
        }
        // The arcs carry the distance; the lines follow through tangency and
        // their eight rows are dropped, reported not lost.
        assert_eq!(count(&k, "DistanceCircleCircle"), 4);
        assert_eq!(count(&k, "DistanceParallelLines"), 0);
        assert_eq!(count(&k, "Parallel"), 0);
        assert_eq!(edit.dropped.len(), 8);
        assert_eq!(edit.distances.len(), 4);
    }
}

#[test]
fn an_open_chain_is_anchored_at_each_free_end() {
    let (s, start) = constrained_two_lines();
    let edit = assert_offset_stays_constrained(s.clone(), start, D);
    assert_eq!(edit.anchors.len(), 2);
    let mut t = s;
    let e = modify::offset(&mut t, start, D, None).unwrap();
    let k = kinds(&t, &e.constraints);
    assert_eq!(count(&k, "Perpendicular"), 2);
    assert!(e.anchors.iter().all(|a| t.is_construction(*a)));
    // Each anchor runs from an original end to the copy's.
    for a in &e.anchors {
        let (x, y) = ends(&t, *a);
        assert!(s_has(&e, x) ^ s_has(&e, y), "one end is the copy's");
    }
}

/// Is `p` one of the copy's corner points?
fn s_has(edit: &OffsetEdit, p: PointId) -> bool {
    edit.corners.contains(&p)
}

#[test]
fn an_arc_end_is_anchored_by_its_centre_on_the_anchor() {
    let (s, start) = constrained_line_and_arc();
    let edit = assert_offset_stays_constrained(s.clone(), start, D);
    assert_eq!(edit.anchors.len(), 2);
    let mut t = s;
    let e = modify::offset(&mut t, start, D, None).unwrap();
    let k = kinds(&t, &e.constraints);
    assert_eq!(count(&k, "Perpendicular"), 1, "the line's free end");
    assert_eq!(count(&k, "PointOnLine"), 1, "the arc's free end");
    // The line beside the tangent arc has no distance row of its own.
    assert_eq!(count(&k, "DistanceParallelLines"), 0);
    assert_eq!(count(&k, "DistanceCircleCircle"), 1);
    assert_eq!(count(&k, "Tangent"), 1);
    // The other side of the arc too.
    let (s, start) = constrained_line_and_arc();
    assert_offset_stays_constrained(s, start, -D);
}

#[test]
fn a_circle_is_a_concentric_pair_at_a_radius_difference() {
    let (s, circle) = constrained_circle();
    let mut t = s.clone();
    let edit = assert_offset_stays_constrained(s, circle, D);
    let e = modify::offset(&mut t, circle, D, None).unwrap();
    assert_eq!(
        kinds(&t, &e.constraints),
        ["Concentric", "DistanceCircleCircle"]
    );
    assert_eq!(edit.distances.len(), 1);
    let (s, circle) = constrained_circle();
    assert_offset_stays_constrained(s, circle, -D);
}

#[test]
fn a_slot_copy_is_held_smooth_though_the_original_has_no_tangent() {
    // The slot template is smooth by endpoint-over-centre. Without a
    // first-order row at each joint its copy keeps a degree of freedom there.
    let (mut s, start) = constrained_slot();
    let base = solve(&mut s);
    let e = modify::offset(&mut s, start, -D, None).unwrap();
    let after = solve(&mut s);
    assert_eq!(after.dof, base.dof, "the copy adds no freedom");
    assert_eq!(after.redundant, base.redundant);
    assert_eq!(after.conflicting, 0);
    assert!(after.moved <= TOL, "{}", after.moved);
    let k = kinds(&s, &e.constraints);
    assert_eq!(count(&k, "Tangent"), 4);

    // The reason: the rows without them leave the joints loose.
    let tangents: Vec<ConstraintId> = e
        .constraints
        .iter()
        .copied()
        .filter(|id| matches!(s.get_constraint(*id), Some(Constraint::Tangent { .. })))
        .collect();
    for id in tangents {
        s.remove_constraint(id);
    }
    assert!(solve(&mut s).dof > base.dof, "the joints are double roots");
}

#[test]
fn an_unconstrained_original_gives_an_equally_loose_copy() {
    let mut s = Draft::seeded(1);
    let (_, l) = rectangle(&mut s);
    let base = solve(&mut s);
    assert!(base.dof > 0);
    modify::offset(&mut s, l[0], D, None).unwrap();
    let after = solve(&mut s);
    assert_eq!(after.dof, base.dof);
    assert_eq!((after.redundant, after.conflicting), (0, 0));
}

// ---- the expression tie and a parameter edit ------------------------------

#[test]
fn a_typed_expression_reaches_every_distance_row() {
    for (build, name) in [
        (
            constrained_rectangle as fn() -> (Draft, EntityId),
            "rectangle",
        ),
        (constrained_rounded, "rounded"),
        (constrained_two_lines, "two lines"),
        (constrained_line_and_arc, "line and arc"),
        (constrained_circle, "circle"),
    ] {
        let (mut s, start) = build();
        let edit = modify::offset(&mut s, start, -D, Some("wall")).unwrap();
        assert!(!edit.distances.is_empty(), "{name}");
        for id in &edit.distances {
            assert_eq!(s.constraint_expr(*id), Some("wall"), "{name}: {id}");
            // The value is the magnitude; the side is the copy's geometry.
            match s.get_constraint(*id).unwrap() {
                Constraint::DistanceParallelLines { value, .. }
                | Constraint::DistanceCircleCircle { value, .. } => {
                    assert!((value - D).abs() <= TOL, "{name}");
                }
                other => panic!("{name}: not a distance: {other:?}"),
            }
        }
        // And no other new row has an expression.
        for id in &edit.constraints {
            assert_eq!(
                s.constraint_expr(*id).is_some(),
                edit.distances.contains(id),
                "{name}: {id}"
            );
        }
    }
}

#[test]
fn a_plain_number_is_stored_on_each_row_and_ties_nothing() {
    let (mut s, start) = constrained_rounded();
    let edit = modify::offset(&mut s, start, -D, None).unwrap();
    assert_eq!(edit.distances.len(), 4);
    assert!(
        edit.constraints
            .iter()
            .all(|id| s.constraint_expr(*id).is_none())
    );
    // Editing one row moves one arc, and the solve still converges: the rows
    // are independent driving dimensions.
    let id = edit.distances[0];
    if let Some(Constraint::DistanceCircleCircle { value, .. }) = s.get_constraint_mut(id) {
        *value = 0.005;
    }
    let out = solve(&mut s);
    assert_eq!(out.dof, 0);
}

/// Every distance row of the edit set to `value`, as a parameter edit would.
fn set_distances(s: &mut Draft, edit: &OffsetEdit, value: f64) {
    for id in &edit.distances {
        match s.get_constraint_mut(*id).unwrap() {
            Constraint::DistanceParallelLines { value: v, .. }
            | Constraint::DistanceCircleCircle { value: v, .. } => *v = value,
            other => panic!("not a distance: {other:?}"),
        }
    }
}

fn positions_of(s: &Sketch, points: &[PointId]) -> Vec<[f64; 2]> {
    points.iter().map(|p| pos(s, *p)).collect()
}

/// The original's points are where they were, to solver noise.
fn assert_held(now: &[[f64; 2]], held: &[[f64; 2]]) {
    for (a, b) in now.iter().zip(held) {
        assert!(near(*a, *b), "moved from {b:?} to {a:?}");
    }
}

#[test]
fn a_parameter_edit_reshapes_the_copy_and_the_original_stays_put() {
    // Rectangle: the copy is 3 mm in, then 6.
    let (mut s, start) = constrained_rectangle();
    let originals: Vec<PointId> = s.points().keys().copied().collect();
    let held = positions_of(&s, &originals);
    let edit = modify::offset(&mut s, start, -D, Some("wall")).unwrap();
    set_distances(&mut s, &edit, 2.0 * D);
    let out = solve(&mut s);
    assert_eq!((out.dof, out.redundant), (0, 0));
    assert_held(&positions_of(&s, &originals), &held);
    let corners = positions_of(&s, &edit.corners);
    for want in [
        [0.006, 0.006],
        [0.034, 0.006],
        [0.034, 0.024],
        [0.006, 0.024],
    ] {
        assert!(
            corners.iter().any(|c| near(*c, want)),
            "{want:?} in {corners:?}"
        );
    }

    // Rounded: the arcs go from r 7 to r 4 and the lines follow.
    let (mut s, start) = constrained_rounded();
    let originals: Vec<PointId> = s.points().keys().copied().collect();
    let held = positions_of(&s, &originals);
    let edit = modify::offset(&mut s, start, -D, Some("wall")).unwrap();
    set_distances(&mut s, &edit, 2.0 * D);
    let out = solve(&mut s);
    assert_eq!((out.dof, out.redundant, out.conflicting), (0, 0, 0));
    assert_held(&positions_of(&s, &originals), &held);
    for (orig, copy) in &edit.pairs {
        match s.entity(*copy) {
            Some(Entity::Arc { .. }) => {
                let (_, r) = round(&s, *copy);
                assert!((r - (R - 2.0 * D)).abs() <= TOL, "{orig}: {r}");
            }
            _ => {
                let (a, b) = line_pos(&s, *copy);
                let (a0, b0) = line_pos(&s, *orig);
                // Parallel to its original, and 6 mm from it.
                let d = [b[0] - a[0], b[1] - a[1]];
                let d0 = [b0[0] - a0[0], b0[1] - a0[1]];
                assert!((d[0] * d0[1] - d[1] * d0[0]).abs() <= 1e-9);
                let n = [-d0[1], d0[0]];
                let gap = ((a[0] - a0[0]) * n[0] + (a[1] - a0[1]) * n[1]) / dist(a0, b0);
                assert!((gap.abs() - 2.0 * D).abs() <= TOL, "{orig}: {gap}");
            }
        }
    }

    // A circle.
    let (mut s, circle) = constrained_circle();
    let edit = modify::offset(&mut s, circle, -D, Some("wall")).unwrap();
    set_distances(&mut s, &edit, 2.0 * D);
    solve(&mut s);
    assert!((round(&s, edit.pairs[0].1).1 - (R - 2.0 * D)).abs() <= TOL);
}

// ---- what the tool reads: the cursor's side and the hover geometry --------

#[test]
fn the_cursor_measures_a_signed_distance_in_offsets_convention() {
    // A closed loop, either way round: outward is positive.
    let mut s = Draft::seeded(1);
    let (_, l) = rectangle(&mut s);
    let d = |s: &Sketch, at| modify::offset_distance_at(s, l[0], at).unwrap();
    assert!((d(&s, [0.020, -0.003]) - 0.003).abs() <= TOL);
    assert!((d(&s, [0.020, 0.004]) + 0.004).abs() <= TOL);
    let mut s = Draft::seeded(1);
    let (_, l) = polygon(
        &mut s,
        &[(0.0, 0.0), (0.0, 0.030), (0.040, 0.030), (0.040, 0.0)],
    );
    let d = |at| modify::offset_distance_at(&s, l[0], at).unwrap();
    assert!(
        (d([-0.002, 0.010]) - 0.002).abs() <= TOL,
        "clockwise, outside"
    );

    // An open chain: left of its direction is positive.
    let mut s = Draft::seeded(1);
    let (a, b) = (pt(&mut s, 0.0, 0.0), pt(&mut s, 0.040, 0.0));
    let l = line(&mut s, a, b);
    assert!((modify::offset_distance_at(&s, l, [0.010, 0.005]).unwrap() - 0.005).abs() <= TOL);

    // A circle: outward is positive.
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, 0.010);
    let d = modify::offset_distance_at(&s, circle, [0.0, 0.007]).unwrap();
    assert!((d + 0.003).abs() <= TOL);
}

#[test]
fn the_hover_geometry_is_what_the_offset_writes() {
    let (s, start) = constrained_rounded();
    for distance in [D, -D] {
        let preview = modify::offset_preview(&s, start, distance).unwrap();
        let mut t = s.clone();
        let edit = modify::offset(&mut t, start, distance, None).unwrap();
        assert_eq!(preview.len(), edit.pairs.len());
        for (lines, (_, copy)) in preview.iter().zip(&edit.pairs) {
            assert_eq!(lines, &modify::curve_samples(&t, *copy).unwrap());
        }
    }
    let before = s.clone();
    assert_eq!(
        modify::offset_preview(&s, start, -0.020),
        Err(ModifyError::TooLarge)
    );
    assert_eq!(s, before);
}
