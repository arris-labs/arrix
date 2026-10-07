//! The user's configuration against the real expression checker and the
//! compiled-in plugins (docs/UI-RENDERING.md §Configuration): a default
//! for a plugin's parameter, and that config never reaches `evaluate`; and
//! `arrix describe` and `arrix config check` as a user's agent runs them,
//! against the directories in `tests/config/`. The goldens are rewritten
//! with `UPDATE_SNAPSHOTS=1`.

mod common;

use arrix_config::{MemorySource, ParamDefaults, SETTINGS_TOML};
use arrix_doc::expr::check_constant;
use common::{registry, workspace};
use std::process::Output;

const MODULE_SET: &str = "[defaults.\"gears.spur\"]\nmodule = \"1.5 mm\"\n";

fn types() -> Vec<arrix_plugin_api::FeatureTypeSpec> {
    let registry = registry();
    registry
        .ids()
        .filter_map(|id| registry.get(id))
        .map(|t| t.spec().clone())
        .collect()
}

fn defaults(text: &str) -> (ParamDefaults, Vec<arrix_config::ConfigDiagnostic>) {
    ParamDefaults::load(
        &MemorySource::with([(SETTINGS_TOML, text)]),
        &types(),
        &check_constant,
    )
}

fn spur() -> arrix_plugin_api::FeatureTypeSpec {
    types().into_iter().find(|t| t.id == "gears.spur").unwrap()
}

#[test]
fn the_real_checker_accepts_a_length_and_names_what_is_wrong_with_the_rest() {
    let (d, diagnostics) = defaults(MODULE_SET);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let form = d.effective_params(&spur());
    assert!(form.contains(&("module".to_owned(), "1.5 mm".to_owned())));
    assert!(form.contains(&("teeth".to_owned(), "20".to_owned())));

    let (_, diagnostics) = defaults(
        "[defaults.\"gears.spur\"]\nmodule = \"1.5 +\"\nwidth = \"3 deg\"\nteeth = \"2.5\"\n\
         pressure_angle = \"module\"\n",
    );
    let codes: Vec<_> = diagnostics
        .iter()
        .map(|d| (d.diagnostic.code.as_str(), d.line))
        .collect();
    assert_eq!(
        codes,
        [
            ("config.expr.syntax", Some(2)),
            ("config.expr.unit-mismatch", Some(3)),
            ("config.expr.not-whole", Some(4)),
            ("config.expr.unknown-name", Some(5)),
        ]
    );
}

/// A parameter default only seeds a new feature's form. A document's
/// evaluation takes no config, and a type's own spec, which `evaluate`'s
/// inputs are checked against, is the same with or without it.
#[test]
fn no_feature_evaluates_differently_under_a_config() {
    let eval = || {
        let mut files = std::collections::BTreeMap::new();
        common::read_tree(
            &workspace().join("tests/docs/gear-on-plane"),
            "",
            &mut files,
        );
        let source = arrix_doc::MemorySource(files);
        let run = arrix_doc::run("gear-on-plane", &source, &registry()).unwrap();
        serde_json::to_string(&run.line).unwrap()
    };
    let before = eval();
    let (d, diagnostics) = defaults(MODULE_SET);
    assert!(diagnostics.is_empty());
    assert!(d.is_overridden("gears.spur", "module"));
    assert_eq!(eval(), before);
    let own = spur();
    assert_eq!(
        own.params
            .iter()
            .find(|p| p.name == "module")
            .unwrap()
            .default,
        "1 mm"
    );
}

fn arrix(args: &[&str]) -> Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(workspace())
        .env_remove("ARRIX_CONFIG_DIR")
        .args(args)
        .output()
        .expect("run arrix")
}

fn stdout(out: &Output) -> String {
    String::from_utf8(out.stdout.clone()).expect("UTF-8 output")
}

/// `text` is the golden `name`, or becomes it under `UPDATE_SNAPSHOTS=1`.
fn assert_golden(name: &str, text: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&path, text).expect("write the golden");
        return;
    }
    let golden = std::fs::read_to_string(&path).expect("read the golden");
    assert!(
        golden == text,
        "{name} differs: rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
}

#[test]
fn an_empty_directory_checks_clean_and_describes_the_defaults() {
    let check = arrix(&["config", "check", "--dir", "tests/config/empty"]);
    assert_eq!(check.status.code(), Some(0));
    assert!(check.stdout.is_empty() && check.stderr.is_empty());
    let out = arrix(&["describe", "--json", "--dir", "tests/config/empty"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_golden("describe_empty.json", &stdout(&out));
    let d: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(d["schema"], 1);
    assert_eq!(d["diagnostics"], serde_json::json!([]));
}

#[test]
fn a_broken_directory_prints_a_line_per_diagnostic_and_exits_1() {
    let out = arrix(&["config", "check", "--dir", "tests/config/broken"]);
    assert_eq!(out.status.code(), Some(1));
    assert_golden("config_check_broken.jsonl", &stdout(&out));
    // `describe` shows the same diagnostics, and the app would have
    // started on the defaults: the bad keys keep their values.
    let d = arrix(&["describe", "--json", "--dir", "tests/config/broken"]);
    assert_eq!(d.status.code(), Some(0));
    let d: serde_json::Value = serde_json::from_str(&stdout(&d)).unwrap();
    assert_eq!(
        d["diagnostics"].as_array().unwrap().len(),
        stdout(&out).lines().count()
    );
    let hidden = d["settings"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == "ui.ribbon.hidden-tabs");
    assert_eq!(hidden.unwrap()["value"], serde_json::json!([]));
}

#[test]
fn describe_shows_a_users_effective_values() {
    let out = arrix(&["describe", "--json", "--dir", "tests/config/user"]);
    assert_eq!(out.status.code(), Some(0));
    let d: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    let spur = d["feature_types"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "gears.spur");
    let module = spur.unwrap()["params"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "module");
    let module = module.unwrap();
    assert_eq!(
        (module["default"].as_str(), module["effective"].as_str()),
        (Some("1 mm"), Some("1.5 mm"))
    );
    let text = arrix(&["describe", "--dir", "tests/config/user"]);
    assert!(stdout(&text).contains("module"), "{}", stdout(&text));
}

#[test]
fn a_directory_that_is_not_one_exits_2() {
    for args in [
        ["config", "check", "--dir", "tests/config/nope"],
        ["describe", "--dir", "tests/config/nope", ""],
    ] {
        let args: Vec<_> = args.into_iter().filter(|a| !a.is_empty()).collect();
        let out = arrix(&args);
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains("no such directory"));
    }
}

#[test]
fn schema_prints_one_json_schema_per_file_kind() {
    let all = arrix(&["config", "schema"]);
    assert_eq!(all.status.code(), Some(0));
    let all: serde_json::Value = serde_json::from_str(&stdout(&all)).unwrap();
    assert_eq!(all["schema"], 1);
    for kind in ["settings", "keybindings", "theme"] {
        let one = arrix(&["config", "schema", kind]);
        assert_eq!(one.status.code(), Some(0));
        let one: serde_json::Value = serde_json::from_str(&stdout(&one)).unwrap();
        assert_eq!(one, all[kind]);
        assert_eq!(one["type"], "object");
    }
    assert_eq!(arrix(&["config", "schema", "nope"]).status.code(), Some(2));
}

/// The acceptance scenario's directories, as the app's `configured_shell`
/// scenario uses them: the user's files check clean, and the one wrong edit
/// (`tests/config/user-broken`) is the one diagnostic the app reports.
#[test]
fn the_acceptance_directories_check_as_the_app_sees_them() {
    let clean = arrix(&["config", "check", "--dir", "tests/config/user"]);
    assert_eq!(clean.status.code(), Some(0), "{}", stdout(&clean));
    assert!(clean.stdout.is_empty());

    let temp = std::env::temp_dir().join(format!("arrix-cli-user-broken-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&temp);
    common::copy_tree(&workspace().join("tests/config/user"), &temp);
    std::fs::copy(
        workspace().join("tests/config/user-broken/settings.toml"),
        temp.join("settings.toml"),
    )
    .unwrap();
    let broken = arrix(&["config", "check", "--dir", temp.to_str().unwrap()]);
    let _ = std::fs::remove_dir_all(&temp);
    assert_eq!(broken.status.code(), Some(1));
    let lines: Vec<serde_json::Value> = stdout(&broken)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert_eq!(lines[0]["file"], "settings.toml");
    assert_eq!(lines[0]["line"], 4);
    assert_eq!(lines[0]["code"], "config.type");
}

#[test]
fn describe_shows_the_users_rebound_palette_and_theme() {
    let out = arrix(&["describe", "--json", "--dir", "tests/config/user"]);
    let d: serde_json::Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(d["theme"]["effective"], "mine");
    let palette = d["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["id"] == "core.palette.open")
        .unwrap();
    assert_eq!(palette["keys"], serde_json::json!(["ctrl+p"]));
    assert_eq!(palette["default_keys"], serde_json::json!(["ctrl+k"]));
}
