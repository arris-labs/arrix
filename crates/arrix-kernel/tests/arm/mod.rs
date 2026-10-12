//! The probe's file (plans/c2-m3 step 1): an `arm` assembly holding a
//! bracket and a nested `mount`, itself holding a servo and one screw
//! placed twice, each part coloured, built in millimetres as Arris's
//! writer declares them. Shared by the probe and the structure tests;
//! `tests/fixtures/step/arm.step` is its text.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::f64::consts::{FRAC_PI_2, FRAC_PI_6};

use arris::Control;
use arris::io::step::{self, Occurrence, ProductTree, Rgb};
use arris::math::nalgebra::UnitQuaternion;
use arris::math::{Axis, Isometry, Point3, Vec3};
use arris::ops;
use arris::topo::{Body, Model};

/// The file is written in millimetres and read in metres.
pub const MM: f64 = 1e-3;

pub const NONE: &Control = &Control::NONE;

pub const GREY: Rgb = Rgb([0.6, 0.6, 0.6]);
pub const BLUE: Rgb = Rgb([0.1, 0.2, 0.7]);
pub const STEEL: Rgb = Rgb([0.8, 0.8, 0.85]);

pub fn rotation(axis: Vec3, angle: f64) -> UnitQuaternion<f64> {
    UnitQuaternion::from_axis_angle(&arris::math::UnitVec3::new_normalize(axis), angle)
}

/// The parts, in millimetres, each in its own frame: a bracket plate with
/// a bore, the servo of `tests/fixtures/step/servo.step`, a screw.
pub fn parts(m: &mut Model) -> [Body; 3] {
    let (plate, _) = ops::primitive_box(m, [0.0; 3], [60.0, 40.0, 5.0], NONE).unwrap();
    let bore = Axis::new(Point3::new(15.0, 20.0, -1.0), Vec3::z()).unwrap();
    let (bore, _) = ops::primitive_cylinder(m, bore, 4.0, 7.0, NONE).unwrap();
    let (bracket, _) = ops::cut(m, plate, bore, NONE).unwrap();

    let (servo, _) = ops::primitive_box(m, [0.0; 3], [40.0, 20.0, 36.0], NONE).unwrap();
    let shaft = Axis::new(Point3::new(10.0, 10.0, 36.0), Vec3::z()).unwrap();
    let (shaft, _) = ops::primitive_cylinder(m, shaft, 3.0, 6.0, NONE).unwrap();
    let (servo, _) = ops::fuse(m, servo, shaft, NONE).unwrap();

    let (screw, _) =
        ops::primitive_cylinder(m, Axis::z_at(Point3::origin()), 1.5, 10.0, NONE).unwrap();
    [bracket, servo, screw]
}

pub fn leaf(
    product: u64,
    name: &str,
    placement: Isometry,
    colour: Rgb,
    solid: usize,
) -> Occurrence {
    Occurrence {
        product: Some(product),
        name: name.into(),
        placement: Ok(placement),
        colour: Some(colour),
        solids: vec![solid],
        children: vec![],
    }
}

/// The placements as written, in millimetres, by name of the child.
pub fn written_placements() -> BTreeMap<&'static str, Isometry> {
    BTreeMap::from([
        (
            "bracket",
            Isometry::from_translation(Vec3::new(5.0, -3.0, 0.0)),
        ),
        (
            "mount",
            Isometry::new(rotation(Vec3::z(), FRAC_PI_2), Vec3::new(50.0, 10.0, 5.0)),
        ),
        (
            "servo",
            Isometry::new(rotation(Vec3::x(), FRAC_PI_6), Vec3::new(0.0, 2.0, 0.0)),
        ),
        (
            "screw-a",
            Isometry::new(
                rotation(Vec3::new(1.0, 1.0, 0.0), 0.3),
                Vec3::new(4.0, 4.0, 36.0),
            ),
        ),
        (
            "screw-b",
            Isometry::from_translation(Vec3::new(36.0, 16.0, 36.0)),
        ),
    ])
}

/// `arm` { bracket, mount { servo, screw, screw } }.
pub fn tree() -> ProductTree {
    let at = written_placements();
    let mount = Occurrence {
        product: Some(3),
        name: "mount".into(),
        placement: Ok(at["mount"]),
        colour: None,
        solids: vec![],
        children: vec![
            leaf(4, "servo", at["servo"], BLUE, 1),
            leaf(5, "screw", at["screw-a"], STEEL, 2),
            leaf(5, "screw", at["screw-b"], STEEL, 2),
        ],
    };
    let arm = Occurrence {
        product: Some(1),
        name: "arm".into(),
        placement: Ok(Isometry::identity()),
        colour: None,
        solids: vec![],
        children: vec![leaf(2, "bracket", at["bracket"], GREY, 0), mount],
    };
    ProductTree {
        roots: vec![arm],
        faces: vec![],
    }
}

/// The file's text.
pub fn arm_step() -> String {
    let mut built = Model::default();
    let parts = parts(&mut built);
    step::write_products(&built, &parts, &tree(), NONE).unwrap()
}
