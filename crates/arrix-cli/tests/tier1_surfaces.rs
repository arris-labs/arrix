//! `--plugin` loads a plugin by the tier its manifest names, on every
//! surface of the CLI (plans/c3-m1 step 10, ADR-0035): `arrix eval`,
//! `describe`, `test` and `serve`, with `gears` built as a component and the
//! `height` test plugin for a reload. `arrix-app render --plugin` is
//! tested in that package.

mod common;

use std::process::{Command, Output};

use common::serve_client::Serve;
use common::tier1::{self, build};
use common::workspace;
use serde_json::{Value, json};

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
fn eval_with_a_tier_1_gears_gives_the_line_the_compiled_in_one_does() {
    let dir = build::gears();
    let at_tier1 = stdout(&arrix(&[
        "eval",
        "--plugin",
        dir.to_str().unwrap(),
        "tests/docs/gear-on-plane",
    ]));
    let at_tier0 = stdout(&arrix(&["eval", "tests/docs/gear-on-plane"]));
    assert_eq!(at_tier1, at_tier0);
    let line: Value = serde_json::from_str(at_tier1.trim()).unwrap();
    assert_eq!(line["status"], "ok", "{line}");
}

#[test]
fn describe_lists_the_component_and_its_fuel_in_place_of_the_compiled_in_plugin() {
    let dir = build::gears();
    let d: Value = serde_json::from_str(&stdout(&arrix(&[
        "describe",
        "--json",
        "--plugin",
        dir.to_str().unwrap(),
    ])))
    .unwrap();
    let gears: Vec<_> = d["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["id"] == "gears")
        .collect();
    assert_eq!(gears.len(), 1, "one gears, the directory's: {gears:?}");
    assert_eq!(gears[0]["tier"], 1);
    assert_eq!(gears[0]["component"], "gears.wasm");
    assert_eq!(gears[0]["fuel"], 1u64 << 30);
    assert_eq!(gears[0]["capabilities"]["kernel"], true);
    assert!(d["feature_types"].to_string().contains("gears.spur"));
}

#[test]
fn the_test_kit_passes_gears_at_tier_1() {
    let dir = tier1::gears();
    let out = arrix(&["test", dir.to_str().unwrap()]);
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(
        out.status.success(),
        "{report}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let lines: Vec<Value> = report
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    for name in ["spec", "determinism", "round_trip", "frozen", "migration"] {
        let line = lines.iter().find(|l| l["check"] == name);
        assert_eq!(line.map(|l| &l["status"]), Some(&json!("pass")), "{report}");
    }
    assert!(lines.iter().all(|l| l["status"] == "pass"), "{report}");
}

/// The block's volume in the evaluation of `generation`, once heard.
fn volume(s: &mut Serve, generation: u64) -> Option<f64> {
    let event = s.hear_event(|e| e["kind"] == "evaluated" && e["generation"] == generation);
    event["event"]["outcome"]["slots"]["body"]["body"]["volume"].as_f64()
}

fn opened(doc: &std::path::Path, extra: &[&str]) -> Serve {
    let mut args = vec![doc.to_str().unwrap()];
    args.extend_from_slice(extra);
    let mut s = Serve::spawn_with(&args);
    let hello = s.call("initialize", json!({"protocol": 1}));
    assert_eq!(hello["result"]["protocol"], 1, "{hello}");
    s
}

#[test]
fn serve_loads_a_component_and_reloads_it_after_a_rebuild() {
    let doc = tier1::height_document_dir("serve");
    let plugin = tier1::copy_plugin(&build::height("height", false), "height");

    // Opened without the plugin the block is frozen; loaded, it is live.
    let mut s = opened(&doc, &[]);
    assert_eq!(volume(&mut s, 0), None);
    let loaded = s.session("load_plugin", json!({"dir": plugin.to_str().unwrap()}));
    assert_eq!(loaded["ok"]["loaded"]["generation"], 1, "{loaded}");
    let before = volume(&mut s, 1).expect("live once loaded");
    assert!((before - 5e-7).abs() < 1e-12, "{before}");

    // A rebuilt component replaces the file; a reload runs the new code
    // with no rebuild of ArriX.
    std::fs::write(plugin.join("height.wasm"), build::height_module(true)).unwrap();
    let reloaded = s.session("reload_plugin", json!({"plugin": "height"}));
    assert_eq!(reloaded["ok"]["loaded"]["generation"], 2, "{reloaded}");
    let after = volume(&mut s, 2).expect("live after the reload");
    assert!((after - 1.25e-6).abs() < 1e-12, "{after}");

    // `--plugin` starts one with the session.
    let mut started = opened(&doc, &["--plugin", plugin.to_str().unwrap()]);
    let at_start = volume(&mut started, 0).expect("live from the start");
    assert!((at_start - after).abs() < 1e-12, "{at_start}");
    let _ = std::fs::remove_dir_all(&plugin);
    let _ = std::fs::remove_dir_all(&doc);
}

#[test]
fn a_broken_component_is_a_plugin_load_error_and_the_session_goes_on() {
    let doc = tier1::height_document_dir("broken");
    let plugin = tier1::copy_plugin(&build::height("height-broken", false), "broken");
    std::fs::write(plugin.join("height-broken.wasm"), b"not wasm").unwrap();
    let mut s = opened(&doc, &[]);
    let refused = s.session("load_plugin", json!({"dir": plugin.to_str().unwrap()}));
    assert!(refused["err"]["plugin_load"].is_object(), "{refused}");
    let _ = std::fs::remove_dir_all(&plugin);
    let _ = std::fs::remove_dir_all(&doc);
}
