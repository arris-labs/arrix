//! `arrix eval` as a user runs it: the binary, from the workspace root,
//! against the scenario documents in `tests/docs/`
//! (docs/ARCHITECTURE.md §Testing).

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arrix_eval(doc: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .arg("eval")
        .arg(doc)
        .output()
        .expect("run arrix")
}

/// A document directory under the test's scratch space, holding
/// `document.json` with `header`.
fn scratch_doc(name: &str, header: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("eval")
        .join(name);
    std::fs::create_dir_all(&dir).expect("create a scratch document");
    std::fs::write(dir.join("document.json"), header).expect("write document.json");
    dir
}

/// `arrix eval` on a scenario document exits 0 and prints its golden.
fn assert_golden(doc: &str, golden: &str) {
    let out = arrix_eval(Path::new(doc));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let golden =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(golden)).expect("read the golden");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&golden)
    );
    assert!(out.stderr.is_empty());
}

#[test]
fn the_empty_document_prints_its_golden_line() {
    assert_golden("tests/docs/empty", "tests/golden/eval_empty.jsonl");
}

/// The document tests/gear_on_plane.rs builds by commands and saves.
#[test]
fn the_gear_on_plane_document_prints_its_golden_line() {
    assert_golden(
        "tests/docs/gear-on-plane",
        "tests/golden/eval_gear_on_plane.jsonl",
    );
}

/// The document tests/slice.rs builds by commands and saves.
#[test]
fn the_slice_document_prints_its_golden_line() {
    assert_golden("tests/docs/slice", "tests/golden/eval_slice.jsonl");
}

/// Exit 2, nothing on stdout, and a message on stderr that says why.
fn assert_refused(out: &Output, says: &str) {
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(out.stdout.is_empty());
    assert!(stderr.contains(says), "{stderr:?} does not say {says:?}");
}

#[test]
fn a_missing_directory_exits_2() {
    assert_refused(
        &arrix_eval(Path::new("tests/docs/no-such-document")),
        "no such directory",
    );
}

#[test]
fn malformed_json_exits_2() {
    let doc = scratch_doc("malformed", "{\"schema\": 1,");
    assert_refused(&arrix_eval(&doc), "document.json is not valid JSON");
}

#[test]
fn a_newer_schema_exits_2() {
    let doc = scratch_doc("newer", "{\"schema\": 6}\n");
    assert_refused(&arrix_eval(&doc), "schema is 6, newer than this build's 5");
}

/// The scenario directory `dir` as an `.arrx` written to the scratch space.
fn scratch_zip(dir: &str, name: &str) -> PathBuf {
    let mut files = std::collections::BTreeMap::new();
    common::read_tree(&workspace().join(dir), "", &mut files);
    let zip = arrix_doc::to_zip(&arrix_doc::MemorySource(files)).expect("zip a scenario");
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.arrx"));
    std::fs::write(&path, zip).expect("write the zip");
    path
}

/// The line `arrix eval` prints for a zip is the golden's, bar the
/// document's name.
#[test]
fn a_zip_prints_the_directorys_line() {
    let zip = scratch_zip("tests/docs/slice", "slice");
    let out = arrix_eval(&zip);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let line = |bytes: &[u8]| -> serde_json::Value {
        let mut v: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        v["doc"] = serde_json::Value::Null;
        v
    };
    let golden =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/eval_slice.jsonl"))
            .unwrap();
    assert_eq!(line(&out.stdout), line(&golden));
}

#[test]
fn a_bad_zip_exits_2() {
    let path = Path::new(env!("CARGO_TARGET_TMPDIR")).join("bad.arrx");
    std::fs::write(&path, b"not a zip").unwrap();
    assert_refused(&arrix_eval(&path), "not a zip file");
}

/// `--without-plugin` leaves a plugin out as a build without it would:
/// its feature has no stored result in this scenario, so it fails as
/// missing, with no outputs.
#[test]
fn without_a_plugin_its_feature_without_a_stored_result_fails_as_missing() {
    let out = Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args([
            "eval",
            "--without-plugin",
            "gears",
            "tests/docs/gear-on-plane",
        ])
        .output()
        .expect("run arrix");
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains(r#""category":"plugin.frozen.missing""#),
        "{stdout}"
    );
    assert!(stdout.contains(r#""bodies":[]"#), "{stdout}");
}

#[test]
fn without_a_plugin_this_build_lacks_is_a_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(["eval", "--without-plugin", "nonesuch", "tests/docs/empty"])
        .output()
        .expect("run arrix");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("no such plugin"));
}

fn arrix_eval_args(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .arg("eval")
        .args(args)
        .output()
        .expect("run arrix")
}

/// The `doc` of each line printed.
fn docs(out: &Output) -> Vec<String> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).unwrap();
            v["doc"].as_str().unwrap().to_owned()
        })
        .collect()
}

/// A directory with no `document.json` is searched: every document below
/// it, sorted by path, one line each, and the exit status is the worst (1:
/// the oversize fillet fails, and the standoff, whose plugin is not started).
#[test]
fn a_directory_of_documents_prints_a_golden_line_for_each() {
    let out = arrix_eval_args(&["tests/docs"]);
    assert_eq!(out.status.code(), Some(1));
    let golden =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/eval_docs.jsonl"))
            .expect("read the golden");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&golden)
    );
    assert!(out.stderr.is_empty());
}

/// Paths are evaluated in the order given, whatever their names sort as,
/// and a directory and a document mix.
#[test]
fn paths_are_printed_in_the_order_given() {
    let out = arrix_eval_args(&[
        "tests/docs/slice",
        "tests/docs/migrations",
        "tests/docs/empty",
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        docs(&out),
        [
            "tests/docs/slice",
            "tests/docs/migrations/schema-1",
            "tests/docs/migrations/schema-2",
            "tests/docs/migrations/schema-3",
            "tests/docs/migrations/schema-4",
            "tests/docs/empty",
        ]
    );
}

/// A path that cannot be read is said on stderr and exits 2, but the
/// documents around it still print.
#[test]
fn an_unreadable_path_among_documents_exits_2_and_the_rest_print() {
    let out = arrix_eval_args(&["tests/docs/empty", "tests/docs/no-such", "tests/docs/slice"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(docs(&out), ["tests/docs/empty", "tests/docs/slice"]);
    assert!(String::from_utf8_lossy(&out.stderr).contains("no-such: no such directory"));
}

#[test]
fn a_directory_without_a_document_exits_2() {
    let dir = scratch_doc("hollow", "{}");
    std::fs::remove_file(dir.join("document.json")).unwrap();
    assert_refused(&arrix_eval(&dir), "no document here");
}

/// `--without-plugin` applies to every document: the gear freezes in the
/// one that has it and the others are as before.
#[test]
fn without_a_plugin_applies_to_every_document() {
    let with = arrix_eval_args(&["tests/docs/gear-on-plane", "tests/docs/empty"]);
    assert_eq!(with.status.code(), Some(0));
    let without = arrix_eval_args(&[
        "--without-plugin",
        "gears",
        "tests/docs/gear-on-plane",
        "tests/docs/empty",
    ]);
    assert_eq!(without.status.code(), Some(1));
    let lines: Vec<_> = String::from_utf8_lossy(&without.stdout)
        .lines()
        .map(str::to_owned)
        .collect();
    assert!(lines[0].contains("plugin.frozen.missing"));
    assert_eq!(
        lines[1],
        String::from_utf8_lossy(&with.stdout)
            .lines()
            .nth(1)
            .unwrap()
    );
}

/// `--save-fixtures` collects the recipes of every document in one place.
#[test]
fn save_fixtures_spans_the_documents() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("eval-fixtures");
    let _ = std::fs::remove_dir_all(&dir);
    let out = arrix_eval_args(&["--save-fixtures", dir.to_str().unwrap(), "tests/docs"]);
    assert_eq!(out.status.code(), Some(1));
    let saved: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(saved.len(), 1, "{saved:?}");
    assert!(saved[0].starts_with("kernel."), "{saved:?}");
    assert!(dir.join(&saved[0]).join("fixture.json").is_file());
}
