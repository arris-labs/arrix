//! `arrix import` as a user runs it (plans/c2-m3 step 7, ADR-0032): the
//! probe's `arm.step` becomes a document directory that `arrix eval`
//! evaluates clean, the same bytes every time; a directory with files in
//! it and a file that is no STEP are refused.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arrix(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(args)
        .output()
        .expect("run arrix")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("import")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn import(step: &str, out: &Path) -> Output {
    arrix(&[
        "import".as_ref(),
        step.as_ref(),
        "-o".as_ref(),
        out.as_os_str(),
    ])
}

fn files(dir: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let name = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned();
                out.push((name, std::fs::read(&path).unwrap()));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

#[test]
fn the_arm_imports_to_a_document_that_evaluates_clean() {
    let out = scratch("arm");
    let done = import("tests/fixtures/step/arm.step", &out);
    assert_eq!(
        done.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&done.stderr)
    );
    let summary: serde_json::Value = serde_json::from_slice(&done.stdout).unwrap();
    assert_eq!(summary["imported"]["parts"], 3);
    assert_eq!(summary["imported"]["assemblies"], 2);
    assert_eq!(summary["imported"]["materials"], 3);
    assert_eq!(summary["imported"]["notes"], 0);

    let eval = arrix(&["eval".as_ref(), out.as_os_str()]);
    assert_eq!(
        eval.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&eval.stderr)
    );
    let line: serde_json::Value = serde_json::from_slice(&eval.stdout).unwrap();
    assert_eq!(line["status"], "ok");
    assert_eq!(line["bodies"].as_array().unwrap().len(), 3);
    assert_eq!(line["assemblies"].as_array().unwrap().len(), 2);
    assert_eq!(line["materials"].as_array().unwrap().len(), 3);
    // Colours only: no density, so the masses are partial, not guessed.
    let parts = line["mass"]["parts"].as_array().unwrap();
    assert!(
        parts
            .iter()
            .all(|p| p["partial"] == true && p.get("mass").is_none())
    );

    // The same file, the same document, byte for byte.
    let again = scratch("arm-again");
    assert_eq!(
        import("tests/fixtures/step/arm.step", &again).status.code(),
        Some(0)
    );
    assert_eq!(files(&out), files(&again));
}

#[test]
fn a_directory_with_files_in_it_is_refused() {
    let out = scratch("busy");
    std::fs::create_dir_all(&out).unwrap();
    std::fs::write(out.join("keep.txt"), "mine").unwrap();
    let done = import("tests/fixtures/step/arm.step", &out);
    assert_eq!(done.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&done.stderr).contains("not empty"));
    assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"mine");
}

#[test]
fn a_file_that_is_no_step_is_refused() {
    let bad = scratch("bad").with_extension("step");
    std::fs::create_dir_all(bad.parent().unwrap()).unwrap();
    std::fs::write(&bad, "nothing").unwrap();
    let out = scratch("bad-out");
    let done = import(bad.to_str().unwrap(), &out);
    assert_eq!(done.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&done.stderr).contains("kernel.import.parse"));
    assert!(!out.exists());
}
