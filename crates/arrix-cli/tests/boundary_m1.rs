//! M1's accept (plans/boundary-m1 step 9): the Python standoff of
//! `python/examples/standoff`, a Tier 2 plugin, on `tests/docs/standoff`.
//! Evaluated through Tier 2 and followed through an upstream edit by the
//! fillet that names its edges; saved with its frozen result, twice to the
//! same bytes; opened without the plugin (frozen with its reason, the
//! fillet live, stale after an edit of its own, re-frozen on return); a
//! crash, a timeout and a kernel failure inside the Python feature each a
//! diagnostic, the last with a replayable recipe.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use arrix_core::FeatureId;
use arrix_doc::{
    Command, Document, EvalLine, EvalStatus, Evaluator, FeatureEdit, FeatureLine, MemorySource,
    Registry, apply, expr::Expr, open, save,
};
use arrix_plugin_host::register_tier2;
use common::{copy_tree, read_tree, workspace};

const PLUGIN: &str = "python/examples/standoff";
const DOC: &str = "tests/docs/standoff";

struct Standoff {
    doc: Document,
    plate: FeatureId,
    standoff: FeatureId,
    fillet: FeatureId,
}

/// The scenario document as it is on disk, its features found by name.
fn scenario() -> Standoff {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join(DOC), "", &mut files);
    let doc = open(&MemorySource(files)).unwrap();
    let named = |name: &str| {
        doc.parts()
            .values()
            .flat_map(|p| p.features.values())
            .find(|f| f.name == name)
            .unwrap_or_else(|| panic!("no feature {name}"))
            .id
    };
    Standoff {
        plate: named("Plate"),
        standoff: named("Standoff"),
        fillet: named("Fillet"),
        doc,
    }
}

/// The core types and the standoff plugin, started as a child process.
fn with_plugin() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier2(&mut r, &workspace().join(PLUGIN)).expect("the standoff plugin starts");
    r
}

fn line(r: Registry, doc: &Document) -> EvalLine {
    Evaluator::new(r).evaluate(doc).line("m1", doc)
}

fn feature(line: &EvalLine, id: FeatureId) -> &FeatureLine {
    line.features.iter().find(|f| f.id == id).unwrap()
}

fn volume(line: &EvalLine, id: FeatureId) -> f64 {
    line.bodies
        .iter()
        .find(|b| b.feature == id)
        .unwrap_or_else(|| panic!("no body line for {id}"))
        .volume
}

fn edit_param(doc: &Document, name: &str, expr: &str) -> Document {
    let (param, _) = doc.param_by_name(name).expect("the parameter");
    let command = Command::SetParam {
        param,
        expr: Expr::parse(expr).unwrap(),
    };
    apply(doc, &command).unwrap().document
}

/// `feature`'s parameter `name` set to `expr`.
fn edit_feature(doc: &Document, feature: FeatureId, name: &str, expr: &str) -> Document {
    let edit = FeatureEdit {
        params: [(name.into(), Some(Expr::parse(expr).unwrap()))].into(),
        ..FeatureEdit::default()
    };
    apply(doc, &Command::EditFeature { feature, edit })
        .unwrap()
        .document
}

/// `doc` saved as the app saves it: every plugin feature's frozen result in.
fn saved(doc: &Document) -> MemorySource {
    let mut doc = doc.clone();
    let mut e = Evaluator::new(with_plugin());
    e.evaluate(&doc);
    assert!(!e.frozen().is_empty(), "the standoff is frozen");
    for (feature, frozen) in e.frozen() {
        assert!(doc.freeze(*feature, frozen));
    }
    save(&doc)
}

fn no_stale(line: &EvalLine) -> bool {
    line.features.iter().all(|f| {
        f.diagnostics
            .iter()
            .all(|d| d.code.as_str() != "plugin.frozen.stale")
    })
}

/// The plate's width edited, the fillet that names the standoff's edges
/// follows it: nothing lost, every body ok.
#[test]
fn the_standoff_follows_an_upstream_edit_with_no_reference_lost() {
    let s = scenario();
    let live = line(with_plugin(), &s.doc);
    assert_eq!(live.status, EvalStatus::Ok);
    assert!(live.features.iter().all(|f| f.status == "ok"), "{live:?}");

    let wider = edit_param(&s.doc, "w", "70 mm");
    let edited = line(with_plugin(), &wider);
    assert_eq!(edited.status, EvalStatus::Ok);
    for f in &edited.features {
        assert_eq!(f.status, "ok", "{}: {:?}", f.id, f.diagnostic);
        assert!(f.diagnostics.is_empty(), "{}: {:?}", f.id, f.diagnostics);
    }
    // The plate grew 20 mm × 30 mm × 5 mm; the standoff on it did not change.
    let grown = 20e-3 * 30e-3 * 5e-3;
    let by = volume(&edited, s.plate) - volume(&live, s.plate);
    assert_eq!(volume(&edited, s.fillet), volume(&live, s.fillet));
    assert!((by - grown).abs() < 1e-3 * grown, "{by} against {grown}");
}

/// Saved twice from two plugin processes, identical bytes; opened without
/// the plugin the standoff is frozen with its reason and the fillet is live.
#[test]
fn saved_it_opens_without_the_plugin_frozen_and_goes_stale_and_back() {
    let s = scenario();
    let files = saved(&s.doc);
    assert_eq!(files.0, saved(&s.doc).0, "two plugin processes, one save");
    let doc = open(&files).unwrap();
    assert!(doc.feature(s.standoff).unwrap().2.frozen.is_some());
    assert!(doc.feature(s.fillet).unwrap().2.frozen.is_none());
    assert_eq!(save(&doc).0, files.0, "it round-trips");

    let live = line(with_plugin(), &doc);
    let core = Registry::with_core_types;
    let without = line(core(), &doc);
    assert_eq!(without.status, EvalStatus::Ok);
    let frozen = feature(&without, s.standoff);
    assert_eq!(frozen.status, "frozen");
    assert_eq!(frozen.category.as_deref(), Some("plugin.frozen.missing"));
    assert_eq!(feature(&without, s.fillet).status, "ok");
    for b in &live.bodies {
        assert_eq!(volume(&without, b.feature), b.volume, "{}", b.feature);
    }

    // The plate's width edited without the plugin: the standoff's plane does
    // not move, so it stays frozen and the fillet follows.
    let wider = edit_param(&doc, "w", "70 mm");
    let edit = line(core(), &wider);
    assert_eq!(edit.status, EvalStatus::Ok);
    assert_eq!(feature(&edit, s.standoff).status, "frozen");
    assert!(volume(&edit, s.plate) > volume(&without, s.plate));
    assert_eq!(volume(&edit, s.fillet), volume(&without, s.fillet));

    // The standoff's own parameter edited: its stored result is used, the
    // fillet on its edges still evaluates, and the standoff is stale.
    let taller = edit_feature(&doc, s.standoff, "height", "12 mm");
    let stale = line(core(), &taller);
    assert_eq!(stale.status, EvalStatus::Ok);
    assert_eq!(feature(&stale, s.standoff).status, "frozen-stale");
    assert_eq!(feature(&stale, s.fillet).status, "ok");
    assert_eq!(volume(&stale, s.fillet), volume(&without, s.fillet));
    assert!(!no_stale(&stale));

    // The plugin back: live at the new height, frozen anew, nothing stale.
    let mut e = Evaluator::new(with_plugin());
    let back = e.evaluate(&taller).line("back", &taller);
    assert_eq!(feature(&back, s.standoff).status, "ok");
    assert!(e.frozen().contains_key(&s.standoff));
    assert!(no_stale(&back));
    assert!(volume(&back, s.standoff) > volume(&without, s.standoff));
}

fn arrix(dir: &Path, args: &[&str]) -> std::process::Output {
    std::process::Command::new(env!("CARGO_BIN_EXE_arrix"))
        .current_dir(dir)
        .args(args)
        .output()
        .expect("run arrix")
}

/// A scratch directory holding the scenario, and a copy of the standoff
/// plugin whose module `standoff` imports the real one and then misbehaves
/// as `misbehaviour` (Python statements in the feature's body), the
/// manifest naming `timeout` seconds. Returns the root and the plugin dir.
fn faulty_plugin(name: &str, timeout: &str, misbehaviour: &str) -> (PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("boundary-m1")
        .join(name);
    let _ = std::fs::remove_dir_all(&root);
    copy_tree(&workspace().join(DOC), &root.join("doc"));
    let plugin = root.join("plugin");
    copy_tree(&workspace().join(PLUGIN), &plugin);
    let _ = std::fs::remove_dir_all(plugin.join("__pycache__"));
    std::fs::rename(plugin.join("standoff.py"), plugin.join("real.py")).unwrap();
    std::fs::write(
        plugin.join("standoff.py"),
        format!(
            "import os, time\nfrom arrix import feature\nimport real\n\
             t = feature._REGISTRY['standoff.standoff']\n\
             def fn(kernel, args):\n    {misbehaviour}\n    return t.fn(kernel, args)\n\
             feature._REGISTRY['standoff.standoff'] = feature._Type(t.spec, fn)\n"
        ),
    )
    .unwrap();
    let manifest = std::fs::read_to_string(plugin.join("arrix-plugin.toml")).unwrap();
    let manifest = manifest.replace("[tier2]\n", &format!("[tier2]\n{timeout}\n"));
    std::fs::write(plugin.join("arrix-plugin.toml"), manifest).unwrap();
    (root, plugin)
}

/// A crash or a hang inside the plugin is the standoff's diagnostic: the
/// evaluation ends and reports the category.
fn fails_as(name: &str, timeout: &str, misbehaviour: &str, category: &str) {
    let (root, plugin) = faulty_plugin(name, timeout, misbehaviour);
    let out = arrix(
        &root,
        &["eval", "--plugin", plugin.to_str().unwrap(), "doc"],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    let line: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(line["status"], "failed");
    let standoff = line["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["type"] == "standoff.standoff")
        .unwrap();
    assert_eq!(standoff["status"], "failed", "{line}");
    assert_eq!(standoff["category"], category, "{line}");
}

#[test]
fn a_crash_in_the_plugin_is_the_features_diagnostic() {
    fails_as("crash", "", "os._exit(3)", "plugin.crash");
}

#[test]
fn a_timeout_in_the_plugin_is_the_features_diagnostic() {
    fails_as(
        "timeout",
        "timeout-seconds = 3",
        "time.sleep(60)",
        "plugin.timeout",
    );
}

/// A fillet no standoff holds fails inside the Python feature as the kernel
/// does for a built-in: its category, and a recipe replaying the calls.
#[test]
fn a_kernel_failure_inside_the_python_feature_leaves_a_recipe() {
    let s = scenario();
    let doc = edit_feature(&s.doc, s.standoff, "round", "30 mm");
    let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("boundary-m1")
        .join("recipe");
    let _ = std::fs::remove_dir_all(&root);
    for (path, bytes) in &save(&doc).0 {
        let to = root.join("doc").join(path);
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(to, bytes).unwrap();
    }
    let plugin = workspace().join(PLUGIN);
    let out = arrix(
        &root,
        &[
            "eval",
            "--plugin",
            plugin.to_str().unwrap(),
            "doc",
            "--save-fixtures",
            "recipes",
        ],
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{stderr}");
    let line: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let failed = line["features"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["id"] == s.standoff.to_string())
        .unwrap();
    assert_eq!(failed["status"], "failed", "{line}");
    let category = failed["category"].as_str().unwrap();
    assert!(category.starts_with("kernel."), "{category}");

    let recipes: Vec<_> = std::fs::read_dir(root.join("recipes"))
        .expect("a recipe was written")
        .map(|e| e.unwrap().path())
        .collect();
    let [recipe] = recipes.as_slice() else {
        panic!("{recipes:?}")
    };
    assert!(
        recipe
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with(category),
        "{recipe:?}"
    );
    let json = std::fs::read_to_string(recipe.join("fixture.json")).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    // The Python feature's calls, replayed: the prism, the hole's tool, the
    // cut, then the fillet that failed, of 30 mm.
    let steps = value["steps"].as_array().unwrap();
    let ops: Vec<_> = steps.iter().map(|s| s["op"].as_str().unwrap()).collect();
    assert!(ops.contains(&"cut"), "{ops:?}");
    let last = steps.last().unwrap();
    assert_eq!(last["op"], "fillet", "{json}");
    assert_eq!(value["result"], last["name"]);
    assert_eq!(last["radius"], 0.03);
}
