//! `arrix eval --save-fixtures` as a user runs it (plans/c1-m2-naming
//! step 10, docs/ARCHITECTURE.md §The kernel choke point, "Fixture
//! export"): a document whose plate is extruded 0 mm fails at the kernel;
//! its record is written as a recipe. The grammar is checked at the kernel
//! (`arrix-kernel`'s `fixture_export`, over the vendored Arris reader):
//! Arris types stay in `arrix-kernel`. The oversize fillet of the plan's
//! acceptance has no feature type to fail through yet (`core.fillet` is
//! `c1-m2-features`), so its recipe is checked there too.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arrix_eval(doc: &str, extra: &[&Path]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_arrix"));
    cmd.current_dir(workspace()).arg("eval").arg(doc);
    for dir in extra {
        cmd.arg("--save-fixtures").arg(dir);
    }
    cmd.output().expect("run arrix")
}

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("save-fixtures")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The directories under `dir`, by name.
fn recipes(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("the fixtures directory exists")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_failed_kernel_call_is_written_as_a_recipe_arris_reads() {
    let dir = scratch("zero-plate");
    let out = arrix_eval("tests/docs/zero-plate", &[&dir]);
    assert_eq!(out.status.code(), Some(1));
    let line: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(line["status"], "failed");
    let code = "kernel.degenerate.not-positive";
    assert!(
        line["features"]
            .as_array()
            .unwrap()
            .iter()
            .any(|f| f["category"] == code),
        "{line}"
    );

    // One failure at a kernel call: the downstream ones read an unavailable
    // input and made no call.
    let names = recipes(&dir);
    let [name] = names.as_slice() else {
        panic!("{names:?}")
    };
    let hash = name.strip_prefix(&format!("{code}-")).expect(name);
    assert_eq!(hash.len(), 12);
    assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()), "{name}");

    let json = std::fs::read_to_string(dir.join(name).join("fixture.json")).unwrap();
    let recipe: serde_json::Value = serde_json::from_str(&json).unwrap();
    let ops: Vec<_> = recipe["steps"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["op"].as_str().unwrap())
        .collect();
    assert_eq!(ops, ["profile", "extrude"]);
    assert_eq!(recipe["result"], "c0");
    assert_eq!(recipe["steps"][1]["length"], 0.0);
    // The name is the recipe's own hash: written again, it is the same.
    let again = scratch("zero-plate-again");
    arrix_eval("tests/docs/zero-plate", &[&again]);
    assert_eq!(recipes(&again), names);
}

#[test]
fn the_line_is_the_same_with_or_without_the_flag() {
    let dir = scratch("same-line");
    let with = arrix_eval("tests/docs/zero-plate", &[&dir]);
    let without = arrix_eval("tests/docs/zero-plate", &[]);
    assert_eq!(with.stdout, without.stdout);
    assert_eq!(with.status.code(), without.status.code());
}

#[test]
fn a_document_that_fails_at_no_kernel_call_writes_no_recipe() {
    let dir = scratch("no-call");
    let out = arrix_eval("tests/docs/slice", &[&dir]);
    assert_eq!(out.status.code(), Some(0));
    assert!(!dir.exists() || recipes(&dir).is_empty());
}
