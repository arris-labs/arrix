//! `tests/docs/arm`, C3 M2's accept document (plans/c3-m2 step 9): the
//! servo on its bracket of C2's exchange assembly with limits on `hinge`
//! and `spin`, every part already carrying a material. Written by the
//! commands below (`UPDATE_SNAPSHOTS=1`), as `c2_m1` writes its document;
//! `robotics.urdf` on it is held to a text golden, the URDF and the size
//! and hash of each mesh.

mod common;

use arrix_core::IdMinter;
use arrix_doc::{AuthorId, Command, Joint, MemorySource, Session, expr::Expr, open};
use common::{Client, golden, read_tree, registry, scenario_directory, workspace};

/// The limits in degrees, by joint name.
const LIMITS: [(&str, &str, &str); 2] = [
    ("hinge", "-90 deg", "90 deg"),
    ("spin", "-180 deg", "180 deg"),
];

fn exchange() -> MemorySource {
    let mut files = std::collections::BTreeMap::new();
    read_tree(
        &workspace().join("tests/docs/assembly-exchange"),
        "",
        &mut files,
    );
    MemorySource(files)
}

/// The exchange assembly's session with its limits set, one command each.
fn arm() -> Client {
    let document = open(&exchange()).unwrap();
    let author = AuthorId(IdMinter::new(2028).next_id());
    let (assembly, joints) = {
        let (id, a) = document.assemblies().iter().next().unwrap();
        (*id, a.joints.values().cloned().collect::<Vec<Joint>>())
    };
    let mut c = Client::of_document(author, document, registry());
    for (name, lo, hi) in LIMITS {
        let mut joint = joints.iter().find(|j| j.name == name).unwrap().clone();
        joint.limits = Some([Expr::parse(lo).unwrap(), Expr::parse(hi).unwrap()]);
        c.submit(Command::EditJoint {
            assembly,
            joint: Box::new(joint),
        });
    }
    c
}

#[test]
fn the_arm_document_is_what_the_commands_save() {
    scenario_directory("arm", &arm().session.save());
}

#[test]
fn the_urdf_of_the_arm_is_held_to_its_golden() {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("arm-urdf");
    let _ = std::fs::remove_dir_all(&dir);
    let document = open(&exchange()).unwrap();
    let assembly = document.assemblies().keys().next().unwrap().to_string();
    let run = std::process::Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(["export", "tests/docs/arm", "--exporter", "robotics.urdf"])
        .args(["--assembly", &assembly, "-o"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );

    let mut files = std::collections::BTreeMap::new();
    read_tree(&dir, "", &mut files);
    let urdf = String::from_utf8(files.remove("Servo_on_bracket.urdf").unwrap()).unwrap();
    let mut text = urdf;
    text.push_str("\nmeshes (bytes, blake3):\n");
    for (name, bytes) in &files {
        text.push_str(&format!(
            "  {name} {} {}\n",
            bytes.len(),
            blake3::hash(bytes).to_hex()
        ));
    }
    golden("c3-m2.arm.txt", &text);
}
