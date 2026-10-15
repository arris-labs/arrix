//! `arrix eval --sweep` (plans/c1-m4 step 9): the grammar, a document
//! parameter and a feature field, units, a point the parameter refuses,
//! and a point equal to the document saved at its value.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arrix_eval(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .arg("eval")
        .args(args)
        .output()
        .expect("run arrix")
}

fn lines(out: &Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect()
}

#[test]
fn a_five_point_sweep_of_the_slices_width_prints_its_golden() {
    let out = arrix_eval(&["tests/docs/slice", "--sweep", "w=20 mm..60 mm:5"]);
    assert_eq!(out.status.code(), Some(0));
    let golden = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/eval_slice_sweep.jsonl"),
    )
    .expect("read the golden");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&golden)
    );
    let points = lines(&out);
    assert_eq!(points.len(), 5);
    for (i, line) in points.iter().enumerate() {
        assert_eq!(line["sweep"]["index"], i);
        assert_eq!(line["sweep"]["target"], "w");
    }
    assert_eq!(points[0]["sweep"]["value"], 0.02);
    assert_eq!(points[4]["sweep"]["value"], 0.06);
}

/// The native side of the wasm32 determinism probe (plans/c4 step 2):
/// `arrix-plugin-host`'s `tests/wasm_determinism.rs` runs the same sweep
/// on wasm32 and compares with this golden. At 40 mm the holes miss the
/// plate, so a failed point is compared too. Rewritten with
/// `UPDATE_SNAPSHOTS=1`.
#[test]
fn a_three_point_sweep_of_the_brackets_width_prints_its_golden() {
    let out = arrix_eval(&["tests/docs/bracket", "--sweep", "w=40 mm..80 mm:3"]);
    assert_eq!(out.status.code(), Some(1), "the 40 mm point fails");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/eval_bracket_sweep.jsonl");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, &out.stdout).unwrap();
    }
    let golden = std::fs::read(&path).expect("read the golden");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&golden)
    );
    let status: Vec<_> = lines(&out).iter().map(|l| l["status"].clone()).collect();
    assert_eq!(status, ["failed", "ok", "ok"]);
}

/// A copy of the slice in the scratch space with `w` set to `expr`.
fn slice_with_width(name: &str, expr: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("sweep")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    let mut files = std::collections::BTreeMap::new();
    common::read_tree(&workspace().join("tests/docs/slice"), "", &mut files);
    for (path, bytes) in &files {
        let at = dir.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, bytes).unwrap();
    }
    let mut params: serde_json::Value = serde_json::from_slice(&files["params.json"]).unwrap();
    params["6504KHBAT217J"]["expr"] = expr.into();
    std::fs::write(
        dir.join("params.json"),
        serde_json::to_vec(&params).unwrap(),
    )
    .unwrap();
    dir
}

/// Every point is what `arrix eval` prints for the document saved with the
/// target at the point's value.
#[test]
fn a_point_is_the_document_saved_at_its_value() {
    let swept = arrix_eval(&["tests/docs/slice", "--sweep", "w=20 mm..60 mm:5"]);
    for (i, point) in lines(&swept).into_iter().enumerate() {
        let value = point["sweep"]["value"].as_f64().unwrap();
        let doc = slice_with_width(&format!("at-{i}"), &format!("{value} m"));
        let saved = arrix_eval(&[doc.to_str().unwrap()]);
        let mut want = lines(&saved).remove(0);
        want["doc"] = point["doc"].clone();
        want["sweep"] = point["sweep"].clone();
        assert_eq!(point, want, "point {i}");
    }
}

#[test]
fn a_feature_field_is_swept_by_the_features_name() {
    let out = arrix_eval(&[
        "tests/docs/slice",
        "--sweep",
        "Spur gear.width=4 mm..12 mm:3",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let volumes: Vec<f64> = lines(&out)
        .iter()
        .map(|l| {
            l["bodies"].as_array().unwrap().last().unwrap()["volume"]
                .as_f64()
                .unwrap()
        })
        .collect();
    assert!(
        volumes[0] < volumes[1] && volumes[1] < volumes[2],
        "{volumes:?}"
    );
}

/// The midpoint of 20..21 is not a whole tooth count: that point fails on
/// its `params` row and the others are untouched.
#[test]
fn a_value_the_parameter_refuses_fails_on_its_params_row() {
    let out = arrix_eval(&["tests/docs/slice", "--sweep", "teeth=20..21:3"]);
    assert_eq!(out.status.code(), Some(1));
    let points = lines(&out);
    let teeth = |i: usize| {
        points[i]["params"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["name"] == "teeth")
            .unwrap()
            .clone()
    };
    assert_eq!(points[0]["status"], "ok");
    assert_eq!(points[1]["status"], "failed");
    assert_eq!(points[2]["status"], "ok");
    assert!(teeth(1)["value"].is_null());
    assert!(
        teeth(1)["diagnostic"]["code"]
            .as_str()
            .unwrap()
            .starts_with("expr.")
    );
}

#[test]
fn a_sweep_over_several_documents_applies_to_each() {
    let out = arrix_eval(&[
        "tests/docs/slice",
        "tests/docs/slice",
        "--sweep",
        "w=20 mm..60 mm:2",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(lines(&out).len(), 4);
}

fn assert_refused(out: &Output, says: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(stderr.contains(says), "{stderr:?} does not say {says:?}");
}

#[test]
fn what_cannot_be_swept_exits_2_and_says_why() {
    let slice = "tests/docs/slice";
    assert_refused(
        &arrix_eval(&[slice, "--sweep", "nope=1 mm..2 mm:3"]),
        "no parameter named nope",
    );
    assert_refused(
        &arrix_eval(&[slice, "--sweep", "Nope.width=1 mm..2 mm:3"]),
        "no feature Nope",
    );
    assert_refused(
        &arrix_eval(&[slice, "--sweep", "Spur gear.nope=1 mm..2 mm:3"]),
        "no numeric field nope",
    );
    assert_refused(
        &arrix_eval(&[slice, "--sweep", "w=1 mm..2 mm:0"]),
        "at least 1",
    );
    assert_refused(&arrix_eval(&[slice, "--sweep", "w=1 mm..t:3"]), "reads t");
    assert_refused(
        &arrix_eval(&[slice, "--sweep", "w=1 mm"]),
        "expected <target>=",
    );
    // A unit the parameter does not have fails its row, not the run.
    let out = arrix_eval(&[slice, "--sweep", "w=1 kg..2 kg:2"]);
    assert_eq!(out.status.code(), Some(1));
}
