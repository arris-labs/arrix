//! `arrix eval --summary` and `--timings` (plans/c1-m4 step 10): the
//! failure histogram as a last line, and the wall time as the one field
//! that is not deterministic.

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

/// The documents' lines are `eval_docs.jsonl`'s, and the histogram after
/// them is the oversize fillet's category, once.
#[test]
fn the_summary_over_the_scenario_documents_prints_its_golden() {
    let out = arrix_eval(&["tests/docs", "--summary"]);
    assert_eq!(out.status.code(), Some(1));
    let golden = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/eval_docs_summary.jsonl"),
    )
    .expect("read the golden");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&golden)
    );
    let plain = arrix_eval(&["tests/docs"]);
    let with = String::from_utf8_lossy(&out.stdout);
    assert!(with.starts_with(&*String::from_utf8_lossy(&plain.stdout)));
}

#[test]
fn a_clean_run_summarises_to_no_rows() {
    let out = arrix_eval(&["tests/docs/slice", "--summary"]);
    assert_eq!(out.status.code(), Some(0));
    let all = lines(&out);
    assert_eq!(all.len(), 2);
    assert_eq!(all[1], serde_json::json!({ "summary": [] }));
}

/// A sweep counts its failing points, each under its own line.
#[test]
fn a_sweep_is_counted_by_point() {
    let out = arrix_eval(&["tests/docs/slice", "--sweep", "teeth=20..21:3", "--summary"]);
    let all = lines(&out);
    assert_eq!(all.len(), 4);
    assert!(all[3]["summary"].is_array());
}

/// Only the fields are checked: the numbers are the machine's.
#[test]
fn timings_add_a_field_to_each_line_and_nothing_else() {
    let out = arrix_eval(&[
        "tests/docs/slice",
        "--sweep",
        "w=20 mm..60 mm:3",
        "--timings",
    ]);
    assert_eq!(out.status.code(), Some(0));
    let plain = arrix_eval(&["tests/docs/slice", "--sweep", "w=20 mm..60 mm:3"]);
    let (timed, plain) = (lines(&out), lines(&plain));
    assert_eq!(timed.len(), 3);
    for (mut line, expected) in timed.into_iter().zip(plain) {
        let timings = line.as_object_mut().unwrap().remove("timings").unwrap();
        assert!(timings["wall_ms"].as_f64().unwrap() > 0.0);
        assert_eq!(timings["lines"], 3);
        assert_eq!(line, expected);
    }
}
