//! `arrix eval` as a user runs it: the binary, from the workspace root,
//! against the scenario documents in `tests/docs/`
//! (docs/ARCHITECTURE.md §Testing).

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

#[test]
fn the_empty_document_prints_its_golden_line() {
    let out = arrix_eval(Path::new("tests/docs/empty"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let golden =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/eval_empty.jsonl"))
            .expect("read the golden");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&golden)
    );
    assert!(out.stderr.is_empty());
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
    let doc = scratch_doc("newer", "{\"schema\": 2}\n");
    assert_refused(&arrix_eval(&doc), "schema is 2, newer than this build's 1");
}
