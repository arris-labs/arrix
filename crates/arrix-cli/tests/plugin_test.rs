//! `arrix test` (plans/boundary-m3 step 5, ADR-0026), on the spawned
//! binary: the report's schema held to `tests/golden/test.schema.json`
//! (rewritten under `UPDATE_SNAPSHOTS=1`), `gears` and the standoff
//! passing every check, a planted non-deterministic Python
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
fn gears_and_the_standoff_pass_every_check() {
    for plugin in ["plugins/gears", "python/examples/standoff"] {
        let out = arrix_test(&[], &workspace().join(plugin));
        let report = lines(&out);
        assert!(
            out.status.success(),
            "{plugin}: {report:?}{}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_valid(&report);
        for name in ["spec", "determinism", "round_trip", "frozen", "migration"] {
            assert_eq!(
                check(&report, name)["status"],
                "pass",
                "{plugin}: {report:?}"
            );
        }
        assert!(report.iter().all(|l| l["status"] == "pass"), "{report:?}");
    }
}

fn bom_copy(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arrix-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    copy_tree(&workspace().join("python/examples/bom"), &dir);
    let _ = std::fs::remove_dir_all(dir.join("__pycache__"));
    dir
}

#[test]
fn an_exporter_passes_its_export_check_and_a_random_one_fails_it() {
    let out = arrix_test(&[], &workspace().join("python/examples/bom"));
    let report = lines(&out);
    assert!(
        out.status.success(),
        "{report:?}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_valid(&report);
    let export = check(&report, "export");
    assert_eq!(export["status"], "pass", "{export}");
    let detail = export["detail"].as_str().unwrap();
    assert!(
        detail.contains("bom.csv") && detail.contains("blake3"),
        "{detail}"
    );

    let dir = bom_copy("random-export");
    let source = dir.join("bom.py");
    let text = std::fs::read_to_string(&source).unwrap();
    let planted = text
        .replace("import arrix\n", "import random\n\nimport arrix\n")
        .replace(
            "lines = [f\"part,",
            "lines = [f\"{random.random()}\", f\"part,",
        );
    assert_ne!(planted, text, "the plugin was changed");
    std::fs::write(&source, planted).unwrap();
    let out = arrix_test(&[], &dir);
    assert_eq!(out.status.code(), Some(1));
    let report = lines(&out);
    let export = check(&report, "export");
    assert_eq!(export["status"], "fail", "{export}");
    assert!(
        export["detail"]
            .as_str()
            .unwrap()
            .contains("two exports differ"),
        "{export}"
    );
    let _ = std::fs::remove_dir_all(&dir);
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

/// The standoff's checks with its manifest's `[test]` table rewritten.
fn standoff_with(name: &str, test_table: &str) -> Vec<Value> {
    let dir = standoff_copy(name);
    copy_tree(
        &workspace().join("tests/docs/empty"),
        &dir.join("fixtures/empty"),
    );
    let manifest = dir.join("arrix-plugin.toml");
    let text = std::fs::read_to_string(&manifest).unwrap();
    let head = text.split("[test]").next().unwrap();
    std::fs::write(&manifest, format!("{head}[test]\n{test_table}\n")).unwrap();
    let out = arrix_test(&[], &dir);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report = lines(&out);
    assert_valid(&report);
    let _ = std::fs::remove_dir_all(&dir);
    report
}

fn failed<'a>(report: &'a [Value], name: &str) -> Vec<&'a Value> {
    report
        .iter()
        .filter(|l| l["check"] == name && l["status"] == "fail")
        .collect()
}

#[test]
fn a_fixture_without_the_type_fails_migration_and_frozen() {
    let report = standoff_with("lacks", "documents = [\"fixtures/empty\"]");
    let migration = failed(&report, "migration");
    assert_eq!(migration.len(), 1, "{report:?}");
    assert!(
        migration[0]["detail"]
            .as_str()
            .unwrap()
            .contains("no fixture holds `standoff.standoff` at type_version 1"),
        "{migration:?}"
    );
    let frozen = failed(&report, "frozen");
    assert_eq!(frozen.len(), 1, "{report:?}");
    assert!(
        frozen[0]["detail"]
            .as_str()
            .unwrap()
            .contains("nothing freezes"),
        "{frozen:?}"
    );
}

#[test]
fn a_fixture_that_does_not_evaluate_fails_migration() {
    let report = standoff_with("broken", "documents = [\"fixtures/oversize-round\"]");
    let migration = failed(&report, "migration");
    assert_eq!(migration.len(), 1, "{report:?}");
    assert!(
        migration[0]["detail"]
            .as_str()
            .unwrap()
            .contains("does not evaluate"),
        "{migration:?}"
    );
}

#[test]
fn a_fail_fixture_that_succeeds_fails_kernel_failure() {
    let report = standoff_with("succeeds", "fail = [\"fixtures/standoff\"]");
    let kernel = failed(&report, "kernel_failure");
    assert_eq!(kernel.len(), 1, "{report:?}");
    assert!(
        kernel[0]["detail"].as_str().unwrap().contains("must"),
        "{kernel:?}"
    );
}
