//! `arrix eval --jobs` (plans/c1-m4 step 11): documents on several threads,
//! the output and the exit status those of one.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn arrix_eval(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .arg("eval")
        .args(args)
        .output()
        .expect("run arrix")
}

/// The same bytes on stdout and stderr and the same status as one job.
fn assert_same_as_one_job(args: &[&str]) {
    let one = arrix_eval(args);
    for jobs in ["2", "4", "64"] {
        let mut with = args.to_vec();
        with.extend(["--jobs", jobs]);
        let many = arrix_eval(&with);
        assert_eq!(many.status.code(), one.status.code(), "--jobs {jobs}");
        assert_eq!(many.stdout, one.stdout, "--jobs {jobs}");
        assert_eq!(many.stderr, one.stderr, "--jobs {jobs}");
    }
}

#[test]
fn the_scenario_documents_print_the_same_with_any_number_of_jobs() {
    assert_same_as_one_job(&["tests/docs", "--summary"]);
}

#[test]
fn a_sweep_prints_the_same_with_any_number_of_jobs() {
    assert_same_as_one_job(&[
        "tests/docs/slice",
        "tests/docs/gear-on-plane",
        "tests/docs/naming",
        "--sweep",
        "w=20 mm..60 mm:4",
    ]);
}

/// An unreadable path is reported where it falls in the order, and the rest
/// still print.
#[test]
fn an_unreadable_document_does_not_change_the_order() {
    let bad = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("jobs-bad.arrx");
    std::fs::write(&bad, b"not a zip").unwrap();
    let bad = bad.to_string_lossy().into_owned();
    assert_same_as_one_job(&["tests/docs/empty", &bad, "tests/docs/slice"]);
}

#[test]
fn zero_jobs_is_one() {
    let one = arrix_eval(&["tests/docs/empty"]);
    let zero = arrix_eval(&["tests/docs/empty", "--jobs", "0"]);
    assert_eq!(one.stdout, zero.stdout);
}
