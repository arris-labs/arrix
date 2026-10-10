//! Plugins in `arrix serve` (plans/boundary-m2 step 8), on the spawned
//! binary: `tests/docs/standoff` opened without its plugin is frozen, then
//! loaded by `session.load_plugin` and live; the plugin's source edited in
//! a temporary copy and reloaded, the new height evaluated; a bad
//! directory is `PluginLoad`, and `--plugin` starts one with the session.

mod common;

use common::serve_client::Serve;
use common::{copy_tree, workspace};
use serde_json::{Value, json};

const PLUGIN: &str = "python/examples/standoff";
const DOC: &str = "tests/docs/standoff";

fn doc() -> String {
    workspace().join(DOC).display().to_string()
}

fn plugin() -> String {
    workspace().join(PLUGIN).display().to_string()
}

fn opened(args: &[&str]) -> Serve {
    let mut s = Serve::spawn_with(args);
    let hello = s.call("initialize", json!({"protocol": 1}));
    assert_eq!(hello["result"]["protocol"], 1, "{hello}");
    s
}

const STANDOFF: &str = "9HN9VBVEXFYYQ";

/// The standoff's outcome in the evaluation of `generation`, once heard.
fn standoff(s: &mut Serve, generation: u64) -> Value {
    let event = s.hear_event(|e| {
        e["kind"] == "evaluated"
            && e["generation"] == generation
            && e["event"]["feature"] == STANDOFF
    });
    event["event"]["outcome"].clone()
}

fn volume(outcome: &Value) -> f64 {
    outcome["slots"]["body"]["body"]["volume"].as_f64().unwrap()
}

#[test]
fn a_frozen_feature_is_live_once_its_plugin_is_loaded() {
    let mut s = opened(&[&doc()]);
    let frozen = standoff(&mut s, 0);
    assert_ne!(frozen["status"], "ok", "{frozen}");

    let loaded = s.session("load_plugin", json!({"dir": plugin()}));
    assert_eq!(loaded["ok"]["loaded"]["generation"], 1, "{loaded}");
    let registry = s.hear_event(|e| e["kind"] == "registry");
    assert_eq!(registry["generation"], 1);
    assert_eq!(registry["added"][0]["id"], "standoff.standoff");

    let live = standoff(&mut s, 1);
    assert_eq!(live["status"], "ok", "{live}");
    assert!(volume(&live) > 0.0);
    // The fillet downstream follows the live body.
    s.hear_event(|e| e["kind"] == "finished" && e["generation"] == 1);
}

#[test]
fn a_reload_after_an_edit_evaluates_the_new_height() {
    let dir = std::env::temp_dir().join(format!("arrix-serve-plugins-{}", std::process::id()));
    let copy = dir.join("standoff");
    copy_tree(&workspace().join(PLUGIN), &copy);
    let mut s = opened(&[&doc(), "--plugin", copy.to_str().unwrap()]);
    let before = standoff(&mut s, 0);
    assert_eq!(
        before["status"], "ok",
        "--plugin starts it with the session"
    );

    let _ = std::fs::remove_dir_all(copy.join("__pycache__"));
    let source = copy.join("standoff.py");
    let text = std::fs::read_to_string(&source).unwrap();
    assert!(text.contains(r#"default="10 mm""#));
    std::fs::write(
        &source,
        text.replace(r#"default="10 mm""#, r#"default="12.5 mm""#),
    )
    .unwrap();

    let reloaded = s.session("reload_plugin", json!({"plugin": "standoff"}));
    assert_eq!(reloaded["ok"]["loaded"]["generation"], 1, "{reloaded}");
    let after = standoff(&mut s, 1);
    assert_eq!(after["status"], "ok", "{after}");
    assert!(volume(&after) > volume(&before) * 1.1, "{before} {after}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_directory_and_an_unknown_plugin_are_plugin_load_errors() {
    let mut s = opened(&[]);
    let nowhere = s.session("load_plugin", json!({"dir": "/nonexistent/plugin"}));
    assert!(nowhere["err"]["plugin_load"].is_object(), "{nowhere}");
    let unknown = s.session("reload_plugin", json!({"plugin": "standoff"}));
    assert!(unknown["err"]["plugin_load"].is_object(), "{unknown}");
    // Nothing was loaded: the next generation is still 0.
    let undo = s.session("undo", json!({"author": "0000000000001"}));
    assert_eq!(undo["err"], "nothing_to_undo");
}
