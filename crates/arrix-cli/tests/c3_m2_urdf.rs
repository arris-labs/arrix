//! `robotics.urdf` held to two independent readers (plans/c3-m2 step 10):
//! `scripts/urdf-read` ran urdfdom's `check_urdf` and yourdfpy on the
//! arm's URDF (`c3-m2.arm.txt`) at two joint configurations and wrote
//! `c3-m2.urdf.txt`, and ArriX's forward kinematics and mass pass on
//! `tests/docs/arm` are held to that reading (`common/urdf.rs`).

mod common;

use std::collections::BTreeMap;

use arrix_doc::{MemorySource, open};
use common::urdf::{arrix_places_and_weighs_as_the_readers_do, reading};
use common::{read_tree, registry, workspace};

const READING: &str = include_str!("golden/c3-m2.urdf.txt");
const ARM: &str = include_str!("golden/c3-m2.arm.txt");

#[test]
fn the_readers_read_the_urdf_the_arm_golden_holds() {
    let urdf = ARM
        .split_once("\nmeshes (bytes, blake3):\n")
        .expect("the arm golden lists its meshes")
        .0;
    assert_eq!(
        reading(READING).blake3,
        blake3::hash(urdf.as_bytes()).to_hex().as_str(),
        "c3-m2.arm.txt changed: write c3-m2.urdf.txt again with scripts/urdf-read"
    );
}

#[test]
fn urdfdom_and_yourdfpy_agree_on_the_tree() {
    common::urdf::the_readers_agree_on_the_tree(&reading(READING));
}

#[test]
fn arrix_places_and_weighs_the_arm_as_the_readers_do() {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/arm"), "", &mut files);
    let doc = open(&MemorySource(files)).expect("the arm opens");
    let r = reading(READING);
    assert_eq!(r.ats.len(), 2, "two configurations");
    arrix_places_and_weighs_as_the_readers_do(&doc, registry(), &r);
}
