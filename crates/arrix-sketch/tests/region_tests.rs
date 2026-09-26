//! Region detection (`find_profiles`): which closed loops become profiles,
//! which become holes, and the largest-first order `Extrude`/`Revolve` rely
//! on when they take `profiles.first()` (docs/DATA-MODEL.md §Sketches).
//!
//! The last section is `find_regions` and `RegionKey`: the same faces, each
//! with the key a feature stores to come back to the one it was given.

use std::f64::consts::PI;

use arrix_sketch::{
    Draft, Entity, EntityId, Point, Profile, RegionKey, ResolveRegion, find_profiles, find_regions,
};

/// An axis-aligned CCW rectangle with one shared point per corner, so the
/// half-edge walk sees a closed loop rather than four loose lines.
fn add_rectangle(s: &mut Draft, x0: f64, y0: f64, x1: f64, y1: f64) {
    let p0 = s.add_point(Point::new(x0, y0));
    let p1 = s.add_point(Point::new(x1, y0));
    let p2 = s.add_point(Point::new(x1, y1));
    let p3 = s.add_point(Point::new(x0, y1));
    for (start, end) in [(p0, p1), (p1, p2), (p2, p3), (p3, p0)] {
        s.add_entity(Entity::Line { start, end });
    }
}

/// The 100×60 mm plate every scenario draws its holes in.
fn plate() -> Draft {
    let mut s = Draft::seeded(1);
    add_rectangle(&mut s, 0.0, 0.0, 0.1, 0.06);
    s
}

const PLATE_AREA: f64 = 0.1 * 0.06;

fn close(got: f64, want: f64, rel: f64) -> bool {
    (got - want).abs() <= rel * want.abs()
}

/// Largest first, where areas within the tolerance squared (1e-12 m²) are
/// equal and ordered by their entities instead.
fn assert_largest_first(profiles: &[Profile]) {
    let areas: Vec<f64> = profiles.iter().map(|p| p.outer.signed_area).collect();
    assert!(
        areas.windows(2).all(|w| w[0] >= w[1] - 1e-12),
        "profiles are not largest first: {areas:?}"
    );
}

#[test]
fn a_circle_inside_a_rectangle_is_a_hole_of_the_plate() {
    let mut s = plate();
    let (_, circle) = s.add_circle(0.05, 0.03, 0.01);
    let disc = PI * 0.01 * 0.01;

    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 2, "{profiles:#?}");

    let plate = &profiles[0];
    assert!(
        close(plate.outer.signed_area, PLATE_AREA, 1e-9),
        "profiles[0] is not the plate: {profiles:#?}"
    );
    assert_eq!(plate.holes.len(), 1, "{profiles:#?}");
    let hole = &plate.holes[0];
    assert_eq!(hole.edges.len(), 1);
    assert_eq!(hole.edges[0].entity, circle);
    assert!(!hole.edges[0].forward, "a hole circle is traversed CW");
    assert!(close(hole.signed_area, -disc, 1e-2), "{}", hole.signed_area);
    let net = plate.outer.signed_area + hole.signed_area;
    assert!(close(net, PLATE_AREA - disc, 1e-2), "net plate area {net}");

    let island = &profiles[1];
    assert_eq!(island.outer.edges.len(), 1);
    assert_eq!(island.outer.edges[0].entity, circle);
    assert!(island.outer.edges[0].forward);
    assert!(close(island.outer.signed_area, disc, 1e-2));
    assert!(island.holes.is_empty());
}

#[test]
fn a_rectangle_inside_a_rectangle_is_a_hole_of_the_plate() {
    let mut s = plate();
    add_rectangle(&mut s, 0.04, 0.02, 0.06, 0.04);

    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 2, "{profiles:#?}");
    assert!(close(profiles[0].outer.signed_area, PLATE_AREA, 1e-9));
    assert_eq!(profiles[0].holes.len(), 1, "{profiles:#?}");
    assert_eq!(profiles[0].holes[0].edges.len(), 4);
    assert!(close(profiles[0].holes[0].signed_area, -0.02 * 0.02, 1e-9));
    assert!(profiles[1].holes.is_empty());
}

#[test]
fn concentric_circles_make_a_washer() {
    let mut s = Draft::seeded(1);
    s.add_circle(0.0, 0.0, 0.02);
    s.add_circle(0.0, 0.0, 0.01);

    let profiles = find_profiles(&s);
    assert_largest_first(&profiles);
    assert!(close(profiles[0].outer.signed_area, PI * 0.02 * 0.02, 1e-2));
    assert_eq!(profiles[0].holes.len(), 1, "{profiles:#?}");
}

#[test]
fn a_circle_crossing_the_boundary_cuts_the_plate_rather_than_holing_it() {
    // Centred on the plate's right edge, so half the disc is in the plate
    // and half is out. The arrangement cuts both: the edge is three pieces
    // and the rim is two, and the plate's own face now runs along the rim
    // where the disc bit into it. Nothing here is a hole — a hole is
    // material removed from inside, and this crosses the boundary.
    let mut s = plate();
    s.add_circle(0.1, 0.03, 0.01);
    let half_disc = PI * 0.01 * 0.01 / 2.0;

    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 3, "{profiles:#?}");
    assert_largest_first(&profiles);
    assert!(profiles.iter().all(|p| p.holes.is_empty()), "{profiles:#?}");

    let bitten = &profiles[0];
    assert!(
        close(bitten.outer.signed_area, PLATE_AREA - half_disc, 1e-9),
        "the plate keeps everything but the half disc: {profiles:#?}"
    );
    assert_eq!(
        bitten.outer.edges.len(),
        6,
        "three sides, two pieces of the fourth, and the rim between them: {bitten:#?}"
    );
    for lune in &profiles[1..] {
        assert!(close(lune.outer.signed_area, half_disc, 1e-9), "{lune:#?}");
        assert_eq!(lune.outer.edges.len(), 2, "{lune:#?}");
    }
}

#[test]
fn a_d_of_an_arc_and_its_chord_is_a_profile() {
    // Two edges, which the coincidence walk's `edges.len() >= 3` floor threw
    // away.
    let mut s = Draft::seeded(1);
    s.add_arc(0.0, 0.0, 0.01, 0.0, -0.01, 0.0);
    s.add_line(-0.01, 0.0, 0.01, 0.0);

    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 1, "{profiles:#?}");
    assert_eq!(profiles[0].outer.edges.len(), 2, "{profiles:#?}");
    assert!(profiles[0].holes.is_empty());
    assert!(close(
        profiles[0].outer.signed_area,
        PI * 0.01 * 0.01 / 2.0,
        1e-9
    ));
}

#[test]
fn a_lens_of_two_arcs_is_a_profile() {
    // Two edges again, and neither of them straight: the pair a chord-ordered
    // walk cannot separate at all, since both arcs run between the same two
    // points.
    let (r, c): (f64, f64) = (0.01, 0.006);
    let h = (r * r - c * c).sqrt();
    let mut s = Draft::seeded(1);
    s.add_arc(-c, 0.0, 0.0, -h, 0.0, h);
    s.add_arc(c, 0.0, 0.0, h, 0.0, -h);
    let theta = 2.0 * h.atan2(c);

    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 1, "{profiles:#?}");
    assert_eq!(profiles[0].outer.edges.len(), 2, "{profiles:#?}");
    assert!(profiles[0].holes.is_empty());
    assert!(close(
        profiles[0].outer.signed_area,
        r * r * (theta - theta.sin()),
        1e-9
    ));
}

#[test]
fn a_circle_crossed_by_a_chord_is_a_cap_and_a_d() {
    // The D-shaft scenario. The chord crosses the rim without sharing a point
    // with it and overhangs at both ends, so before the arrangement neither
    // the cap nor the D was a region: the disc was one profile and the chord
    // was loose geometry lying across it.
    let (r, y): (f64, f64) = (0.01, 0.006);
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, r);
    let (_, _, chord) = s.add_line(-0.012, y, 0.012, y);
    let theta = 2.0 * (y / r).acos();
    let cap = 0.5 * r * r * (theta - theta.sin());

    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 2, "{profiles:#?}");
    assert_largest_first(&profiles);
    assert!(profiles.iter().all(|p| p.holes.is_empty()), "{profiles:#?}");
    assert!(close(profiles[0].outer.signed_area, PI * r * r - cap, 1e-9));
    assert!(close(profiles[1].outer.signed_area, cap, 1e-9));

    // Each is the rim's own piece and the chord proper — never the whole
    // circle, and never the chord's overhanging stubs.
    for profile in &profiles {
        assert_eq!(profile.outer.edges.len(), 2, "{profile:#?}");
        let rim = profile
            .outer
            .edges
            .iter()
            .find(|e| e.entity == circle)
            .expect("a rim piece");
        let cut = profile
            .outer
            .edges
            .iter()
            .find(|e| e.entity == chord)
            .expect("the chord proper");
        assert!(rim.piece.is_some(), "the rim was cut in two: {profile:#?}");
        assert_eq!(
            cut.piece.map(|p| p.index),
            Some(1),
            "the chord's middle piece, between its two stubs: {profile:#?}"
        );
    }
}

#[test]
fn a_rectangle_split_by_a_shared_line_is_two_profiles_without_holes() {
    let mut s = Draft::seeded(1);
    let p0 = s.add_point(Point::new(0.0, 0.0));
    let bottom = s.add_point(Point::new(0.05, 0.0));
    let p1 = s.add_point(Point::new(0.1, 0.0));
    let p2 = s.add_point(Point::new(0.1, 0.06));
    let top = s.add_point(Point::new(0.05, 0.06));
    let p3 = s.add_point(Point::new(0.0, 0.06));
    for (start, end) in [
        (p0, bottom),
        (bottom, p1),
        (p1, p2),
        (p2, top),
        (top, p3),
        (p3, p0),
        (bottom, top),
    ] {
        s.add_entity(Entity::Line { start, end });
    }

    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 2, "{profiles:#?}");
    for profile in &profiles {
        assert!(close(profile.outer.signed_area, 0.05 * 0.06, 1e-9));
        assert!(profile.holes.is_empty(), "{profiles:#?}");
    }
}

#[test]
fn profiles_are_largest_first_with_a_small_circle() {
    let mut s = plate();
    s.add_circle(0.02, 0.03, 0.005);
    assert_largest_first(&find_profiles(&s));
}

#[test]
fn profiles_are_largest_first_with_a_large_circle() {
    // The circle encloses the whole plate: it is the largest profile, and the
    // plate is both its hole and an island of its own.
    let mut s = plate();
    s.add_circle(0.05, 0.03, 0.1);
    let profiles = find_profiles(&s);
    assert_largest_first(&profiles);
    assert_eq!(profiles[0].outer.edges.len(), 1);
    assert_eq!(profiles[0].holes.len(), 1, "{profiles:#?}");
    assert_eq!(profiles[0].holes[0].edges.len(), 4);
}

#[cfg(feature = "conics")]
#[test]
fn an_ellipse_inside_a_rectangle_is_a_hole_of_the_plate() {
    let mut s = plate();
    let (_, _, ellipse) = s.add_ellipse(0.05, 0.03, 0.07, 0.03, 0.01);

    let profiles = find_profiles(&s);
    assert_eq!(profiles.len(), 2, "{profiles:#?}");
    assert!(close(profiles[0].outer.signed_area, PLATE_AREA, 1e-9));
    assert_eq!(profiles[0].holes.len(), 1, "{profiles:#?}");
    let hole = &profiles[0].holes[0];
    assert_eq!(hole.edges[0].entity, ellipse);
    assert!(!hole.edges[0].forward);
    assert!(close(hole.signed_area, -PI * 0.02 * 0.01, 1e-2));
}

// --------------------------------------------------------------- region keys

/// The D-shaft sketch: a circle of 10 mm crossed by an overhanging chord at
/// height `y`. The D is the major segment, the cap the minor one, and both
/// are bounded by the same two entities.
fn crossed_circle(y: f64) -> (Draft, EntityId, EntityId) {
    let mut s = Draft::seeded(1);
    let (_, circle) = s.add_circle(0.0, 0.0, 0.01);
    let (_, _, chord) = s.add_line(-0.012, y, 0.012, y);
    (s, circle, chord)
}

/// The area of the minor segment a chord at height `y` cuts off a circle of
/// radius 10 mm.
fn cap_area(y: f64) -> f64 {
    let r: f64 = 0.01;
    let theta = 2.0 * (y / r).acos();
    0.5 * r * r * (theta - theta.sin())
}

#[test]
fn regions_are_the_profiles_with_a_key_each() {
    let (s, circle, chord) = crossed_circle(0.006);

    let regions = find_regions(&s);
    let profiles: Vec<Profile> = regions.iter().map(|r| r.profile.clone()).collect();
    assert_eq!(profiles, find_profiles(&s), "same faces, same order");
    let mut bounding = vec![circle, chord];
    bounding.sort();
    for region in &regions {
        assert_eq!(region.key.entities, bounding, "sorted: {region:#?}");
    }
}

#[test]
fn a_key_tells_the_cap_from_the_d() {
    // The whole reason the key carries a sample: the two faces are bounded
    // by the same two curves, so the entity set alone names neither.
    let (s, _, _) = crossed_circle(0.006);
    let regions = find_regions(&s);
    assert_eq!(regions.len(), 2, "{regions:#?}");

    for region in &regions {
        let resolved = region.key.resolve(&s).expect("its own face");
        assert!(
            close(
                resolved.profile.outer.signed_area,
                region.profile.outer.signed_area,
                1e-9
            ),
            "{region:#?} resolved to {resolved:#?}"
        );
    }
    assert_ne!(regions[0].key, regions[1].key);
}

#[test]
fn a_keys_sample_is_inside_the_face_and_outside_its_holes() {
    // A washer, where the centroid of the outer loop is in the hole: the
    // sample has to be guaranteed-interior, not an average.
    let mut s = Draft::seeded(1);
    s.add_circle(0.0, 0.0, 0.02);
    s.add_circle(0.0, 0.0, 0.01);

    let regions = find_regions(&s);
    assert_eq!(regions.len(), 2, "{regions:#?}");
    let washer = &regions[0];
    assert_eq!(washer.profile.holes.len(), 1, "{washer:#?}");
    let [x, y] = washer.key.sample_m();
    let radius = x.hypot(y);
    assert!(
        (0.01..0.02).contains(&radius),
        "the sample is in the ring, not the bore: {:?}",
        washer.key.sample
    );
}

#[test]
fn a_key_follows_its_face_when_a_dimension_moves_the_chord() {
    // The acceptance scenario in miniature: the chord's dimension changes,
    // the geometry moves, and each key still names the face it named before.
    let (mut s, _, chord) = crossed_circle(0.006);
    let regions = find_regions(&s);
    let (d, cap) = (regions[0].key.clone(), regions[1].key.clone());

    let Some(&Entity::Line { start, end }) = s.entity(chord) else {
        panic!("the chord is a line");
    };
    for p in [start, end] {
        s.point_mut(p).expect("the chord's end").y = 0.002;
    }

    let moved_cap = cap_area(0.002);
    let resolved_d = d.resolve(&s).expect("the D is still there");
    assert!(
        close(
            resolved_d.profile.outer.signed_area,
            PI * 0.01 * 0.01 - moved_cap,
            1e-9
        ),
        "the D grew with the chord, and is not the cap: {resolved_d:#?}"
    );
    let resolved_cap = cap.resolve(&s).expect("the cap is still there");
    assert!(
        close(resolved_cap.profile.outer.signed_area, moved_cap, 1e-9),
        "{resolved_cap:#?}"
    );
}

#[test]
fn a_key_resolves_to_nothing_once_its_chord_is_deleted() {
    // Fail soft: the disc is still a region and still the largest, and the
    // key does not quietly become it.
    let (mut s, _, chord) = crossed_circle(0.006);
    let d = find_regions(&s)[0].key.clone();

    s.remove_entity(chord);

    assert_eq!(find_regions(&s).len(), 1, "the disc is back to one face");
    assert!(d.resolve(&s).is_none(), "{d:#?}");
}

#[test]
fn a_key_resolves_to_nothing_once_another_chord_splits_its_face() {
    // The D is cut in two, so neither half is bounded by the D's curves any
    // more. The piece the sample lands in is *part* of the old face, but
    // saying so is a re-bind, and the feature is re-picked instead.
    let (mut s, _, _) = crossed_circle(0.006);
    let d = find_regions(&s)[0].key.clone();

    s.add_line(-0.012, -0.004, 0.012, -0.004);

    assert_eq!(find_regions(&s).len(), 3, "the cap and the D's two halves");
    assert!(d.resolve(&s).is_none(), "{d:#?}");
}

#[test]
fn a_key_of_entities_that_never_bounded_a_face_together_resolves_to_nothing() {
    let mut s = plate();
    let (_, circle) = s.add_circle(0.05, 0.03, 0.01);
    let key = RegionKey::new([circle], [0.02, 0.03]);

    assert!(key.resolve(&s).is_none(), "the sample is in the plate");
}
