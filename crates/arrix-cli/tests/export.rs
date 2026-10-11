//! `arrix export` as a user runs it (plans/c1-m4 step 6): the slice in each
//! format, read back as the kernel's tests read an export (STEP refused
//! until Arris's writer can declare metres), the body selection, the
//! `.arrx` input and the exit statuses.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("export");
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    dir.join(name)
}

fn arrix_export(doc: &str, extra: &[&str], out: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(["export", doc])
        .args(extra)
        .arg("-o")
        .arg(out)
        .output()
        .expect("run arrix")
}

/// The slice's final body: the plate, with the gear joined and the pocket
/// cut, named by the feature that made it and measured as its last version.
struct Final {
    feature: String,
    volume: f64,
}

fn slice_final() -> Final {
    let out = Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(["eval", "tests/docs/slice"])
        .output()
        .expect("run arrix");
    let line: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let bodies = line["bodies"].as_array().unwrap();
    let last = bodies.last().unwrap().clone();
    Final {
        feature: bodies[0]["feature"].as_str().unwrap().to_owned(),
        volume: last["volume"].as_f64().unwrap(),
    }
}

fn assert_exit(out: &Output, code: i32) {
    assert_eq!(
        out.status.code(),
        Some(code),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn step_is_refused_until_the_writer_declares_metres() {
    let path = scratch("slice.step");
    let _ = std::fs::remove_file(&path);
    let out = arrix_export("tests/docs/slice", &["--format", "step"], &path);
    assert_refused(&out, "kernel.export.unit");
    assert!(String::from_utf8_lossy(&out.stderr).contains("A12"));
    assert!(!path.exists(), "a refused export writes nothing");
}

#[test]
fn stl_and_obj_are_closed_with_the_bodys_volume() {
    let want = slice_final().volume;
    let stl = scratch("slice.stl");
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "stl"], &stl),
        0,
    );
    common::assert_closed_mesh(&common::stl_triangles(&std::fs::read(&stl).unwrap()), want);
    let obj = scratch("slice.obj");
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "obj"], &obj),
        0,
    );
    common::assert_closed_mesh(
        &common::obj_triangles(&std::fs::read_to_string(&obj).unwrap()),
        want,
    );
}

#[test]
fn a_body_by_feature_id_is_the_same_file_as_all_of_them() {
    let want = slice_final();
    let all = scratch("all.obj");
    let one = scratch("one.obj");
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "obj"], &all),
        0,
    );
    let by_id = format!("{}.body", want.feature);
    assert_exit(
        &arrix_export(
            "tests/docs/slice",
            &["--format", "obj", "--body", &by_id],
            &one,
        ),
        0,
    );
    assert_eq!(std::fs::read(&all).unwrap(), std::fs::read(&one).unwrap());
}

#[test]
fn an_arrx_exports_like_its_directory() {
    let mut files = BTreeMap::new();
    common::read_tree(&workspace().join("tests/docs/slice"), "", &mut files);
    let zip = arrix_doc::to_zip(&arrix_doc::MemorySource(files)).unwrap();
    let arrx = scratch("slice.arrx");
    std::fs::write(&arrx, zip).unwrap();
    let from_zip = scratch("zip.stl");
    let from_dir = scratch("dir.stl");
    assert_exit(
        &arrix_export(arrx.to_str().unwrap(), &["--format", "stl"], &from_zip),
        0,
    );
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "stl"], &from_dir),
        0,
    );
    assert_eq!(
        std::fs::read(&from_zip).unwrap(),
        std::fs::read(&from_dir).unwrap()
    );
}

fn assert_refused(out: &Output, says: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains(says), "{stderr:?} does not say {says:?}");
}

#[test]
fn what_cannot_be_exported_exits_2_and_writes_nothing() {
    let out = scratch("never.obj");
    let _ = std::fs::remove_file(&out);
    let obj = ["--format", "obj"];
    assert_refused(
        &arrix_export("tests/docs/no-such", &obj, &out),
        "no such directory",
    );
    assert_refused(
        &arrix_export(
            "tests/docs/slice",
            &["--format", "obj", "--body", "nope.body"],
            &out,
        ),
        "no such feature",
    );
    assert_refused(
        &arrix_export(
            "tests/docs/slice",
            &["--format", "obj", "--body", "nodot"],
            &out,
        ),
        "expected <feature>.<slot>",
    );
    assert_refused(
        &arrix_export("tests/docs/empty", &obj, &out),
        "export.no-bodies",
    );
    assert!(!out.exists());
}

#[test]
fn a_document_with_a_failed_parameter_exits_1_with_the_file_written() {
    let doc = scratch("bad-param");
    let _ = std::fs::remove_dir_all(&doc);
    let mut files = BTreeMap::new();
    common::read_tree(&workspace().join("tests/docs/slice"), "", &mut files);
    for (path, bytes) in &files {
        let at = doc.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, bytes).unwrap();
    }
    let mut params: serde_json::Value = serde_json::from_slice(&files["params.json"]).unwrap();
    params["0ZZZZZZZZZZZZ"] = serde_json::json!({"expr": "1 kg", "kind": "length", "name": "bad"});
    std::fs::write(
        doc.join("params.json"),
        serde_json::to_vec(&params).unwrap(),
    )
    .unwrap();

    let out = scratch("partial.obj");
    let _ = std::fs::remove_file(&out);
    let result = arrix_export(doc.to_str().unwrap(), &["--format", "obj"], &out);
    assert_exit(&result, 1);
    assert!(String::from_utf8_lossy(&result.stderr).contains("some features failed"));
    common::assert_closed_mesh(
        &common::obj_triangles(&std::fs::read_to_string(&out).unwrap()),
        slice_final().volume,
    );
}

#[test]
fn a_gear_frozen_away_leaves_no_body_to_write() {
    let out = scratch("frozen.obj");
    let result = arrix_export(
        "tests/docs/gear-on-plane",
        &["--format", "obj", "--without-plugin", "gears"],
        &out,
    );
    assert_refused(&result, "export.no-bodies");
}
