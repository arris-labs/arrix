//! `arrix-app render`'s surface (ADR-0025): each flag on the spawned
//! binary, the JSON it writes, and the failures it reports with a non-zero
//! exit.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

use crate::harness;

/// The repository root: the binary is run from there, as an agent would.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A fresh directory for one test's outputs.
fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("arrix-render-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    dir
}

/// `arrix-app render <args>` from the repository root.
fn render(args: &[&str]) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_arrix-app"));
    cmd.current_dir(root()).arg("render").args(args);
    cmd
}

/// Runs `cmd`, requires success, and reads the JSON it wrote to `json`.
fn rendered(mut cmd: Command, json: &Path) -> Value {
    let out = cmd.output().expect("arrix-app runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = std::fs::read_to_string(json).expect("the JSON is written");
    let value: Value = serde_json::from_str(&text).expect("the JSON parses");
    assert_valid(&golden_validator(), &value, "a render");
    value
}

/// The validator of the golden schema, which `the_render_schema_is_fresh…`
/// holds equal to what `--schema` prints: every render here is checked
/// against it.
fn golden_validator() -> jsonschema::Validator {
    let text = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_GOLDEN))
        .expect("the schema golden");
    jsonschema::validator_for(&serde_json::from_str(&text).expect("JSON")).expect("a schema")
}

fn failed(out: &Output) -> String {
    assert!(!out.status.success(), "it should have failed");
    assert!(out.stdout.is_empty(), "nothing on stdout");
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn path(p: &Path) -> &str {
    p.to_str().expect("a UTF-8 temporary path")
}

/// The JSON names its format and the adapter, and `--size` is the
/// window's and the PNG's.
#[test]
fn render_size_sets_the_window_and_the_png() {
    harness::with_gpu("render_size_sets_the_window_and_the_png", || {
        let dir = scratch("size");
        let (png, json) = (dir.join("slice.png"), dir.join("slice.json"));
        let mut cmd = render(&["tests/docs/slice", "--size", "640x400"]);
        cmd.args(["-o", path(&png), "--json", path(&json)]);
        let out = rendered(cmd, &json);
        assert_eq!(out["format"], arrix_app::render::FORMAT);
        assert!(!out["adapter"].as_str().unwrap_or_default().is_empty());
        assert_eq!(
            out["debug_state"]["window"],
            serde_json::json!([640.0, 400.0])
        );
        let image = image::open(&png).expect("the PNG reads");
        assert_eq!((image.width(), image.height()), (640, 400));
    });
}

/// `--view` turns the camera once the document has settled: the app's
/// view commands, from where home left it.
#[test]
fn render_view_turns_the_camera() {
    harness::with_gpu("render_view_turns_the_camera", || {
        let dir = scratch("view");
        let camera = |view: Option<&str>| {
            let (png, json) = (dir.join("v.png"), dir.join("v.json"));
            let mut cmd = render(&["tests/docs/slice", "--size", "320x200"]);
            cmd.args(["-o", path(&png), "--json", path(&json)]);
            if let Some(view) = view {
                cmd.args(["--view", view]);
            }
            let out = rendered(cmd, &json);
            let c = &out["debug_state"]["camera"];
            let angle = |k: &str| c[k].as_f64().expect("an angle");
            (angle("yaw"), angle("pitch"))
        };
        let close =
            |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).abs() < 1e-4 && (a.1 - b.1).abs() < 1e-4;
        let home = camera(None);
        assert!(close(camera(Some("home")), home));
        let top = camera(Some("top"));
        assert!(
            (top.1 - std::f64::consts::FRAC_PI_2).abs() < 1e-4,
            "{top:?}"
        );
        assert!(close(
            camera(Some("front")),
            (std::f64::consts::FRAC_PI_2, 0.0)
        ));
        assert!(close(camera(Some("right")), (0.0, 0.0)));
        let iso = camera(Some("iso"));
        assert!(!close(iso, home) && iso.1 > 0.0, "{iso:?}");
    });
}

/// `--plugin` starts a Tier 2 plugin for the document: the standoff
/// evaluates live and is drawn; without it, it fails and is not.
#[test]
fn render_plugin_evaluates_a_tier2_feature() {
    harness::with_gpu("render_plugin_evaluates_a_tier2_feature", || {
        const STANDOFF: &str = "9HN9VBVEXFYYQ";
        let dir = scratch("plugin");
        let bodies = |plugin: bool| {
            let (png, json) = (dir.join("s.png"), dir.join("s.json"));
            let mut cmd = render(&["tests/docs/standoff", "--size", "320x200"]);
            cmd.args(["-o", path(&png), "--json", path(&json)]);
            if plugin {
                cmd.args(["--plugin", "python/examples/standoff"]);
            }
            let out = rendered(cmd, &json);
            out["debug_state"]["bodies"]
                .as_array()
                .expect("bodies")
                .clone()
        };
        let live = bodies(true);
        let standoff = live
            .iter()
            .find(|b| b["feature"] == STANDOFF)
            .expect("the standoff is drawn");
        // 10 mm tall, on the 5 mm plate.
        let top = standoff["bounds"][5].as_f64().expect("bounds");
        assert!((top - 0.015).abs() < 1e-6, "{standoff}");
        assert!(
            bodies(false).iter().all(|b| b["feature"] != STANDOFF),
            "without the plugin the standoff has no body"
        );
    });
}

/// `--plugin` loads a Tier 1 plugin by its manifest: `gears` built as a
/// component takes the compiled-in one's place and draws the same gear.
#[test]
fn render_plugin_evaluates_a_tier1_feature() {
    #[path = "../../../arrix-plugin-host/tests/support/tier1.rs"]
    mod tier1_build;

    harness::with_gpu("render_plugin_evaluates_a_tier1_feature", || {
        let dir = scratch("tier1");
        let component = tier1_build::gears();
        let bodies = |plugin: bool| {
            let (png, json) = (dir.join("g.png"), dir.join("g.json"));
            let mut cmd = render(&["tests/docs/gear-on-plane", "--size", "320x200"]);
            cmd.args(["-o", path(&png), "--json", path(&json)]);
            if plugin {
                cmd.args(["--plugin", path(component)]);
            }
            rendered(cmd, &json)["debug_state"]["bodies"].clone()
        };
        let at_tier1 = bodies(true);
        assert!(!at_tier1.as_array().expect("bodies").is_empty());
        assert_eq!(at_tier1, bodies(false), "the same gear at either tier");
    });
}

/// `--without-plugin` opens a saved slice as a build without `gears`
/// would: the gear frozen, and its badge in the JSON.
#[test]
fn render_without_plugin_shows_the_frozen_badge() {
    harness::with_app("render_without_plugin_shows_the_frozen_badge", |h| {
        let dir = scratch("without");
        harness::open_settled(h, "tests/docs/slice");
        let saved = h.state().saved().expect("a document is open");
        let arrx = dir.join("slice.arrx");
        std::fs::write(&arrx, arrix_doc::to_zip(&saved).expect("it zips")).expect("written");

        let (png, json) = (dir.join("frozen.png"), dir.join("frozen.json"));
        let mut cmd = render(&[path(&arrx), "--without-plugin", "gears"]);
        cmd.args(["-o", path(&png), "--json", path(&json)]);
        let out = rendered(cmd, &json);
        let badges = out["debug_state"]["badges"].as_array().expect("badges");
        assert_eq!(badges.len(), 1, "{badges:?}");
        assert_eq!(badges[0]["title"], "Spur gear");
        assert_eq!(badges[0]["state"], "frozen");

        let out = render(&[path(&arrx), "--without-plugin", "nosuch", "-o", path(&png)])
            .output()
            .expect("arrix-app runs");
        assert!(failed(&out).contains("no plugin `nosuch`"));
    });
}

/// A document that does not open, a plugin that does not start, and no
/// adapter at all: each exits non-zero with its reason, and writes nothing.
#[test]
fn render_failures_exit_non_zero_with_the_reason() {
    harness::with_gpu("render_failures_exit_non_zero_with_the_reason", || {
        let dir = scratch("failures");
        let png = dir.join("x.png");
        let run = |cmd: &mut Command| cmd.output().expect("arrix-app runs");

        let err = failed(&run(&mut render(&["tests/docs/nosuch", "-o", path(&png)])));
        assert!(err.contains("tests/docs/nosuch"), "{err}");

        let mut cmd = render(&["tests/docs/slice", "--plugin", "tests/docs/nosuch"]);
        let err = failed(&run(cmd.args(["-o", path(&png)])));
        assert!(err.contains("tests/docs/nosuch"), "{err}");

        // No backend is a machine without an adapter.
        let mut cmd = render(&["tests/docs/slice", "-o", path(&png)]);
        let err = failed(&run(cmd.env("WGPU_BACKEND", "none")));
        assert!(err.contains("no wgpu"), "{err}");

        let err = failed(&run(&mut render(&[
            "tests/docs/slice",
            "--size",
            "0x9",
            "-o",
            path(&png),
        ])));
        assert!(err.contains("WxH"), "{err}");
        assert!(!png.exists(), "a failed render writes no PNG");
    });
}

const SCHEMA_GOLDEN: &str = "tests/snapshots/render.schema.json";

/// `render --schema` prints the schema, held to a golden (rewritten under
/// `UPDATE_SNAPSHOTS=1`). It needs no GPU, no document and no `-o`.
fn schema() -> Value {
    let out = render(&["--schema"]).output().expect("arrix-app runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("UTF-8");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join(SCHEMA_GOLDEN);
    if harness::env_flag("UPDATE_SNAPSHOTS") {
        std::fs::write(&golden, &text).expect("the golden is written");
    }
    assert!(
        text == std::fs::read_to_string(&golden).unwrap_or_default(),
        "{SCHEMA_GOLDEN} differs from `arrix-app render --schema`; rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
    serde_json::from_str(&text).expect("JSON")
}

fn assert_valid(validator: &jsonschema::Validator, value: &Value, what: &str) {
    let errors: Vec<String> = validator
        .iter_errors(value)
        .map(|e| e.to_string())
        .collect();
    assert!(errors.is_empty(), "{what}:\n{}", errors.join("\n"));
}

/// The schema is fresh, names the format, and rejects a JSON without one.
#[test]
fn the_render_schema_is_fresh_and_names_the_format() {
    let schema = schema();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(
        schema["properties"]["format"]["const"],
        arrix_app::render::FORMAT
    );
    let validator = jsonschema::validator_for(&schema).expect("a valid schema");
    assert!(!validator.is_valid(&serde_json::json!({"adapter": "x", "debug_state": {}})));
}

/// Every render the CLI makes validates against its schema, and every
/// field the schema marks promised is present in a slice render with a
/// frozen gear (which has bodies, a camera, a badge and an evaluation).
#[test]
fn renders_validate_and_carry_every_promised_field() {
    harness::with_app("renders_validate_and_carry_every_promised_field", |h| {
        let schema = schema();
        let validator = jsonschema::validator_for(&schema).expect("a valid schema");
        let dir = scratch("schema");
        harness::open_settled(h, "tests/docs/slice");
        let saved = h.state().saved().expect("a document is open");
        let arrx = dir.join("slice.arrx");
        std::fs::write(&arrx, arrix_doc::to_zip(&saved).expect("it zips")).expect("written");

        let (png, json) = (dir.join("a.png"), dir.join("a.json"));
        let mut cmd = render(&[
            path(&arrx),
            "--without-plugin",
            "gears",
            "--size",
            "320x200",
        ]);
        cmd.args(["-o", path(&png), "--json", path(&json)]);
        let frozen = rendered(cmd, &json);
        assert_valid(&validator, &frozen, "a frozen slice");

        let mut cmd = render(&["tests/docs/slice", "--size", "320x200"]);
        cmd.args(["-o", path(&png), "--json", path(&json)]);
        assert_valid(&validator, &rendered(cmd, &json), "the slice");

        let promised: Vec<&String> = schema["properties"]["debug_state"]["properties"]
            .as_object()
            .expect("properties")
            .iter()
            .filter(|(_, v)| v["x-arrix-promised"] == true)
            .map(|(k, _)| k)
            .collect();
        assert!(promised.len() >= 5, "{promised:?}");
        for field in promised {
            assert!(
                !frozen["debug_state"][field.as_str()].is_null(),
                "promised field `{field}` is missing from the render"
            );
        }

        let mut cmd = render(&["tests/docs/assembly", "--size", "320x200"]);
        cmd.args(["-o", path(&png), "--json", path(&json)]);
        let assembly = rendered(cmd, &json);
        assert_valid(&validator, &assembly, "the assembly");
        let state = &assembly["debug_state"];
        assert_eq!(state["context"]["kind"], "assembly");
        assert_eq!(state["instances"].as_array().map(Vec::len), Some(2));

        // `--context` looks at a part by name, and refuses an unknown one.
        let mut cmd = render(&[
            "tests/docs/assembly",
            "--context",
            "Bracket",
            "--size",
            "320x200",
        ]);
        cmd.args(["-o", path(&png), "--json", path(&json)]);
        let part = rendered(cmd, &json);
        assert_valid(&validator, &part, "the bracket");
        assert_eq!(part["debug_state"]["context"]["kind"], "part");
        assert!(part["debug_state"]["instances"].is_null());
        let mut cmd = render(&["tests/docs/assembly", "--context", "nothing"]);
        cmd.args(["-o", path(&png)]);
        assert_eq!(cmd.output().expect("it runs").status.code(), Some(1));
    });
}
