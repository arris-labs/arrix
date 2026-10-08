//! `arrix export` as a user runs it (plans/c1-m4 step 6): the slice in each
//! format, read back as the kernel's tests read an export, the body
//! selection, the `.arrx` input and the exit statuses.

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
    counts: [usize; 3],
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
    let n = |k: &str| last[k].as_u64().unwrap() as usize;
    Final {
        feature: bodies[0]["feature"].as_str().unwrap().to_owned(),
        volume: last["volume"].as_f64().unwrap(),
        counts: [n("faces"), n("edges"), n("vertices")],
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
fn step_reads_back_to_the_evaluated_body() {
    let want = slice_final();
    let path = scratch("slice.step");
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "step"], &path),
        0,
    );
    let text = std::fs::read_to_string(&path).unwrap();
    let solids = arrix_kernel::read_step(&text).expect("Arris reads the file");
    assert_eq!(solids.len(), 1);
    let solid = solids[0];
    assert_eq!([solid.faces, solid.edges, solid.vertices], want.counts);
    assert!(
        (solid.volume - want.volume).abs() <= 1e-9 * want.volume,
        "{} against {}",
        solid.volume,
        want.volume
    );
    let again = scratch("slice-again.step");
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "step"], &again),
        0,
    );
    assert_eq!(
        std::fs::read(&again).unwrap(),
        text.as_bytes(),
        "deterministic"
    );
}

type Triangle = [[f64; 3]; 3];

/// Whether every edge of `triangles` is in two of them, and their volume.
fn closed_and_volume(triangles: &[Triangle]) -> (bool, f64) {
    let bits = |p: [f64; 3]| p.map(f64::to_bits);
    let mut uses: BTreeMap<([u64; 3], [u64; 3]), u32> = BTreeMap::new();
    let mut volume = 0.0;
    for t in triangles {
        for i in 0..3 {
            let (a, b) = (bits(t[i]), bits(t[(i + 1) % 3]));
            *uses.entry((a.min(b), a.max(b))).or_default() += 1;
        }
        let [a, b, c] = *t;
        let cross = [
            b[1] * c[2] - b[2] * c[1],
            b[2] * c[0] - b[0] * c[2],
            b[0] * c[1] - b[1] * c[0],
        ];
        volume += (a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2]) / 6.0;
    }
    (uses.values().all(|&n| n == 2), volume)
}

fn stl_triangles(bytes: &[u8]) -> Vec<Triangle> {
    let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
    assert_eq!(bytes.len(), 84 + 50 * count);
    let f = |at: usize| f64::from(f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap()));
    (0..count)
        .map(|i| {
            let base = 84 + 50 * i + 12;
            std::array::from_fn(|v| std::array::from_fn(|c| f(base + 12 * v + 4 * c)))
        })
        .collect()
}

fn obj_triangles(text: &str) -> Vec<Triangle> {
    let mut points = Vec::new();
    let mut triangles = Vec::new();
    for line in text.lines() {
        let mut words = line.split_whitespace();
        match words.next() {
            Some("v") => points.push(words.map(|w| w.parse().unwrap()).collect::<Vec<f64>>()),
            Some("f") => {
                let at = |w: &str| {
                    let i: usize = w.split('/').next().unwrap().parse().unwrap();
                    [points[i - 1][0], points[i - 1][1], points[i - 1][2]]
                };
                let corners: Vec<_> = words.map(at).collect();
                triangles.push([corners[0], corners[1], corners[2]]);
            }
            _ => {}
        }
    }
    triangles
}

/// The mesh is closed and encloses the body's volume to within 1 %: the
/// refined chord of a gear's teeth is far finer than that.
fn assert_closed_mesh(triangles: &[Triangle], want: f64) {
    let (closed, volume) = closed_and_volume(triangles);
    assert!(closed, "every edge is in two triangles");
    assert!(
        (volume - want).abs() <= 0.01 * want,
        "{volume} against {want}"
    );
}

#[test]
fn stl_and_obj_are_closed_with_the_bodys_volume() {
    let want = slice_final().volume;
    let stl = scratch("slice.stl");
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "stl"], &stl),
        0,
    );
    assert_closed_mesh(&stl_triangles(&std::fs::read(&stl).unwrap()), want);
    let obj = scratch("slice.obj");
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "obj"], &obj),
        0,
    );
    assert_closed_mesh(
        &obj_triangles(&std::fs::read_to_string(&obj).unwrap()),
        want,
    );
}

#[test]
fn a_body_by_feature_id_is_the_same_file_as_all_of_them() {
    let want = slice_final();
    let all = scratch("all.step");
    let one = scratch("one.step");
    assert_exit(
        &arrix_export("tests/docs/slice", &["--format", "step"], &all),
        0,
    );
    let by_id = format!("{}.body", want.feature);
    assert_exit(
        &arrix_export(
            "tests/docs/slice",
            &["--format", "step", "--body", &by_id],
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
    let out = scratch("never.step");
    let _ = std::fs::remove_file(&out);
    let step = ["--format", "step"];
    assert_refused(
        &arrix_export("tests/docs/no-such", &step, &out),
        "no such directory",
    );
    assert_refused(
        &arrix_export(
            "tests/docs/slice",
            &["--format", "step", "--body", "nope.body"],
            &out,
        ),
        "no such feature",
    );
    assert_refused(
        &arrix_export(
            "tests/docs/slice",
            &["--format", "step", "--body", "nodot"],
            &out,
        ),
        "expected <feature>.<slot>",
    );
    assert_refused(
        &arrix_export("tests/docs/empty", &step, &out),
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

    let out = scratch("partial.step");
    let _ = std::fs::remove_file(&out);
    let result = arrix_export(doc.to_str().unwrap(), &["--format", "step"], &out);
    assert_exit(&result, 1);
    assert!(String::from_utf8_lossy(&result.stderr).contains("some features failed"));
    assert!(arrix_kernel::read_step(&std::fs::read_to_string(&out).unwrap()).is_ok());
}

#[test]
fn a_gear_frozen_away_leaves_no_body_to_write() {
    let out = scratch("frozen.step");
    let result = arrix_export(
        "tests/docs/gear-on-plane",
        &["--format", "step", "--without-plugin", "gears"],
        &out,
    );
    assert_refused(&result, "export.no-bodies");
}
