//! The north star's URDF held to the readers (plans/c3-m3 step 15): screws
//! placed on `tests/docs/north-star` by `fasteners.place` over `arrix serve`
//! (`common/north_star.rs`), the document saved and exported by
//! `robotics.urdf` with `fasteners` loaded. The URDF and its meshes are
//! `c3-accept.north-star.txt`; `scripts/urdf-read` read that URDF into
//! `c3-accept.urdf.txt`, and ArriX's forward kinematics and mass pass are
//! held to the reading (`common/urdf.rs`). They run on the saved document
//! opened without `fasteners`, its screws frozen, so the reading also holds
//! the frozen bodies to the live ones the export weighed. The screws are
//! links on fixed joints.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use arrix_doc::{MemorySource, open};
use common::north_star::{ASSEMBLY, screwed};
use common::urdf::{arrix_places_and_weighs_as_the_readers_do, reading};
use common::{golden, read_tree, registry, workspace};

const READING: &str = include_str!("golden/c3-accept.urdf.txt");
const URDF: &str = "Servo_on_bracket.urdf";

fn scratch() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("c3-accept-urdf")
}

/// The north star with its screws, saved once for every test here.
fn saved() -> &'static Path {
    static SAVED: OnceLock<PathBuf> = OnceLock::new();
    SAVED.get_or_init(|| {
        let dir = scratch().join("north-star");
        screwed(&dir);
        dir
    })
}

/// `robotics.urdf` on the saved document, `fasteners` loaded: the files it
/// wrote, by relative name. The URDF is left at
/// `target/tmp/c3-accept-urdf/urdf/Servo_on_bracket.urdf` for the script.
fn exported() -> BTreeMap<String, Vec<u8>> {
    let out = scratch().join("urdf");
    let _ = std::fs::remove_dir_all(&out);
    let plugin = workspace().join("plugins/fasteners");
    let run = Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .env("PYTHONPATH", workspace().join("python"))
        .args(["export", saved().to_str().unwrap()])
        .args(["--plugin", plugin.to_str().unwrap()])
        .args(["--exporter", "robotics.urdf", "--assembly", ASSEMBLY])
        .args(["-o", out.to_str().unwrap()])
        .output()
        .expect("run arrix");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let mut files = BTreeMap::new();
    read_tree(&out, "", &mut files);
    files
}

#[test]
fn the_north_star_with_screws_exports_the_urdf_the_readers_read() {
    let mut files = exported();
    let urdf = files.remove(URDF).expect("the URDF");
    let mut text = String::from_utf8(urdf.clone()).unwrap();
    text.push_str("\nmeshes (bytes, blake3):\n");
    for (name, bytes) in &files {
        text.push_str(&format!(
            "  {name} {} {}\n",
            bytes.len(),
            blake3::hash(bytes).to_hex()
        ));
    }
    golden("c3-accept.north-star.txt", &text);
    assert_eq!(
        reading(READING).blake3,
        blake3::hash(&urdf).to_hex().as_str(),
        "the URDF is not the one the readers read: write c3-accept.urdf.txt \
         again with scripts/urdf-read"
    );
}

#[test]
fn urdfdom_and_yourdfpy_agree_on_the_tree() {
    common::urdf::the_readers_agree_on_the_tree(&reading(READING));
}

#[test]
fn arrix_places_and_weighs_the_screwed_north_star_as_the_readers_do() {
    let mut files = BTreeMap::new();
    read_tree(saved(), "", &mut files);
    let doc = open(&MemorySource(files)).expect("the north star opens");
    let r = reading(READING);
    assert_eq!(r.ats.len(), 2, "two configurations");
    let fixed = r.joints.values().filter(|j| j.kind == "fixed").count();
    assert_eq!(fixed, 4, "base_to_bracket, and a screw per hole");
    arrix_places_and_weighs_as_the_readers_do(&doc, registry(), &r);
}
