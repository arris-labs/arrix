//! Tier 1 parity (plans/c3-m1 step 7, ADR-0035): `gears` built as a
//! component evaluates `gear-on-plane` and the bracket to the eval line,
//! kernel calls and saved bytes it gives compiled in, to the bit; a
//! document saved at either tier opens live, not stale, at the other, and
//! frozen with neither. A plugin's result does not depend on its tier
//! (docs/CONCURRENCY-WASM.md §Determinism).

#[path = "support/tier1.rs"]
mod tier1_build;

use std::sync::Arc;

use arrix_doc::{Document, EvalLine, Evaluator, MemorySource, Registry, open, save};
use arrix_plugin_host::{register_tier0, register_tier1};

const DOCS: [&str; 2] = ["gear-on-plane", "bracket"];

fn tier0() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears)).unwrap();
    r
}

fn tier1() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier1(&mut r, tier1_build::gears()).unwrap();
    r
}

/// A document directory's files, read into memory.
fn files(name: &str) -> MemorySource {
    let dir = tier1_build::root().join("tests/docs").join(name);
    let mut files = MemorySource::default();
    let mut pending = vec![dir.clone()];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let rel = path.strip_prefix(&dir).unwrap().to_str().unwrap();
                files
                    .0
                    .insert(rel.replace('\\', "/"), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

/// What one evaluation of `doc` gives: its line, its kernel calls as
/// JSON, and the document saved with its frozen results, as a session
/// saves it.
struct Run {
    line: EvalLine,
    calls: Vec<serde_json::Value>,
    saved: MemorySource,
}

fn run(registry: Registry, name: &str, doc: &Document) -> Run {
    let mut evaluator = Evaluator::new(registry);
    let line = evaluator.evaluate(doc).line(name, doc);
    let calls = evaluator
        .kernel_calls()
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .collect();
    let mut frozen = doc.clone();
    for (id, result) in evaluator.frozen() {
        assert!(frozen.freeze(*id, result));
    }
    Run {
        line,
        calls,
        saved: save(&frozen),
    }
}

/// Each feature's status in `line`, by type.
fn statuses(line: &EvalLine) -> Vec<(String, String)> {
    line.features
        .iter()
        .map(|f| (f.feature_type.clone(), f.status.clone()))
        .collect()
}

#[test]
fn gears_evaluates_to_the_bit_at_either_tier() {
    for name in DOCS {
        let doc = open(&files(name)).unwrap();
        let (native, component) = (run(tier0(), name, &doc), run(tier1(), name, &doc));
        assert!(
            statuses(&native.line).iter().all(|(_, s)| s == "ok"),
            "{name}: {:?}",
            statuses(&native.line)
        );
        assert_eq!(native.line, component.line, "{name}: the eval line");
        assert_eq!(native.calls, component.calls, "{name}: the kernel calls");
        assert_eq!(native.saved.0, component.saved.0, "{name}: the saved bytes");
        let blobs = native.saved.0.keys().filter(|p| p.starts_with("blobs/"));
        assert!(blobs.count() > 0, "{name}: the gear's result is saved");
    }
}

#[test]
fn a_document_saved_at_one_tier_opens_live_at_the_other() {
    for name in DOCS {
        let doc = open(&files(name)).unwrap();
        let live = run(tier0(), name, &doc).line;
        let saved = [
            ("tier 0", run(tier0(), name, &doc).saved),
            ("tier 1", run(tier1(), name, &doc).saved),
        ];
        for (at, files) in &saved {
            let reopened = open(files).unwrap();
            for (other, registry) in [("tier 0", tier0()), ("tier 1", tier1())] {
                let line = run(registry, name, &reopened).line;
                assert_eq!(line, live, "{name} saved at {at}, opened at {other}");
            }
            // With neither, the gear is frozen, on the input it was saved
            // with, so not stale.
            let line = run(Registry::with_core_types(), name, &reopened).line;
            let gear = statuses(&line)
                .into_iter()
                .find(|(t, _)| t == "gears.spur")
                .unwrap();
            assert_eq!(
                gear.1, "frozen",
                "{name} saved at {at}, opened with neither"
            );
        }
    }
}
