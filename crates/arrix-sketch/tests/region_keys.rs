//! A region key resolves to exactly the region it was cut from, or to
//! none: never the nearest (docs/DATA-MODEL.md §Sketches). And the same
//! sketch gives the same regions, in the same order, every time.

use arrix_sketch::{Draft, EntityId, Point, RegionKey, find_regions};

/// The acceptance plate: 40 × 30 mm, one shared point per corner, and a
/// 10 mm bore. Returns the draft, its four sides and the bore.
fn holed_plate(seed: u64) -> (Draft, [EntityId; 4], EntityId) {
    let mut s = Draft::seeded(seed);
    let p: Vec<_> = [(0.0, 0.0), (0.04, 0.0), (0.04, 0.03), (0.0, 0.03)]
        .iter()
        .map(|&(x, y)| s.add_point(Point::new(x, y)))
        .collect();
    let sides = [0, 1, 2, 3].map(|i| {
        s.add_entity(arrix_sketch::Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        })
    });
    let (_, bore) = s.add_circle(0.02, 0.015, 0.005);
    (s, sides, bore)
}

#[test]
fn the_same_sketch_gives_the_same_regions_in_the_same_order() {
    let (a, _, _) = holed_plate(5);
    let (b, _, _) = holed_plate(5);
    let (ra, rb) = (find_regions(&a), find_regions(&b));
    assert_eq!(ra, rb);
    assert_eq!(ra.len(), 2, "the holed plate and the bore's disc");
    assert!(ra[0].profile.outer.signed_area > ra[1].profile.outer.signed_area);
    // Solving moves nothing here, and changes no region.
    let mut c = a.clone();
    c.solve();
    assert_eq!(find_regions(&c), ra);
}

#[test]
fn equal_regions_are_ordered_by_their_entities() {
    // Two equal discs: the tie is broken by the ids that bound them, not
    // by the order the walk met them in.
    let mut s = Draft::seeded(8);
    let (_, a) = s.add_circle(0.0, 0.0, 0.01);
    let (_, b) = s.add_circle(0.05, 0.0, 0.01);
    let regions = find_regions(&s);
    let first: Vec<EntityId> = regions.iter().map(|r| r.key.entities[0]).collect();
    assert_eq!(first, vec![a.min(b), a.max(b)]);
}

#[test]
fn the_holed_plates_key_names_its_bore_and_resolves_to_it() {
    let (s, sides, bore) = holed_plate(5);
    let plate = find_regions(&s)[0].key.clone();
    let mut bounding = sides.to_vec();
    bounding.push(bore);
    bounding.sort();
    assert_eq!(plate.entities, bounding);
    let found = plate.resolve(&s).expect("resolves");
    assert_eq!(found.profile.holes.len(), 1);
}

#[test]
fn deleting_the_bore_loses_the_holed_plate_never_the_rectangle() {
    let (mut s, _, bore) = holed_plate(5);
    let plate = find_regions(&s)[0].key.clone();
    s.remove_entity(bore);
    assert_eq!(find_regions(&s).len(), 1, "the plain rectangle is there");
    assert_eq!(plate.resolve(&s), None, "and the key does not take it");
}

#[test]
fn drawing_a_hole_inside_a_region_makes_it_another_region() {
    let (mut s, _, _) = holed_plate(5);
    let plate = find_regions(&s)[0].key.clone();
    s.add_circle(0.008, 0.008, 0.002);
    assert_eq!(plate.resolve(&s), None);
}

#[test]
fn deleting_a_side_loses_every_region_it_bounded() {
    let (mut s, sides, _) = holed_plate(5);
    let regions = find_regions(&s);
    s.remove_entity(sides[1]);
    assert_eq!(regions[0].key.resolve(&s), None, "the plate is open");
    assert!(
        regions[1].key.resolve(&s).is_some(),
        "the bore's disc stays"
    );
}

#[test]
fn a_key_is_plain_data() {
    let (s, _, _) = holed_plate(5);
    let key = find_regions(&s)[0].key.clone();
    let json = serde_json::to_string(&key).unwrap();
    let back: RegionKey = serde_json::from_str(&json).unwrap();
    assert_eq!(back, key);
    assert_eq!(back.resolve(&s).unwrap().key, key);
}
