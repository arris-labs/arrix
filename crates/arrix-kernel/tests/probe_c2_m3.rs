//! The C2 M3 probe (plans/c2-m3 step 1): what Arris 0.6 reads back from a
//! structured STEP its own `write_products` wrote, at `Metre`, before
//! ADR-0032 maps a product tree onto parts and components.
//!
//! The file: an `arm` assembly holding a bracket and a nested `mount`,
//! itself holding a servo and one screw placed twice, each part coloured,
//! built in millimetres as Arris's writer declares them.
//!
//! What it found:
//!
//! - The tree comes back whole: names, nesting, one product id shared by
//!   the two screws, colours exact, every placement in its parent's frame
//!   and in metres, a root's the identity.
//! - `Read::solids` holds one body per instance, baked at its world
//!   placement: the screw's two are one file solid at instances 0 and 1,
//!   and each occurrence's `solids` names its own instance.
//! - A body moved back into its product's frame by the inverse of its
//!   composed placement (`ops::transform`) passes the checker at
//!   `Level::Full`, its vertices within [`BACK_IN_FRAME`] of the part as
//!   built, its volume within [`VOLUME`] relative; the screw's two
//!   instances moved back agree vertex by vertex, by file entity, within
//!   [`INSTANCES_AGREE`], and their `Role::File` entities are the same
//!   ids, only the instance differs. Every error is within a few ulps of
//!   the coordinates, eight orders under the kernel's 1e-6 m tolerance.
//! - So a product's body in its own frame needs no new Arris ask: the
//!   inverse of the composed placement through `ops::transform` is exact
//!   to rounding (plans/c2-m3 Open question 1, ADR-0032). What it costs is
//!   work, not accuracy: a part placed n times is read n times.
//! - Every face, edge, vertex, shell and the body of every instance is
//!   `Generated` from a file entity, and nothing else is.
//!
//! The parts are planes and cylinders; a vendor file's B-splines move the
//! same way (`Surface::transformed` carries the control points), which the
//! accept's vendor part exercises at step 11.

mod arm;

use std::collections::BTreeMap;

use arm::{BLUE, GREY, MM, NONE, STEEL, parts, tree, written_placements};

use arris::check::{Level, check};
use arris::io::step::{self, LengthUnit, Occurrence, ProductTree, ReadOptions};
use arris::math::{Isometry, Point3, Precision};
use arris::ops;
use arris::topo::{Body, EntityId, Model, Origin, Provenance, Role, Shape};

/// How far a read placement may stand from the written one, in metres,
/// over points 0.1 m around: found 1.4e-17 (one rotated screw), every
/// other placement exact.
const PLACEMENT: f64 = 1e-16;

/// How far a vertex moved back into its product's frame may stand from
/// the part as built, in metres: found 2.0e-17 at worst (the servo, under
/// two rotations composed and inverted), against a 1e-6 m tolerance.
const BACK_IN_FRAME: f64 = 1e-16;

/// How far two instances of one part, each moved back, may differ, file
/// entity by file entity: found 2.9e-17 m.
const INSTANCES_AGREE: f64 = 1e-16;

/// The relative volume error of a body read and moved back: found
/// 4.7e-16 at worst.
const VOLUME: f64 = 1e-14;

/// The model the read lands in: metres, at the kernel's tolerance.
fn metres() -> Model {
    Model::new(Precision {
        default_tolerance: arrix_core::LENGTH_TOLERANCE,
        ..Precision::DEFAULT
    })
    .unwrap()
}

struct Probe {
    /// The parts as built, in millimetres.
    built: Model,
    parts: [Body; 3],
    /// The file read at `Metre`.
    read_into: Model,
    read: step::Read,
}

fn probe() -> Probe {
    let mut built = Model::default();
    let parts = parts(&mut built);
    let text = step::write_products(&built, &parts, &tree(), NONE).unwrap();
    let mut read_into = metres();
    let options = ReadOptions {
        length_unit: LengthUnit::Metre,
        ..ReadOptions::default()
    };
    let read = step::read(&mut read_into, &text, &options, NONE).unwrap();
    Probe {
        built,
        parts,
        read_into,
        read,
    }
}

/// The written placement in metres.
fn in_metres(p: &Isometry) -> Isometry {
    Isometry::new(p.rotation(), p.translation() * MM)
}

/// How far apart two placements move a few points 0.1 m around.
fn placement_error(a: &Isometry, b: &Isometry) -> f64 {
    [
        Point3::origin(),
        Point3::new(0.1, 0.0, 0.0),
        Point3::new(0.0, 0.1, 0.0),
        Point3::new(0.0, 0.0, 0.1),
    ]
    .iter()
    .map(|&p| (a.apply(p) - b.apply(p)).norm())
    .fold(0.0, f64::max)
}

/// Each leaf of the tree with its placement composed from the root, in
/// the tree's order.
fn leaves(tree: &ProductTree) -> Vec<(&Occurrence, Isometry)> {
    fn walk<'t>(o: &'t Occurrence, above: Isometry, out: &mut Vec<(&'t Occurrence, Isometry)>) {
        let here = o.placement.clone().unwrap().then(&above);
        if !o.solids.is_empty() {
            out.push((o, here));
        }
        for child in &o.children {
            walk(child, here, out);
        }
    }
    let mut out = Vec::new();
    for root in &tree.roots {
        walk(root, Isometry::identity(), &mut out);
    }
    out
}

/// Every vertex of `body` by the file entity it was generated from,
/// through `moved` (`Modified` one to one) when the body was moved.
fn vertices_by_file_entity(
    m: &Model,
    read: &Provenance,
    moved: &Provenance,
) -> BTreeMap<u64, (u32, Point3)> {
    let mut out = BTreeMap::new();
    for origin in read.origins_recorded() {
        let Origin::Role(Role::File(file)) = origin else {
            panic!("{origin} is not a file entity");
        };
        for shape in read.generated_from(origin) {
            let EntityId::Vertex(_) = shape.id else {
                continue;
            };
            let [after] = moved.modified_from(*shape) else {
                panic!("a move is one to one");
            };
            let EntityId::Vertex(v) = after.id else {
                panic!("a vertex moves to a vertex");
            };
            let point = m.vertex(v).unwrap().point();
            assert!(out.insert(file.id, (file.instance, point)).is_none());
        }
    }
    out
}

fn volume(m: &Model, body: Body) -> f64 {
    ops::measure::mass_properties(m, body, NONE).unwrap().volume
}

#[test]
fn the_tree_reads_back_with_its_names_nesting_products_and_colours() {
    let Probe { read, .. } = probe();
    let [arm] = &read.products.roots[..] else {
        panic!("one root: {:?}", read.products.roots)
    };
    assert_eq!(arm.name, "arm");
    assert_eq!(arm.placement, Ok(Isometry::identity()));
    let names = |o: &Occurrence| {
        o.children
            .iter()
            .map(|c| c.name.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(arm), ["bracket", "mount"]);
    let mount = &arm.children[1];
    assert_eq!(names(mount), ["servo", "screw", "screw"]);

    let [servo, a, b] = &mount.children[..] else {
        unreachable!()
    };
    assert!(a.product.is_some() && a.product == b.product, "one screw");
    let products: Vec<_> = read.products.occurrences().map(|o| o.product).collect();
    let mut distinct = products.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        5,
        "arm, bracket, mount, servo, screw: {products:?}"
    );

    assert_eq!(arm.children[0].colour, Some(GREY));
    assert_eq!(servo.colour, Some(BLUE));
    assert_eq!((a.colour, b.colour), (Some(STEEL), Some(STEEL)));
    assert_eq!((arm.colour, mount.colour), (None, None));
    assert!(read.products.faces.is_empty());
}

#[test]
fn placements_are_in_the_parent_s_frame_in_metres() {
    let Probe { read, .. } = probe();
    let at = written_placements();
    let arm = &read.products.roots[0];
    let mount = &arm.children[1];
    let pairs = [
        (&arm.children[0], "bracket"),
        (mount, "mount"),
        (&mount.children[0], "servo"),
        (&mount.children[1], "screw-a"),
        (&mount.children[2], "screw-b"),
    ];
    let mut worst: f64 = 0.0;
    for (o, written) in pairs {
        let read = o.placement.clone().unwrap();
        let error = placement_error(&read, &in_metres(&at[written]));
        println!("{written}: placement off by {error:e} m");
        worst = worst.max(error);
    }
    assert!(worst < PLACEMENT, "{worst:e}");
}

#[test]
fn each_instance_is_its_own_solid_baked_at_its_world_placement() {
    let Probe { read, .. } = probe();
    assert_eq!(read.solids.len(), 4, "bracket, servo, two screws");
    let leaves = leaves(&read.products);
    let indices: Vec<_> = leaves.iter().map(|(o, _)| o.solids.clone()).collect();
    println!("solid indices by leaf: {indices:?}");
    let [_, _, [a], [b]] = &indices.iter().map(Vec::as_slice).collect::<Vec<_>>()[..] else {
        panic!("one solid per leaf: {indices:?}")
    };
    let (a, b) = (&read.solids[*a], &read.solids[*b]);
    assert_eq!(a.entity.id, b.entity.id, "one file solid");
    let mut instances = [a.entity.instance, b.entity.instance];
    instances.sort();
    assert_eq!(instances, [0, 1]);
    for (o, _) in &leaves[..2] {
        assert_eq!(read.solids[o.solids[0]].entity.instance, 0, "{}", o.name);
    }
}

#[test]
fn a_solid_moved_back_by_its_composed_placement_is_its_part_as_built() {
    let Probe {
        built,
        parts,
        mut read_into,
        read,
    } = probe();
    let mut by_file = Vec::new();
    for (o, world) in leaves(&read.products) {
        let solid = read.solids[o.solids[0]].result.as_ref().unwrap();
        let back = world.inverse();
        let (body, moved) = ops::transform(&mut read_into, solid.body, &back, NONE).unwrap();
        assert!(check(&read_into, body, Level::Full).is_ok(), "{}", o.name);

        let part = match o.name.as_str() {
            "bracket" => parts[0],
            "servo" => parts[1],
            _ => parts[2],
        };
        let expected = volume(&built, part) * MM.powi(3);
        let got = volume(&read_into, body);
        let relative = ((got - expected) / expected).abs();

        let original: Vec<Point3> = built
            .closure(part)
            .unwrap()
            .vertices
            .iter()
            .map(|&v| built.vertex(v).unwrap().point() * MM)
            .collect();
        let vertices = vertices_by_file_entity(&read_into, &solid.provenance, &moved);
        assert_eq!(vertices.len(), original.len(), "{}", o.name);
        let off = vertices
            .values()
            .map(|(_, p)| {
                original
                    .iter()
                    .map(|q| (p - q).norm())
                    .fold(f64::INFINITY, f64::min)
            })
            .fold(0.0, f64::max);
        println!(
            "{}: vertices off by {off:e} m, volume by {relative:e}",
            o.name
        );
        assert!(off < BACK_IN_FRAME, "{}: {off:e}", o.name);
        assert!(relative < VOLUME, "{}: {relative:e}", o.name);
        by_file.push(vertices);
    }

    // The screw's two instances: the same file entities, the same points.
    let (a, b) = (&by_file[2], &by_file[3]);
    assert!(a.keys().eq(b.keys()), "one file solid, one set of names");
    assert!(a.values().all(|(i, _)| *i == a.values().next().unwrap().0));
    assert_ne!(a.values().next().unwrap().0, b.values().next().unwrap().0);
    let apart = a
        .iter()
        .map(|(id, (_, p))| (p - b[id].1).norm())
        .fold(0.0, f64::max);
    println!("screw instances apart by {apart:e} m");
    assert!(apart < INSTANCES_AGREE, "{apart:e}");
}

/// Which shapes of the read carry no file entity at all: none.
#[test]
fn every_entity_read_is_generated_from_a_file_entity() {
    let Probe {
        read, read_into, ..
    } = probe();
    for solid in &read.solids {
        let body = solid.result.as_ref().unwrap();
        let closure = read_into.closure(body.body).unwrap();
        let named: usize = body
            .provenance
            .origins_recorded()
            .map(|o| body.provenance.generated_from(o).len())
            .sum();
        let count = closure.faces.len()
            + closure.edges.len()
            + closure.vertices.len()
            + closure.shells.len()
            + 1;
        assert_eq!(named, count, "{:?}", Shape::from(body.body));
    }
}
