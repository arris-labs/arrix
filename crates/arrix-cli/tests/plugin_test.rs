//! `arrix test` (plans/boundary-m3 step 5, ADR-0026), on the spawned
//! binary: the report's schema held to `tests/golden/test.schema.json`
//! (rewritten under `UPDATE_SNAPSHOTS=1`), `gears` and the standoff
//! passing determinism and round trip, a planted non-deterministic Python
//! plugin failing determinism with its detail, and a missing fixture and a
//! bad `[test]` key each a diagnostic with the manifest's file and line.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{copy_tree, workspace};
use serde_json::Value;

const GOLDEN: &str = "tests/golden/test.schema.json";

fn arrix_test(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .env("PYTHONPATH", workspace().join("python"))
        .arg("test")
        .args(args)
        .arg(dir)
        .output()
        .expect("run arrix")
}

fn lines(out: &Output) -> Vec<Value> {
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).expect("a JSON line"))
        .collect()
}

fn schema() -> Value {
    let out = Command::new(env!("CARGO_BIN_EXE_arrix"))
        .args(["test", "--schema"])
        .output()
        .expect("run arrix");
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).expect("UTF-8");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&golden, &text).expect("the golden is written");
    }
    assert!(
        text == std::fs::read_to_string(&golden).unwrap_or_default(),
        "{GOLDEN} differs from `arrix test --schema`; rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
    serde_json::from_str(&text).expect("JSON")
}

fn assert_valid(lines: &[Value]) {
    let validator = jsonschema::validator_for(&schema()).expect("a valid schema");
    for line in lines {
        let errors: Vec<String> = validator.iter_errors(line).map(|e| e.to_string()).collect();
        assert!(errors.is_empty(), "{line}\n{}", errors.join("\n"));
    }
}

fn check<'a>(lines: &'a [Value], name: &str) -> &'a Value {
    lines
        .iter()
        .find(|l| l["check"] == name)
        .unwrap_or_else(|| panic!("no `{name}` line in {lines:?}"))
}

/// A copy of the standoff plugin and its fixture in a fresh directory.
fn standoff_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arrix-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_tree(&workspace().join("python/examples/standoff"), &dir);
    let _ = std::fs::remove_dir_all(dir.join("__pycache__"));
    dir
}

#[test]
fn the_report_schema_is_fresh_and_names_the_format() {
    let schema = schema();
    assert_eq!(schema["properties"]["format"]["const"], 1);
    let validator = jsonschema::validator_for(&schema).expect("a valid schema");
    assert!(!validator.is_valid(&serde_json::json!({"plugin": "x"})));
}

#[test]
fn gears_and_the_standoff_pass_determinism_and_round_trip() {
    for plugin in ["plugins/gears", "python/examples/standoff"] {
        let out = arrix_test(&[], &workspace().join(plugin));
        let report = lines(&out);
        assert!(
            out.status.success(),
            "{plugin}: {report:?}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_valid(&report);
        for name in ["determinism", "round_trip"] {
            assert_eq!(
                check(&report, name)["status"],
                "pass",
                "{plugin}: {report:?}"
            );
        }
    }
}

#[test]
fn a_non_deterministic_plugin_fails_determinism_with_its_detail() {
    let dir = standoff_copy("random");
    let source = dir.join("standoff.py");
    let text = std::fs::read_to_string(&source).unwrap();
    let planted = text
        .replace("import math\n", "import math\nimport random\n")
        .replace(
            "shapes.polygon_loop(hexagon, SIDES)), p[\"height\"]",
            "shapes.polygon_loop(hexagon, SIDES)), p[\"height\"] * (1 + random.random() / 100)",
        );
    assert_ne!(planted, text, "the plugin was changed");
    std::fs::write(&source, planted).unwrap();

    let out = arrix_test(&[], &dir);
    assert_eq!(out.status.code(), Some(1));
    let report = lines(&out);
    assert_valid(&report);
    let determinism = check(&report, "determinism");
    assert_eq!(determinism["status"], "fail", "{determinism}");
    assert!(
        determinism["detail"]
            .as_str()
            .unwrap()
            .contains("two evaluations differ"),
        "{determinism}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_fixture_and_a_bad_key_are_diagnostics_with_file_and_line() {
    let dir = standoff_copy("diagnostics");
    let manifest = dir.join("arrix-plugin.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let missing = text.replace("fixtures/standoff", "fixtures/nowhere");
    std::fs::write(&manifest, &missing).unwrap();
    let line = missing
        .lines()
        .position(|l| l.contains("fixtures/nowhere"))
        .unwrap()
        + 1;
    let out = arrix_test(&[], &dir);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    let at = format!("arrix-plugin.toml:{line}: the fixture `fixtures/nowhere` does not exist");
    assert!(err.contains(&at), "{err}");

    std::fs::write(
        &manifest,
        text.replace("[test]", "[test]\ndocuments_typo = []"),
    )
    .unwrap();
    let out = arrix_test(&[], &dir);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        err.contains("arrix-plugin.toml") && err.contains("line") && err.contains("documents_typo"),
        "{err}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
