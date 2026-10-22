//! wasm32 evaluates as native does, to the bit (plans/c4 step 2,
//! docs/CONCURRENCY-WASM.md §Determinism): every document under
//! `tests/docs`, opened from bytes and evaluated with this build's Tier 0
//! plugins, prints `arrix eval`'s native golden line, and so does a sweep
//! of the bracket's width, each point an edit on one evaluator. No
//! tolerance: since arris 0.9 the gear's three-point arcs, whose sweeps
//! differed in the last place under 0.8, build the same bits too.
//!
//! Run under Node by `wasm-bindgen-test-runner` (.cargo/config.toml):
//! `cargo test --target wasm32-unknown-unknown -p arrix-plugin-host --test
//! wasm_determinism`. Node's `fs` stands in for the browser's file picker:
//! the evaluator still sees only a `MemorySource`. Here rather than in
//! `arrix-doc`, since the documents use `gears` and this crate registers a
//! Tier 0 plugin.
#![cfg(target_arch = "wasm32")]

use std::sync::Arc;

use arrix_doc::{MemorySource, Registry, Sweep};
use arrix_plugin_host::{register_tier0, register_tier0_exporters};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen(module = "fs")]
extern "C" {
    #[wasm_bindgen(js_name = readFileSync, catch)]
    fn read_file(path: &str) -> Result<Vec<u8>, JsValue>;
    #[wasm_bindgen(js_name = readdirSync, catch)]
    fn read_dir(path: &str) -> Result<js_sys::Array, JsValue>;
}

/// The workspace root, where `tests/docs` and the goldens' paths start.
const ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");

const EVAL_DOCS: &str = include_str!("../../arrix-cli/tests/golden/eval_docs.jsonl");
const BRACKET_SWEEP: &str = include_str!("../../arrix-cli/tests/golden/eval_bracket_sweep.jsonl");

/// The bracket's width sweep, as `tests/sweep.rs` in `arrix-cli` runs it.
const BRACKET_SWEEP_SPEC: &str = "w=40 mm..80 mm:3";

/// `arrix eval`'s registry with no `--plugin`: the core types and the
/// compiled-in plugins (`arrix-cli`'s `plugins::registry`).
fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears)).unwrap();
    register_tier0_exporters(
        &mut r,
        arrix_robotics::MANIFEST,
        Arc::new(arrix_robotics::Robotics),
    )
    .unwrap();
    r
}

/// The files under the document directory `doc` (relative to the root),
/// keyed by their `/`-separated path inside it.
fn source(doc: &str) -> MemorySource {
    fn walk(dir: &str, prefix: &str, out: &mut MemorySource) {
        let names = read_dir(dir).unwrap_or_else(|e| panic!("{dir}: {e:?}"));
        for name in names.iter() {
            let name = name.as_string().expect("a file name");
            let (path, key) = (format!("{dir}/{name}"), format!("{prefix}{name}"));
            match read_dir(&path) {
                Ok(_) => walk(&path, &format!("{key}/"), out),
                Err(_) => {
                    let bytes = read_file(&path).unwrap_or_else(|e| panic!("{path}: {e:?}"));
                    out.0.insert(key, bytes);
                }
            }
        }
    }
    let mut out = MemorySource::default();
    walk(&format!("{ROOT}/{doc}"), "", &mut out);
    out
}

fn doc_of(line: &str) -> String {
    let v: serde_json::Value = serde_json::from_str(line).expect("a golden line is JSON");
    v["doc"]
        .as_str()
        .expect("a line names its document")
        .to_owned()
}

/// What is wrong with `got` against the native `want`, if anything.
fn mismatch(what: &str, got: &str, want: &str) -> Option<String> {
    (got != want).then(|| format!("{what}\n  native: {want}\n  wasm32: {got}"))
}

#[wasm_bindgen_test]
fn every_document_evaluates_to_its_native_line() {
    let registry = registry();
    let mut differ = Vec::new();
    for want in EVAL_DOCS.lines() {
        let doc = doc_of(want);
        let run = arrix_doc::run(&doc, &source(&doc), &registry, None)
            .unwrap_or_else(|e| panic!("{doc}: {e}"));
        let got = serde_json::to_string(&run.line).unwrap();
        differ.extend(mismatch(&doc, &got, want));
    }
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}

#[wasm_bindgen_test]
fn the_bracket_width_sweep_evaluates_to_its_native_lines() {
    let sweep: Sweep = BRACKET_SWEEP_SPEC.parse().unwrap();
    let doc = "tests/docs/bracket";
    let runs = arrix_doc::sweep(doc, &source(doc), &registry(), &sweep, None).unwrap();
    let got: Vec<String> = runs
        .iter()
        .map(|r| serde_json::to_string(&r.line).unwrap())
        .collect();
    let want: Vec<&str> = BRACKET_SWEEP.lines().collect();
    assert_eq!(got.len(), want.len());
    let differ: Vec<_> = got
        .iter()
        .zip(&want)
        .enumerate()
        .filter_map(|(i, (got, want))| mismatch(&format!("point {i}"), got, want))
        .collect();
    assert!(differ.is_empty(), "{}", differ.join("\n"));
}
