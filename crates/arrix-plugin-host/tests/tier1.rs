//! Tier 1 hosting (plans/c3-m1 step 6, ADR-0035): `gears`, built from its
//! own source as a WebAssembly component, loads from a tier-1 plugin
//! directory and evaluates `gear-on-plane` through wasmtime, its kernel
//! calls served on the evaluating feature's kernel, a datum plane resolved
//! on the face it named. The parity with Tier 0, to the bit, is step 7's.

#[path = "support/tier1.rs"]
mod tier1_build;

use std::path::Path;
use std::sync::Arc;

use arrix_doc::{
    Document, Evaluator, FeatureOutcome, FeatureTypeId, MemorySource, Registry, SlotView, open,
};
use arrix_plugin_api::{FeatureId, Id};
use arrix_plugin_host::{HostError, register_tier0, register_tier1};

fn tier1() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier1(&mut r, tier1_build::gears()).unwrap();
    r
}

fn tier0() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears)).unwrap();
    r
}

/// A document directory's files, read into memory.
fn document(dir: &Path) -> Document {
    let mut files = MemorySource::default();
    let mut pending = vec![dir.to_owned()];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let rel = path.strip_prefix(dir).unwrap().to_str().unwrap();
                files
                    .0
                    .insert(rel.replace('\\', "/"), std::fs::read(&path).unwrap());
            }
        }
    }
    open(&files).unwrap()
}

fn id(text: &str) -> FeatureId {
    FeatureId(text.parse::<Id>().unwrap())
}

#[test]
fn gears_registers_at_tier1_what_it_describes() {
    let ids = register_tier1(&mut Registry::with_core_types(), tier1_build::gears()).unwrap();
    assert_eq!(ids, [FeatureTypeId::new("gears.spur").unwrap()]);
    let r = tier1();
    let spec = &r.get(&ids[0]).unwrap().spec().clone();
    assert_eq!(
        spec,
        &arrix_plugin_api::Feature::describe(&arrix_gears::Gears)[0]
    );
}

#[test]
fn gear_on_plane_evaluates_through_the_component() {
    let doc = document(&tier1_build::root().join("tests/docs/gear-on-plane"));
    let mut live = Evaluator::new(tier1());
    let mut native = Evaluator::new(tier0());
    let (a, b) = (live.evaluate(&doc), native.evaluate(&doc));
    for e in &a.events {
        assert!(
            matches!(e.outcome, FeatureOutcome::Ok { .. }),
            "{:?}: {:?}",
            e.feature,
            e.outcome
        );
    }
    let gear = id("F7Y3N3HJPEEDE");
    let Some(SlotView::Body(m)) = a.slot(gear, "body") else {
        panic!("{:?}", a.outcome(gear))
    };
    assert!(m.volume > 0.0, "{m:?}");
    let Some(SlotView::Body(n)) = b.slot(gear, "body") else {
        panic!("{:?}", b.outcome(gear))
    };
    assert_eq!(m.faces, n.faces, "the same gear");
    let top = id("CVXP4WNHXHRHD");
    assert!(
        matches!(a.slot(top, "plane"), Some(SlotView::Plane(_))),
        "the top plane resolves on the gear's end cap: {:?}",
        a.outcome(top)
    );
    let ops = |e: &Evaluator| {
        e.kernel_calls()
            .iter()
            .map(|c| serde_json::to_value(&c.op).unwrap()["op"].clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ops(&live), ops(&native), "the same kernel calls");
}

#[test]
fn a_plugin_directory_without_its_component_is_refused() {
    let dir = tier1_build::plugin_dir("gears-missing", arrix_gears::MANIFEST, b"");
    std::fs::remove_file(dir.join("gears-missing.wasm")).unwrap();
    let e = register_tier1(&mut Registry::with_core_types(), &dir).unwrap_err();
    let HostError::Process { message, .. } = &e else {
        panic!("{e:?}")
    };
    assert!(message.contains("gears-missing.wasm"), "{message}");
}

#[test]
fn a_tier0_manifest_is_refused_at_tier1() {
    let dir = tier1_build::gears();
    let other = dir.with_file_name("gears-as-tier0");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("arrix-plugin.toml"), arrix_gears::MANIFEST).unwrap();
    let e = register_tier1(&mut Registry::with_core_types(), &other).unwrap_err();
    assert!(
        matches!(
            e,
            HostError::Tier {
                tier: 0,
                expected: 1,
                ..
            }
        ),
        "{e:?}"
    );
}
