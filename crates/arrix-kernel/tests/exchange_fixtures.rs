//! The accept's two small parts (plans/c2-m3 step 11), as Arris's own STEP
//! of their recipes, in millimetres like `servo.step`:
//!
//! - `horn.step`: a 30 × 10 × 4 bar with a shaft of radius 3 standing 6 on
//!   its top at (5, 5), the shape of the servo's own shaft, so a joint
//!   seats it in the servo's bore.
//! - `screw.step`: a cylinder of radius 1.5 and height 8 on the origin.
//!
//! `UPDATE_SNAPSHOTS=1` writes them again.

use std::path::PathBuf;

use arris::Control;
use arris::io::step;
use arris::math::{Axis, Point3, Vec3};
use arris::topo::Model;

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/step")
}

fn horn() -> String {
    let mut m = Model::default();
    let none = &Control::NONE;
    let (bar, _) = arris::ops::primitive_box(&mut m, [0.0; 3], [30.0, 10.0, 4.0], none).unwrap();
    let axis = Axis::new(Point3::new(5.0, 5.0, 4.0), Vec3::z()).unwrap();
    let (shaft, _) = arris::ops::primitive_cylinder(&mut m, axis, 3.0, 6.0, none).unwrap();
    let (body, _) = arris::ops::fuse(&mut m, bar, shaft, none).unwrap();
    step::write(&m, &[body]).unwrap()
}

fn screw() -> String {
    let mut m = Model::default();
    let axis = Axis::new(Point3::new(0.0, 0.0, 0.0), Vec3::z()).unwrap();
    let (body, _) = arris::ops::primitive_cylinder(&mut m, axis, 1.5, 8.0, &Control::NONE).unwrap();
    step::write(&m, &[body]).unwrap()
}

fn held(file: &str, text: &str) {
    let path = fixtures().join(file);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, text).unwrap();
        return;
    }
    let golden = std::fs::read_to_string(&path).unwrap();
    assert!(
        text == golden,
        "{}: Arris's writer changed it; rerun with UPDATE_SNAPSHOTS=1 and read the diff",
        path.display()
    );
}

#[test]
fn the_horn_fixture_is_its_recipe_s_step() {
    held("horn.step", &horn());
}

#[test]
fn the_screw_fixture_is_its_recipe_s_step() {
    held("screw.step", &screw());
}
