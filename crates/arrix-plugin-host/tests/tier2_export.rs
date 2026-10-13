//! Exporters at Tier 2 (plans/c3-m2 step 7, ADR-0037): `python/examples/
//! bom` registers its exporter, writes the bill of materials of
//! `tests/docs/assembly-exchange` the same on every run, and a plugin whose
//! manifest lacks `document-read` is refused where it describes its
//! exporters.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use arrix_doc::{Evaluator, MemorySource, Registry, open};
use arrix_plugin_api::Subject;
use arrix_plugin_host::{HostError, register_tier2};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn bom() -> PathBuf {
    root().join("python/examples/bom")
}

/// `bom` copied to a directory of its own, its command setting
/// `PYTHONPATH` to the SDK, and `edit` applied to its manifest.
fn staged(name: &str, edit: impl Fn(String) -> String) -> PathBuf {
    let dir = std::env::temp_dir().join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::copy(bom().join("bom.py"), dir.join("bom.py")).unwrap();
    let sdk = root().join("python");
    let manifest = std::fs::read_to_string(bom().join("arrix-plugin.toml"))
        .unwrap()
        .replace(
            "command = [\"python3\"",
            &format!(
                "command = [\"env\", \"PYTHONPATH={}\", \"python3\"",
                sdk.display()
            ),
        );
    std::fs::write(dir.join("arrix-plugin.toml"), edit(manifest)).unwrap();
    dir
}

fn files(name: &str) -> MemorySource {
    let dir = root().join("tests/docs").join(name);
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

#[test]
fn a_plugin_that_reads_the_document_without_declaring_it_is_refused() {
    let dir = staged("arrix-tier2-export-denied", |m| {
        m.replace("document-read = true", "document-read = false")
    });
    let mut registry = Registry::with_core_types();
    let err = register_tier2(&mut registry, &dir).unwrap_err();
    assert!(matches!(err, HostError::Capability { .. }), "{err}");
    assert!(registry.exporter_specs().is_empty());
}

#[test]
fn the_bill_of_materials_is_written_the_same_on_every_run() {
    let mut registry = Registry::with_core_types();
    register_tier2(&mut registry, &staged("arrix-tier2-export-bom", |m| m)).unwrap();
    assert_eq!(registry.exporter_specs()[0].id, "bom.csv");
    let document = open(&files("assembly-exchange")).unwrap();
    let run = || {
        let mut evaluator = Evaluator::new(registry.clone());
        let evaluation = evaluator.evaluate(&document);
        let subject = Subject::Assembly(*document.assemblies().keys().next().unwrap());
        let exporter = registry.exporter("bom.csv").unwrap().clone();
        evaluator
            .export_with(
                &document,
                &evaluation,
                &*exporter,
                subject,
                &BTreeMap::new(),
                &BTreeMap::new(),
            )
            .unwrap()
    };
    let first = run();
    assert_eq!(first, run());
    let text = String::from_utf8(first[0].bytes.clone()).unwrap();
    assert!(
        text.starts_with("part,count,mass (kg),material\n"),
        "{text}"
    );
    assert!(text.contains("Screw,2,"), "{text}");
}
