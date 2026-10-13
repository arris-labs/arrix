//! C3 M1's accept (plans/c3-m1, ADR-0034, ADR-0035): `gears` built from its
//! own source as a component and loaded from a tier-1 plugin directory by
//! `--plugin` gives the eval line and saved bytes it has compiled in, to the
//! bit, on `gear-on-plane` and the bracket, and a document saved at either
//! tier opens live at the other and frozen with neither; `arrix test`
//! passes it at Tier 1; a plugin that loops, panics, grows its memory,
//! calls a kernel it was not granted or imports beyond the world ends as
//! that feature's diagnostic with the rest evaluating, the loop's on fuel
//! and the same on two runs; `arrix serve` loads a component and reloads
//! it after a rebuild.

mod common;

use std::collections::BTreeMap;
use std::process::{Command, Output};

use arrix_doc::{Document, Evaluator, MemorySource, Registry, open, save};
use arrix_plugin_host::register_tier1;
use common::tier1::{self, build};
use common::{read_tree, registry, workspace};
use serde_json::{Value, json};

const DOCS: [&str; 2] = ["gear-on-plane", "bracket"];

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

fn files(name: &str) -> MemorySource {
    let mut tree = BTreeMap::new();
    read_tree(&workspace().join("tests/docs").join(name), "", &mut tree);
    MemorySource(tree)
}

fn tier1_registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier1(&mut r, build::gears()).expect("gears loads as a component");
    r
}

/// One evaluation of `doc`: its eval line as JSON, and the document saved
/// with its frozen results, as a session saves it.
fn run(registry: Registry, name: &str, doc: &Document) -> (Value, MemorySource) {
    let mut evaluator = Evaluator::new(registry);
    let line = evaluator.evaluate(doc).line(name, doc);
    let mut frozen = doc.clone();
    for (id, result) in evaluator.frozen() {
        assert!(frozen.freeze(*id, result));
    }
    (serde_json::to_value(line).unwrap(), save(&frozen))
}

fn gear_status(line: &Value) -> String {
    line["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["type"] == "gears.spur")
        .map(|f| f["status"].as_str().unwrap().to_owned())
        .unwrap()
}

#[test]
fn gears_at_tier_1_gives_the_eval_line_and_saved_bytes_it_has_compiled_in() {
    for name in DOCS {
        let doc = open(&files(name)).unwrap();
        let (native, native_saved) = run(registry(), name, &doc);
        let (component, component_saved) = run(tier1_registry(), name, &doc);
        assert_eq!(gear_status(&native), "ok", "{name}");
        assert_eq!(native, component, "{name}: the eval line");
        assert_eq!(native_saved.0, component_saved.0, "{name}: saved bytes");

        // `arrix eval --plugin` prints that line.
        let dir = build::gears();
        let doc_path = format!("tests/docs/{name}");
        let at_tier1 = stdout(&arrix(&[
            "eval",
            "--plugin",
            dir.to_str().unwrap(),
            &doc_path,
        ]));
        assert_eq!(at_tier1, stdout(&arrix(&["eval", &doc_path])), "{name}");

        // Saved at either tier, opened at the other: live; with neither:
        // frozen, not stale.
        for (at, saved) in [("tier 0", &native_saved), ("tier 1", &component_saved)] {
            let reopened = open(saved).unwrap();
            for (other, registry) in [("tier 0", registry()), ("tier 1", tier1_registry())] {
                let (line, _) = run(registry, name, &reopened);
                assert_eq!(line, native, "{name} saved at {at}, opened at {other}");
            }
            let (line, _) = run(Registry::with_core_types(), name, &reopened);
            assert_eq!(gear_status(&line), "frozen", "{name} saved at {at}");
        }
    }
}

#[test]
fn the_test_kit_passes_gears_at_tier_1() {
    let dir = tier1::gears();
    let out = arrix(&["test", dir.to_str().unwrap()]);
    let report = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{report}");
    let lines: Vec<Value> = report
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(lines.len() >= 5, "{report}");
    assert!(lines.iter().all(|l| l["status"] == "pass"), "{report}");
}

const FEATURES: [(&str, &str); 5] = [
    ("1000000000001", "misbehave.fine"),
    ("1000000000002", "misbehave.loop"),
    ("1000000000003", "misbehave.panic"),
    ("1000000000004", "misbehave.memory"),
    ("1000000000005", "misbehave.kernel"),
];

/// The diagnostic each misbehaving feature of `plugin` ends with in
/// `arrix eval`'s line, `None` for one that evaluated.
fn misbehave(plugin: &std::path::Path) -> Vec<Option<String>> {
    let doc = common::tier1::document_of(&FEATURES);
    let (code, out) = common::arrix_eval(
        &format!(
            "misbehave-{}",
            plugin.file_name().unwrap().to_string_lossy()
        ),
        &doc,
        &["--plugin", plugin.to_str().unwrap()],
    );
    assert_eq!(code, Some(1), "a failed feature is exit 1: {out}");
    let line: Value = serde_json::from_str(out.trim()).unwrap();
    line["features"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| match f["status"].as_str().unwrap() {
            "ok" => None,
            _ => Some(f["diagnostic"]["code"].as_str().unwrap().to_owned()),
        })
        .collect()
}

#[test]
fn a_misbehaving_plugin_ends_as_diagnostics_and_the_rest_evaluates() {
    let dir = build::misbehave("misbehave", 20_000_000, true);
    let (first, second) = (misbehave(&dir), misbehave(&dir));
    assert_eq!(
        first,
        [
            None,
            Some("plugin.fuel".into()),
            Some("plugin.trap".into()),
            Some("plugin.memory".into()),
            None,
        ]
    );
    assert_eq!(first, second, "the loop ends on its fuel both times");

    let denied = build::misbehave("misbehave-no-kernel", 20_000_000, false);
    let outcomes = misbehave(&denied);
    assert_eq!(outcomes[0].as_deref(), Some("plugin.capability"));
    assert_eq!(outcomes[4].as_deref(), Some("plugin.capability"));
    assert_eq!(outcomes[1].as_deref(), Some("plugin.fuel"));
}

/// A component whose only import is `wasi:clocks/monotonic-clock`.
fn wasi_component() -> Vec<u8> {
    use wasm_encoder::{
        Component, ComponentImportSection, ComponentTypeRef, ComponentTypeSection, InstanceType,
    };
    let mut types = ComponentTypeSection::new();
    types.instance(&InstanceType::new());
    let mut imports = ComponentImportSection::new();
    imports.import(
        "wasi:clocks/monotonic-clock@0.2.0",
        ComponentTypeRef::Instance(0),
    );
    let mut component = Component::new();
    component.section(&types).section(&imports);
    component.finish()
}

#[test]
fn a_foreign_import_is_refused_at_load() {
    let manifest = std::fs::read_to_string(
        build::root().join("crates/arrix-plugin-host/tests/plugins/misbehave/arrix-plugin.toml"),
    )
    .unwrap();
    let dir = build::plugin_dir("misbehave-wasi", &manifest, &wasi_component());
    let out = arrix(&[
        "eval",
        "--plugin",
        dir.to_str().unwrap(),
        "tests/docs/empty",
    ]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(2), "{stderr}");
    assert!(
        stderr.contains("wasi:clocks/monotonic-clock@0.2.0")
            && stderr.contains("outside the world"),
        "{stderr}"
    );
}

#[test]
fn serve_loads_a_component_and_reloads_it_after_a_rebuild() {
    use common::serve_client::Serve;
    let doc = tier1::height_document_dir("accept");
    let plugin = tier1::copy_plugin(&build::height("height", false), "accept");
    let mut s = Serve::spawn_with(&[doc.to_str().unwrap()]);
    let hello = s.call("initialize", json!({"protocol": 1}));
    assert_eq!(hello["result"]["protocol"], 1, "{hello}");
    let volume = |s: &mut Serve, generation: u64| {
        let e = s.hear_event(|e| e["kind"] == "evaluated" && e["generation"] == generation);
        e["event"]["outcome"]["slots"]["body"]["body"]["volume"].as_f64()
    };
    assert_eq!(volume(&mut s, 0), None, "frozen without the plugin");

    let loaded = s.session("load_plugin", json!({"dir": plugin.to_str().unwrap()}));
    assert_eq!(loaded["ok"]["loaded"]["generation"], 1, "{loaded}");
    let before = volume(&mut s, 1).expect("live once loaded");

    std::fs::write(plugin.join("height.wasm"), build::height_module(true)).unwrap();
    let reloaded = s.session("reload_plugin", json!({"plugin": "height"}));
    assert_eq!(reloaded["ok"]["loaded"]["generation"], 2, "{reloaded}");
    let after = volume(&mut s, 2).expect("live after the reload");
    assert!(
        after > before * 2.0,
        "{before} then {after}: the new code ran"
    );
    let _ = std::fs::remove_dir_all(&plugin);
    let _ = std::fs::remove_dir_all(&doc);
}
