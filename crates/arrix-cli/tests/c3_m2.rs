//! C3 M2's accept (plans/c3-m2 step 11, ADR-0034, ADR-0037): `robotics`
//! built as a Tier 1 component exports `tests/docs/arm` through `arrix
//! export --exporter robotics.urdf --plugin <dir>` to the files it writes
//! compiled in, to the byte, and they are step 9's golden and step 10's
//! reading; the same export goes through `arrix serve` from `arrix.session`;
//! `bom` writes the arm's bill at Tier 2; a build without `robotics` names
//! the exporter and those there are; a width edit of the bracket moves the
//! URDF's joint origin by the edit.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use arrix_doc::{MemorySource, open};
use common::tier1::build;
use common::{copy_tree, golden, read_tree, workspace};

const READING: &str = include_str!("golden/c3-m2.urdf.txt");
const URDF: &str = "Servo_on_bracket.urdf";

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("c3-m2")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn arrix(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(args)
        .env("PYTHONPATH", workspace().join("python"))
        .output()
        .expect("run arrix")
}

/// The arm's only assembly, by id.
fn assembly() -> String {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/arm"), "", &mut files);
    let document = open(&MemorySource(files)).unwrap();
    document.assemblies().keys().next().unwrap().to_string()
}

/// `robotics.urdf` on `doc` exported to a fresh directory `name`, with
/// `extra` flags; the files it wrote, by relative name.
fn export(doc: &str, name: &str, extra: &[&str]) -> BTreeMap<String, Vec<u8>> {
    let dir = scratch(name);
    let assembly = assembly();
    let mut args = vec!["export", doc, "--exporter", "robotics.urdf"];
    args.extend(["--assembly", &assembly]);
    args.extend(extra);
    args.push("-o");
    let out = dir.to_str().unwrap();
    args.push(out);
    let run = arrix(&args);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let mut files = BTreeMap::new();
    read_tree(&dir, "", &mut files);
    files
}

/// The text `arm.rs` holds to `c3-m2.arm.txt`: the URDF, then each mesh's
/// size and hash.
fn arm_text(mut files: BTreeMap<String, Vec<u8>>) -> String {
    let mut text = String::from_utf8(files.remove(URDF).unwrap()).unwrap();
    text.push_str("\nmeshes (bytes, blake3):\n");
    for (name, bytes) in &files {
        text.push_str(&format!(
            "  {name} {} {}\n",
            bytes.len(),
            blake3::hash(bytes).to_hex()
        ));
    }
    text
}

#[test]
fn the_component_writes_the_files_it_has_compiled_in_to_the_byte() {
    let component = build::robotics();
    let native = export("tests/docs/arm", "native", &[]);
    let at_tier1 = export(
        "tests/docs/arm",
        "tier1",
        &["--plugin", component.to_str().unwrap()],
    );
    assert!(at_tier1.len() > 1, "a URDF and its meshes");
    assert_eq!(native, at_tier1, "the same files, the same bytes");

    // Step 9's golden, and step 10's reading of the same URDF.
    golden("c3-m2.arm.txt", &arm_text(at_tier1.clone()));
    let hash = blake3::hash(&at_tier1[URDF]).to_hex().to_string();
    assert!(
        READING.contains(&format!("blake3={hash}")),
        "the URDF is not the one the readers read; run scripts/urdf-read"
    );
}

#[test]
fn the_same_export_goes_through_serve_from_the_python_session() {
    let component = build::robotics();
    let want = export(
        "tests/docs/arm",
        "for-serve",
        &["--plugin", component.to_str().unwrap()],
    );
    let script = "\
import sys
from arrix.session import Session
binary, plugin, doc, assembly = sys.argv[1:5]
s = Session.spawn(arrix=binary, plugins=[plugin], doc=doc)
try:
    s.initialize()
    s.export_with('robotics.urdf', assembly=assembly)
    for name, data in s.exported_files(s.wait_event(kind='exported')):
        sys.stdout.write(name + ' ' + data.hex() + '\\n')
finally:
    s.close()
";
    let run = Command::new("python3")
        .arg("-c")
        .arg(script)
        .args([
            env!("CARGO_BIN_EXE_arrix"),
            component.to_str().unwrap(),
            workspace().join("tests/docs/arm").to_str().unwrap(),
            &assembly(),
        ])
        .env("PYTHONPATH", workspace().join("python"))
        .output()
        .expect("python3 runs");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "{stderr}");
    let mut served = BTreeMap::new();
    for line in String::from_utf8(run.stdout).unwrap().lines() {
        let (name, hex) = line.split_once(' ').unwrap();
        let bytes: Vec<u8> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        served.insert(name.to_owned(), bytes);
    }
    assert_eq!(served, want, "serve writes what the CLI writes");
}

#[test]
fn bom_writes_the_arms_bill_at_tier_2() {
    let dir = scratch("bom");
    let assembly = assembly();
    let run = arrix(&[
        "export",
        "tests/docs/arm",
        "--plugin",
        "python/examples/bom",
        "--exporter",
        "bom.csv",
        "--assembly",
        &assembly,
        "-o",
        dir.to_str().unwrap(),
    ]);
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let csv = std::fs::read_to_string(&dir).unwrap();
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!(rows[0], "part,count,mass (kg),material");
    assert!(rows.iter().any(|r| r.starts_with("Bracket,1,")), "{csv}");
    assert!(rows.last().unwrap().starts_with("total,6,"), "{csv}");
}

#[test]
fn a_build_without_robotics_names_the_exporter_and_those_there_are() {
    let assembly = assembly();
    let out = scratch("without");
    let run = arrix(&[
        "export",
        "tests/docs/arm",
        "--without-plugin",
        "robotics",
        "--exporter",
        "robotics.urdf",
        "--assembly",
        &assembly,
        "-o",
        out.to_str().unwrap(),
    ]);
    assert_eq!(run.status.code(), Some(2));
    let err = String::from_utf8_lossy(&run.stderr);
    assert!(
        err.contains("robotics.urdf") && err.contains("core.stl"),
        "{err}"
    );
    assert!(!err.contains("core.stl, robotics.urdf"), "{err}");
}

/// The `x` of the `<origin>` that follows `<joint name="{joint}"`.
fn joint_x(urdf: &str, joint: &str) -> f64 {
    let at = urdf
        .find(&format!("<joint name=\"{joint}\""))
        .expect("the joint");
    let rest = &urdf[at..];
    let origin = &rest[rest.find("<origin xyz=\"").unwrap() + 13..];
    origin.split_whitespace().next().unwrap().parse().unwrap()
}

#[test]
fn a_width_edit_of_the_bracket_moves_the_joint_origin_by_the_edit() {
    let edited = scratch("edited-doc");
    copy_tree(&workspace().join("tests/docs/arm"), &edited);
    let params = edited.join("params.json");
    let text = std::fs::read_to_string(&params).unwrap();
    assert_eq!(text.matches("\"expr\": \"40 mm\"").count(), 1);
    std::fs::write(
        &params,
        text.replace("\"expr\": \"40 mm\"", "\"expr\": \"46 mm\""),
    )
    .unwrap();

    let before = export("tests/docs/arm", "width-before", &[]);
    let after = export(edited.to_str().unwrap(), "width-after", &[]);
    let (before, after) = (
        String::from_utf8(before[URDF].clone()).unwrap(),
        String::from_utf8(after[URDF].clone()).unwrap(),
    );
    let moved = joint_x(&after, "hinge") - joint_x(&before, "hinge");
    assert!((moved - 0.006).abs() < 1e-9, "the hinge moved {moved} m");
    assert_eq!(joint_x(&after, "spin"), joint_x(&before, "spin"));
}
