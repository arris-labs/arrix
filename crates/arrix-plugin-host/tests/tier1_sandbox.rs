//! The Tier 1 sandbox's failures (plans/c3-m1 step 8, ADR-0035 §5): a test
//! plugin that loops, panics, grows its memory without bound and calls a
//! kernel its manifest does not grant, each ending as that feature's
//! diagnostic while the feature beside it evaluates; and a component that
//! imports an interface outside the world, refused at load.

#[path = "support/tier1.rs"]
mod tier1_build;

use arrix_doc::{Evaluator, FeatureOutcome, MemorySource, Registry, open};
use arrix_plugin_api::{FeatureId, Id};
use arrix_plugin_host::{HostError, register_tier1};

/// The fuel the misbehaving plugin's manifest sets: a square's extrude
/// takes a small fraction of it, a loop all of it.
const FUEL: u64 = 20_000_000;

const FEATURES: [(&str, &str); 5] = [
    ("1000000000001", "misbehave.fine"),
    ("1000000000002", "misbehave.loop"),
    ("1000000000003", "misbehave.panic"),
    ("1000000000004", "misbehave.memory"),
    ("1000000000005", "misbehave.kernel"),
];

/// A one-part document with a feature of each type, none with inputs.
fn document() -> arrix_doc::Document {
    let features: serde_json::Map<_, _> = FEATURES
        .iter()
        .map(|(id, ty)| {
            (
                (*id).to_owned(),
                serde_json::json!({
                    "id": id, "inputs": {}, "name": ty, "params": {},
                    "suppressed": false, "type_id": ty, "type_version": 1,
                }),
            )
        })
        .collect();
    let history: Vec<_> = FEATURES.iter().map(|(id, _)| *id).collect();
    let part = serde_json::json!({
        "features": features, "history": history, "id": "2000000000001",
        "name": "Misbehave", "rollback": null,
    });
    let mut files = MemorySource::default();
    for (path, json) in [
        ("document.json", serde_json::json!({ "schema": 5 })),
        ("params.json", serde_json::json!({})),
        ("parts/2000000000001.json", part),
    ] {
        files
            .0
            .insert(path.to_owned(), serde_json::to_vec_pretty(&json).unwrap());
    }
    open(&files).unwrap()
}

fn feature(n: usize) -> FeatureId {
    FeatureId(FEATURES[n].0.parse::<Id>().unwrap())
}

/// The code and message each feature ends with, `None` for one that is Ok.
fn run(dir: &std::path::Path) -> Vec<Option<(String, String)>> {
    let mut registry = Registry::with_core_types();
    register_tier1(&mut registry, dir).unwrap();
    let result = Evaluator::new(registry).evaluate(&document());
    (0..FEATURES.len())
        .map(|n| match result.outcome(feature(n)).unwrap() {
            FeatureOutcome::Ok { .. } => None,
            FeatureOutcome::Failed { diagnostic, .. } => {
                Some((diagnostic.code.to_string(), diagnostic.message.clone()))
            }
            other => panic!("{}: {other:?}", FEATURES[n].1),
        })
        .collect()
}

fn code(outcome: &Option<(String, String)>) -> Option<&str> {
    outcome.as_ref().map(|(c, _)| c.as_str())
}

#[test]
fn each_failure_ends_as_its_diagnostic_and_the_rest_evaluates() {
    let dir = tier1_build::misbehave("misbehave", FUEL, true);
    let first = run(&dir);
    let codes: Vec<_> = first.iter().map(code).collect();
    assert_eq!(
        codes,
        [
            None,
            Some("plugin.fuel"),
            Some("plugin.trap"),
            Some("plugin.memory"),
            None,
        ],
        "{first:#?}"
    );
    let trap = &first[2].as_ref().unwrap().1;
    assert!(trap.contains("trapped"), "{trap}");
    assert!(trap.contains("misbehave.panic"), "{trap}");
}

#[test]
fn a_loop_ends_on_its_fuel_the_same_way_every_time() {
    let dir = tier1_build::misbehave("misbehave", FUEL, true);
    let (a, b) = (run(&dir), run(&dir));
    assert_eq!(a[1], b[1]);
    assert_eq!(code(&a[1]), Some("plugin.fuel"));
}

#[test]
fn the_kernel_is_refused_when_the_manifest_does_not_grant_it() {
    let dir = tier1_build::misbehave("misbehave-no-kernel", FUEL, false);
    let outcomes = run(&dir);
    // Both types that extrude are refused; the loop still ends on fuel.
    assert_eq!(
        code(&outcomes[0]),
        Some("plugin.capability"),
        "{outcomes:#?}"
    );
    assert_eq!(
        code(&outcomes[4]),
        Some("plugin.capability"),
        "{outcomes:#?}"
    );
    assert_eq!(code(&outcomes[1]), Some("plugin.fuel"));
    let (_, message) = outcomes[4].as_ref().unwrap();
    assert!(message.contains("kernel.extrude"), "{message}");
}

/// A component whose only import is `wasi:clocks/monotonic-clock`, an
/// interface the world does not have.
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
fn a_component_importing_beyond_the_world_is_refused_at_load() {
    let manifest = std::fs::read_to_string(
        tier1_build::root()
            .join("crates/arrix-plugin-host/tests/plugins/misbehave/arrix-plugin.toml"),
    )
    .unwrap();
    let dir = tier1_build::plugin_dir("misbehave-wasi", &manifest, &wasi_component());
    let e = register_tier1(&mut Registry::with_core_types(), &dir).unwrap_err();
    let HostError::Process { message, .. } = &e else {
        panic!("{e:?}")
    };
    assert!(
        message.contains("wasi:clocks/monotonic-clock@0.2.0")
            && message.contains("outside the world"),
        "{message}"
    );
}
