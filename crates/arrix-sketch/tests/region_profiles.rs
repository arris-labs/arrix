//! Regions as kernel profiles (docs/DATA-MODEL.md §Persistent naming): a
//! region becomes an `arrix_core::Profile` whose curves are keyed by the
//! entities that bound it, a whole entity by its id and a piece of a cut
//! one by `curve_key(entity, piece)`, so no profile holds a key twice.

use arrix_core::{CurveKey, DVec2, Frame, Id, ProfileLoop, ProfileSegment};
use arrix_sketch::{Draft, EntityId, curve_key, find_regions};

const MM: f64 = 1e-3;

fn rectangle(d: &mut Draft, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<EntityId> {
    let p = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .map(|(x, y)| d.add_point(arrix_sketch::Point::new(x * MM, y * MM)));
    (0..4)
        .map(|i| {
            d.add_entity(arrix_sketch::Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect()
}

fn keys(l: &ProfileLoop) -> Vec<CurveKey> {
    l.keys()
}

fn near(a: DVec2, b: DVec2) -> bool {
    (a - b).length() <= 1e-12
}

#[test]
fn a_whole_entity_is_keyed_by_its_id_and_a_piece_by_a_pinned_mix() {
    let e = arrix_sketch::SketchEntityId::from(Id(0x1234_5678_9ABC_DEF0));
    assert_eq!(curve_key(e, None), CurveKey(Id(0x1234_5678_9ABC_DEF0)));
    let pieces: Vec<CurveKey> = (0..3).map(|i| curve_key(e, Some(i))).collect();
    assert_eq!(
        pieces,
        [
            CurveKey(Id(0x0E7A_EF1B_D26B_F9BB)),
            CurveKey(Id(0x6FAA_2CC7_E49D_96B7)),
            CurveKey(Id(0xC25E_2094_5F74_1C14)),
        ]
    );
}

#[test]
fn a_plate_with_a_bore_keys_its_sides_and_its_bore_by_their_entities() {
    let mut d = Draft::seeded(1);
    let sides = rectangle(&mut d, 0.0, 0.0, 40.0, 30.0);
    let (_, bore) = d.add_circle(20.0 * MM, 15.0 * MM, 5.0 * MM);
    let regions = find_regions(&d);
    assert_eq!(regions.len(), 2, "the plate and the bore's disc");
    let profile = regions[0].to_profile(&d, Frame::WORLD_XY).unwrap();

    let ProfileLoop::Path { start, segments } = profile.outer() else {
        panic!("the outer loop is four lines");
    };
    let mut outer = keys(profile.outer());
    outer.sort();
    let mut want: Vec<CurveKey> = sides.iter().map(|s| curve_key(*s, None)).collect();
    want.sort();
    assert_eq!(outer, want);
    assert!(
        segments
            .iter()
            .all(|s| matches!(s, ProfileSegment::Line { .. }))
    );
    assert!(near(segments.last().unwrap().end(), *start), "it closes");
    assert_eq!(
        profile.holes(),
        [ProfileLoop::Circle {
            key: CurveKey(bore.0),
            center: DVec2::new(20.0 * MM, 15.0 * MM),
            radius: 5.0 * MM,
        }]
    );
}

#[test]
fn an_entity_bounding_a_region_twice_gives_each_piece_its_own_key() {
    // A notch across the plate's bottom edge cuts it at x = 10 and x = 30:
    // the plate less the notch runs along pieces 0 and 2 of that edge.
    let mut d = Draft::seeded(2);
    let plate = rectangle(&mut d, 0.0, 0.0, 40.0, 30.0);
    rectangle(&mut d, 10.0, -10.0, 30.0, 10.0);
    let regions = find_regions(&d);
    let profile = regions[0].to_profile(&d, Frame::WORLD_XY).unwrap();
    let outer = keys(profile.outer());
    let bottom = plate[0];
    for piece in [0, 2] {
        assert!(outer.contains(&curve_key(bottom, Some(piece))), "{outer:?}");
    }
    assert!(!outer.contains(&curve_key(bottom, None)));
    assert!(
        !outer.contains(&curve_key(bottom, Some(1))),
        "the notch's side"
    );
    for side in &plate[1..] {
        assert!(outer.contains(&curve_key(*side, None)), "uncut");
    }
    assert_eq!(outer.len(), 8);
    let ProfileLoop::Path { start, segments } = profile.outer() else {
        panic!("a path");
    };
    assert!(near(segments.last().unwrap().end(), *start), "it closes");
}

#[test]
fn a_circle_crossed_by_a_chord_becomes_arcs_through_their_midpoints() {
    let mut d = Draft::seeded(3);
    let (_, circle) = d.add_circle(0.0, 0.0, 10.0 * MM);
    let (_, _, chord) = d.add_line(-20.0 * MM, 5.0 * MM, 20.0 * MM, 5.0 * MM);
    let regions = find_regions(&d);
    assert_eq!(regions.len(), 2, "the D and the cap");
    for region in &regions {
        let profile = region.to_profile(&d, Frame::WORLD_XY).unwrap();
        let ProfileLoop::Path { start, segments } = profile.outer() else {
            panic!("a path");
        };
        assert_eq!(segments.len(), 2);
        assert!(near(segments[1].end(), *start));
        for s in segments {
            match *s {
                ProfileSegment::Arc { key, via, .. } => {
                    assert!((via.length() - 10.0 * MM).abs() <= 1e-12, "on the rim");
                    assert!(key == curve_key(circle, Some(0)) || key == curve_key(circle, Some(1)));
                }
                ProfileSegment::Line { key, .. } => assert_eq!(key, curve_key(chord, Some(1))),
            }
        }
    }
}
