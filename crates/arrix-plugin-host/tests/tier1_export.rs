//! Exporters at Tier 1 (plans/c3-m2 step 6, ADR-0037): the `dump` test
//! exporter built as a component writes, on `tests/docs/assembly-exchange`,
//! the files it writes compiled in, to the byte; its `document` import is
//! refused without `document-read`, in an evaluation and outside a call;
//! a refused name is the export's diagnostic.

#[path = "support/tier1.rs"]
mod tier1_build;

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_doc::{Evaluator, MemorySource, Registry, open};
use arrix_plugin_api::{Diagnostic, ExportFile, Subject};
use arrix_plugin_host::{HostError, register_tier0_exporters, register_tier1};

#[path = "plugins/dump/src/dump.rs"]
mod dump;

fn manifest() -> String {
    std::fs::read_to_string(
        tier1_build::root().join("crates/arrix-plugin-host/tests/plugins/dump/arrix-plugin.toml"),
    )
    .unwrap()
}

fn tier0() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0_exporters(&mut r, &manifest(), Arc::new(dump::Dump)).unwrap();
    r
}

fn tier1() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier1(&mut r, &tier1_build::dump("dump-granted", true)).unwrap();
    r
}

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

/// The exporter `id` run on the exchange assembly of `registry`.
fn export(
    registry: &Registry,
    id: &str,
    params: &[(&str, &str)],
) -> Result<Vec<ExportFile>, Diagnostic> {
    let document = open(&files("assembly-exchange")).unwrap();
    let mut evaluator = Evaluator::new(registry.clone());
    let evaluation = evaluator.evaluate(&document);
    let subject = Subject::Assembly(*document.assemblies().keys().next().unwrap());
    let params: BTreeMap<_, _> = params
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let exporter = registry.exporter(id).expect("registered").clone();
    evaluator.export_with(
        &document,
        &evaluation,
        &*exporter,
        subject,
        &params,
        &BTreeMap::new(),
    )
}

#[test]
fn a_component_registers_the_same_exporters_as_compiled_in() {
    assert_eq!(tier1().exporter_specs(), tier0().exporter_specs());
}

#[test]
fn the_files_are_the_same_at_both_tiers_to_the_byte() {
    let (t0, t1) = (tier0(), tier1());
    for (id, params) in [
        ("dump.text", &[][..]),
        ("dump.text", &[("scale", "2")]),
        ("dump.pair", &[]),
        ("dump.empty", &[]),
    ] {
        let (a, b) = (export(&t0, id, params), export(&t1, id, params));
        assert_eq!(a, b, "{id} {params:?}");
        assert!(a.is_ok(), "{id}: {a:?}");
    }
    // The text is the snapshot: more than a header.
    let text = export(&t1, "dump.text", &[]).unwrap();
    assert!(text[0].bytes.len() > 200);
}

#[test]
fn an_export_runs_in_a_fresh_store_each_time() {
    let t1 = tier1();
    assert_eq!(export(&t1, "dump.text", &[]), export(&t1, "dump.text", &[]));
}

#[test]
fn a_refused_name_is_the_exports_diagnostic_at_both_tiers() {
    let t1 = tier1();
    let bad = export(&t1, "dump.bad", &[]).unwrap_err();
    assert_eq!(bad.code.as_str(), "export.path", "{bad:?}");
    assert_eq!(
        export(&tier0(), "dump.bad", &[]).unwrap_err().code,
        bad.code
    );
}

#[test]
fn a_component_that_reads_the_document_without_the_capability_is_refused_at_load() {
    let mut r = Registry::with_core_types();
    let err = register_tier1(&mut r, &tier1_build::dump("dump-denied", false)).unwrap_err();
    assert!(
        matches!(&err, HostError::Process { message, .. } if message.contains("document-read")),
        "{err}"
    );
    assert!(r.exporter_specs().is_empty());
}
