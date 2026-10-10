//! `arrix eval --schema` (ADR-0026): the eval line's JSON Schema, held to
//! `tests/golden/eval.schema.json` and rewritten under `UPDATE_SNAPSHOTS=1`,
//! and every kind of line `eval` prints validated against it.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const GOLDEN: &str = "tests/golden/eval.schema.json";

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

fn schema() -> serde_json::Value {
    let out = arrix_eval(&["--schema"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("UTF-8");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&golden, &text).expect("the golden is written");
    }
    assert!(
        text == std::fs::read_to_string(&golden).unwrap_or_default(),
        "{GOLDEN} differs from `arrix eval --schema`; rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
    serde_json::from_str(&text).expect("JSON")
}

fn assert_valid(validator: &jsonschema::Validator, text: &str, what: &str) {
    for line in text.lines() {
        let value: serde_json::Value = serde_json::from_str(line).expect("a JSON line");
        let errors: Vec<String> = validator
            .iter_errors(&value)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{what}: {line}\n{}", errors.join("\n"));
    }
}

#[test]
fn the_eval_schema_is_fresh_and_names_the_format() {
    let schema = schema();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    let line = &schema["$defs"]["EvalLine"]["properties"];
    assert!(line["format"].is_object());
    assert!(line["timings"].is_object());
}

#[test]
fn every_eval_golden_a_sweep_and_a_timings_line_validate() {
    let validator = jsonschema::validator_for(&schema()).expect("a valid schema");
    let goldens = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut seen = 0;
    for entry in std::fs::read_dir(&goldens).expect("golden dir") {
        let path = entry.expect("entry").path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if (name.starts_with("eval_") || name == "bracket.jsonl") && name.ends_with(".jsonl") {
            assert_valid(&validator, &std::fs::read_to_string(&path).unwrap(), &name);
            seen += 1;
        }
    }
    assert!(seen >= 10, "only {seen} goldens seen");

    let sweep = arrix_eval(&["tests/docs/slice", "--sweep", "w=20 mm..60 mm:3"]);
    let text = String::from_utf8_lossy(&sweep.stdout).into_owned();
    assert!(
        text.lines().count() >= 3,
        "{text}{}",
        String::from_utf8_lossy(&sweep.stderr)
    );
    assert_valid(&validator, &text, "a sweep");

    let timed = arrix_eval(&[
        "tests/docs/empty",
        "tests/docs/standoff",
        "--timings",
        "--plugin",
        "python/examples/standoff",
    ]);
    let text = String::from_utf8_lossy(&timed.stdout).into_owned();
    assert!(
        text.contains("\"timings\""),
        "{text}{}",
        String::from_utf8_lossy(&timed.stderr)
    );
    assert_valid(&validator, &text, "timings");
}

#[test]
fn a_line_without_a_format_is_not_valid() {
    let validator = jsonschema::validator_for(&schema()).expect("a valid schema");
    let line = serde_json::json!({"doc": "x", "sweep": null, "status": "ok",
        "params": [], "features": [], "bodies": []});
    assert!(!validator.is_valid(&line));
}
