//! The probe of plans/c1-m1-sketch step 8, through `arrix-kernel`: a
//! sketched region extrudes, and its side faces are named by the sketch
//! entities that swept them (docs/DATA-MODEL.md §Persistent naming).

use std::f64::consts::PI;

use arrix_core::{
    CurveKey, FeatureId, Frame, Id, NameRoot, PersistentName, SweepPartName, TopoKind,
};
use arrix_kernel::Kernel;
use arrix_sketch::{Draft, Entity, EntityId, Point, curve_key, find_regions};

const MM: f64 = 1e-3;

fn feature() -> FeatureId {
    FeatureId(Id(100))
}

fn side(key: CurveKey) -> PersistentName {
    PersistentName {
        kind: TopoKind::Face,
        root: NameRoot::Sweep {
            feature: feature(),
            part: SweepPartName::Side(key),
        },
        chain: vec![],
    }
}

fn rectangle(d: &mut Draft, x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<EntityId> {
    let p = [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .map(|(x, y)| d.add_point(Point::new(x * MM, y * MM)));
    (0..4)
        .map(|i| {
            d.add_entity(Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect()
}

#[test]
fn a_sketched_plate_with_a_bore_extrudes_with_its_sides_named_by_their_entities() {
    let mut d = Draft::seeded(1);
    let sides = rectangle(&mut d, 0.0, 0.0, 40.0, 30.0);
    let (_, bore) = d.add_circle(20.0 * MM, 15.0 * MM, 5.0 * MM);
    let profile = find_regions(&d)[0].to_profile(&d, Frame::WORLD_XY).unwrap();

    let mut k = Kernel::new();
    let body = k.extrude(feature(), &profile, 5.0 * MM).unwrap();
    let volume = k.mass_properties(body).unwrap().volume;
    let want = (40.0 * 30.0 - PI * 5.0 * 5.0) * 5.0 * MM.powi(3);
    assert!((volume - want).abs() <= 1e-9 * want, "{volume} vs {want}");

    let names = k.names(body).unwrap();
    assert_eq!(
        names.count(TopoKind::Face),
        7,
        "two caps, four sides, the bore"
    );
    for entity in sides.iter().chain([&bore]) {
        assert!(
            names.contains(&side(curve_key(*entity, None))),
            "no side face named by {entity}"
        );
    }
}

#[test]
fn a_region_an_entity_bounds_twice_extrudes_with_unique_names() {
    // The plate less a notch across its bottom edge, which that edge
    // bounds in its pieces 0 and 2. The notch's sides are cut where they
    // cross it; its top is whole.
    let mut d = Draft::seeded(2);
    let plate = rectangle(&mut d, 0.0, 0.0, 40.0, 30.0);
    let notch = rectangle(&mut d, 10.0, -10.0, 30.0, 10.0);
    let profile = find_regions(&d)[0].to_profile(&d, Frame::WORLD_XY).unwrap();

    let mut k = Kernel::new();
    let body = k.extrude(feature(), &profile, 5.0 * MM).unwrap();
    let volume = k.mass_properties(body).unwrap().volume;
    let want = (40.0 * 30.0 - 20.0 * 10.0) * 5.0 * MM.powi(3);
    assert!((volume - want).abs() <= 1e-9 * want, "{volume} vs {want}");

    let names = k.names(body).unwrap();
    assert_eq!(names.count(TopoKind::Face), 10, "two caps, eight sides");
    let faces: Vec<&PersistentName> = names.of_kind(TopoKind::Face).collect();
    let mut unique = faces.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), faces.len(), "every face its own name");
    let keys = [
        curve_key(plate[0], Some(0)),
        curve_key(plate[0], Some(2)),
        curve_key(plate[1], None),
        curve_key(plate[2], None),
        curve_key(plate[3], None),
        curve_key(notch[1], Some(1)),
        curve_key(notch[2], None),
        curve_key(notch[3], Some(0)),
    ];
    for key in keys {
        assert!(names.contains(&side(key)), "no side face keyed {key:?}");
    }
}
