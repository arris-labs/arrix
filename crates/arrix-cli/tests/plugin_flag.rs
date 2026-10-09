//! `--plugin <dir>` of `arrix eval` and `arrix describe` (plans/boundary-m1
//! step 8): a Tier 2 plugin started from a directory holding an
//! `arrix-plugin.toml`, the sample standoff against `tests/docs/standoff`.
//! The Python SDK is found on `PYTHONPATH` (`.cargo/config.toml`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const PLUGIN: &str = "python/examples/standoff";
const DOC: &str = "tests/docs/standoff";
const GOLDEN: &str = "tests/golden/eval_standoff.jsonl";

fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn arrix(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .args(args)
        .output()
        .expect("run arrix")
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout.clone()).unwrap()
}

#[test]
fn a_python_plugin_evaluates_through_tier2_to_its_golden_line_twice_the_same() {
    let first = stdout(&arrix(&["eval", "--plugin", PLUGIN, DOC]));
    let second = stdout(&arrix(&["eval", "--plugin", PLUGIN, DOC]));
    assert_eq!(first, second, "two runs, two plugin processes");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&golden, &first).unwrap();
    }
    assert_eq!(first, std::fs::read_to_string(golden).unwrap());
    let line: serde_json::Value = serde_json::from_str(first.trim()).unwrap();
    assert_eq!(line["status"], "ok");
    let types: Vec<_> = line["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        types,
        [
            "core.sketch",
            "core.extrude",
            "standoff.standoff",
            "core.fillet"
        ]
    );
}

#[test]
fn describe_lists_the_plugin_how_it_runs_and_its_feature_type() {
    let d: serde_json::Value =
        serde_json::from_str(&stdout(&arrix(&["describe", "--json", "--plugin", PLUGIN]))).unwrap();
    let plugin = d["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "standoff")
        .expect("listed");
    assert_eq!(plugin["tier"], 2);
    assert_eq!(
        plugin["command"],
        serde_json::json!(["python3", "-m", "arrix.plugin", "standoff"])
    );
    assert_eq!(plugin["timeout_seconds"], 30.0);
    assert_eq!(plugin["capabilities"]["kernel"], true);
    assert_eq!(plugin["capabilities"]["network"], false);
    let types = serde_json::to_string(&d["feature_types"]).unwrap();
    assert!(types.contains("standoff.standoff"), "{types}");
    let text = stdout(&arrix(&["describe", "--plugin", PLUGIN]));
    assert!(text.contains("\"standoff\" \"0.1.0\" (tier 2)"), "{text}");
}

#[test]
fn a_directory_that_is_no_plugin_is_a_usage_error() {
    for (dir, says) in [
        (
            "tests/docs/no-such-plugin",
            "--plugin tests/docs/no-such-plugin",
        ),
        ("tests/docs/empty", "arrix-plugin.toml"),
    ] {
        let out = arrix(&["eval", "--plugin", dir, "tests/docs/empty"]);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{stderr}");
        assert!(out.stdout.is_empty());
        assert!(stderr.contains(says), "{stderr:?} does not say {says:?}");
    }
}
