//! `arrix eval --budget` (plans/c1-m4 step 12): a step budget per kernel
//! call, the same on every run and every `--jobs`.

use std::path::Path;
use std::process::{Command, Output};

fn arrix_eval(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
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

/// Spends the slice's boolean (the gear joined to the plate) and nothing
/// before it; the sketch and extrude that read it then have no input.
const SMALL: &str = "1000";

#[test]
fn a_small_budget_fails_the_booleans_call_by_its_category() {
    let out = arrix_eval(&["tests/docs/slice", "--budget", SMALL]);
    assert_eq!(out.status.code(), Some(1));
    let line = &lines(&out)[0];
    assert_eq!(line["status"], "failed");
    let features = line["features"].as_array().unwrap();
    let failed: Vec<_> = features
        .iter()
        .filter(|f| f["category"] == "kernel.interrupted.budget")
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0]["type"], "core.boolean");
    assert_eq!(failed[0]["status"], "failed");
    // The calls before it were within budget.
    assert_eq!(features[3]["status"], "ok");
}

#[test]
fn the_same_budget_gives_the_same_bytes_twice_and_with_jobs() {
    let args = ["tests/docs/slice", "tests/docs/naming", "--budget", SMALL];
    let first = arrix_eval(&args);
    let second = arrix_eval(&args);
    let jobs = arrix_eval(&[&args[..], &["--jobs", "4"]].concat());
    assert_eq!(first.stdout, second.stdout);
    assert_eq!(first.stdout, jobs.stdout);
    assert_eq!(first.status.code(), jobs.status.code());
}

/// A budget no call outruns is no budget.
#[test]
fn a_generous_budget_changes_nothing() {
    let free = arrix_eval(&["tests/docs"]);
    let capped = arrix_eval(&["tests/docs", "--budget", "100000000"]);
    assert_eq!(free.stdout, capped.stdout);
    assert_eq!(free.status.code(), capped.status.code());
}

#[test]
fn the_summary_counts_the_budget_and_no_recipe_is_saved() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("budget-fixtures");
    let _ = std::fs::remove_dir_all(&dir);
    let out = arrix_eval(&[
        "tests/docs/slice",
        "--budget",
        SMALL,
        "--summary",
        "--save-fixtures",
        dir.to_str().unwrap(),
    ]);
    let summary = lines(&out).pop().unwrap();
    let row = summary["summary"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["category"] == "kernel.interrupted.budget")
        .expect("the budget's row");
    assert_eq!(row["count"], 1);
    assert_eq!(row["docs"], serde_json::json!(["tests/docs/slice"]));
    // Its record replays to success, so it is no fixture.
    assert!(!dir.exists() || std::fs::read_dir(&dir).unwrap().next().is_none());
}
