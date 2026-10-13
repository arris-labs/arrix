//! The snapshot read and exporters at Tier 0 (plans/c3-m2 step 4,
//! ADR-0037): a test exporter writes a text dump of the snapshot of
//! `tests/docs/assembly-exchange`, held to a golden that pins its
//! contents: poses, joints, masses, mesh counts. Regenerate with
//! `UPDATE_SNAPSHOTS=1`, then read the diff.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use arrix_doc::{Evaluator, MemorySource, Registry, open};
use arrix_plugin_api::{Diagnostic, ExportFile, Subject};
use arrix_plugin_host::{HostError, register_tier0_exporters};

#[path = "plugins/dump/src/dump.rs"]
mod dump;
use dump::Dump;

fn manifest(capabilities: &str) -> String {
    format!(
        "[plugin]\nid = \"dump\"\nversion = \"0.1.0\"\napi = \"^0.7\"\ntier = 0\ntitle = \"Dump\"\n\
         licence = \"MIT OR Apache-2.0\"\n[capabilities]\n{capabilities}\n"
    )
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
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

fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0_exporters(&mut r, &manifest("document-read = true"), Arc::new(Dump)).unwrap();
    r
}

/// `assembly-exchange` evaluated, and the exporter `id` run on its first
/// assembly (or `subject`).
struct Run {
    evaluator: Evaluator,
    document: arrix_doc::Document,
    evaluation: arrix_doc::Evaluation,
    registry: Registry,
}

fn run() -> Run {
    let registry = registry();
    let document = open(&files("assembly-exchange")).unwrap();
    let mut evaluator = Evaluator::new(registry.clone());
    let evaluation = evaluator.evaluate(&document);
    Run {
        evaluator,
        document,
        evaluation,
        registry,
    }
}

impl Run {
    fn export(
        &mut self,
        id: &str,
        subject: Subject,
        params: &[(&str, &str)],
    ) -> Result<Vec<ExportFile>, Diagnostic> {
        let params: BTreeMap<_, _> = params
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let exporter = self.registry.exporter(id).expect("registered").clone();
        self.evaluator.export_with(
            &self.document,
            &self.evaluation,
            &*exporter,
            subject,
            &params,
            &BTreeMap::new(),
        )
    }

    fn assembly(&self) -> Subject {
        Subject::Assembly(*self.document.assemblies().keys().next().unwrap())
    }
}

#[test]
fn the_snapshot_of_the_exchange_assembly_is_what_the_golden_pins() {
    let mut run = run();
    let subject = run.assembly();
    let files = run.export("dump.text", subject, &[]).unwrap();
    assert_eq!(files.len(), 1);
    let got = String::from_utf8(files[0].bytes.clone()).unwrap();

    let golden =
        root().join("crates/arrix-plugin-host/tests/golden/snapshot-assembly-exchange.txt");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&golden, &got).unwrap();
    }
    let want = std::fs::read_to_string(&golden).unwrap_or_default();
    assert_eq!(
        got, want,
        "regenerate with UPDATE_SNAPSHOTS=1 and read the diff"
    );

    // Pure: the same snapshot, the same bytes.
    let again = run.export("dump.text", subject, &[]).unwrap();
    assert_eq!(again, files);
}

#[test]
fn an_option_is_an_expression_and_a_refused_one_is_a_diagnostic() {
    let mut run = run();
    let subject = run.assembly();
    let files = run
        .export("dump.text", subject, &[("scale", "2 * 3")])
        .unwrap();
    let text = String::from_utf8(files[0].bytes.clone()).unwrap();
    assert!(
        text.contains("value: 6.0") || text.contains("si: 6.0"),
        "{text}"
    );

    let code = |r: Result<Vec<ExportFile>, Diagnostic>| r.unwrap_err().code.to_string();
    assert_eq!(
        code(run.export("dump.text", subject, &[("nope", "1")])),
        "feature.unknown-field"
    );
    assert_eq!(
        code(run.export("dump.text", subject, &[("scale", "1 mm")])),
        "expr.unit-mismatch"
    );
}

#[test]
fn a_subject_of_the_wrong_kind_or_not_there_is_refused() {
    let mut run = run();
    let part = Subject::Part(*run.document.parts().keys().next().unwrap());
    let code = |r: Result<Vec<ExportFile>, Diagnostic>| r.unwrap_err().code.to_string();
    assert_eq!(code(run.export("dump.text", part, &[])), "export.subject");
    let missing = Subject::Assembly(arrix_plugin_api::AssemblyId(arrix_plugin_api::Id(0x7ff)));
    assert_eq!(
        code(run.export("dump.text", missing, &[])),
        "export.unknown-subject"
    );
}

#[test]
fn files_are_named_relative_and_the_host_refuses_the_rest() {
    let mut run = run();
    let subject = run.assembly();
    let pair = run.export("dump.pair", subject, &[]).unwrap();
    assert_eq!(
        pair.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["dump.txt", "meshes/a.bin"]
    );
    assert_eq!(
        run.export("dump.bad", subject, &[])
            .unwrap_err()
            .code
            .to_string(),
        "export.path"
    );
    for name in ["", "/a", "a//b", "./a", "a/../b", "a\\b", "C:/a", ".."] {
        assert!(arrix_doc::check_export_name(name).is_err(), "{name:?}");
    }
    for name in ["a", "meshes/link.stl", "a.b/c"] {
        assert!(arrix_doc::check_export_name(name).is_ok(), "{name:?}");
    }
}

#[test]
fn a_panic_is_the_exports_diagnostic() {
    let mut run = run();
    let part = Subject::Part(*run.document.parts().keys().next().unwrap());
    let d = run.export("dump.part", part, &[]).unwrap_err();
    assert_eq!(d.code.to_string(), "plugin.panic");
}

#[test]
fn a_plugin_that_reads_the_document_declares_it() {
    let mut r = Registry::with_core_types();
    let err =
        register_tier0_exporters(&mut r, &manifest("kernel = true"), Arc::new(Dump)).unwrap_err();
    assert!(matches!(err, HostError::Capability { .. }), "{err}");
    assert!(r.exporter_specs().is_empty());
}

#[test]
fn leaving_a_plugin_out_leaves_its_exporters_out() {
    let mut r = registry();
    assert_eq!(r.exporter_specs().len(), 5);
    assert!(r.remove_plugin(&"dump".parse().unwrap()));
    assert!(r.exporter("dump.text").is_none());
}

/// The session answers an exporter request with the files `export_with`
/// writes, in the same bytes (plans/c3-m2 step 5).
#[test]
fn a_session_export_by_an_exporter_writes_the_files_of_the_headless_path() {
    use arrix_doc::{ExportSubject, ExporterRequest, LocalSession, Outcome, Session, SessionEvent};
    use std::time::Duration;

    let mut headless = run();
    let Subject::Assembly(assembly) = headless.assembly() else {
        unreachable!()
    };
    let want = headless
        .export("dump.pair", Subject::Assembly(assembly), &[])
        .unwrap();

    let document = open(&files("assembly-exchange")).unwrap();
    let session = LocalSession::open(document, registry()).unwrap();
    let events = session.subscribe();
    let ask = |id: &str| ExporterRequest {
        id: id.into(),
        subject: ExportSubject::Assembly(assembly),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
    };
    let answer = |events: &arrix_doc::EventStream| loop {
        let event = events
            .recv_timeout(Duration::from_secs(60))
            .expect("the session answers");
        if let SessionEvent::Exported { bytes, files, .. } = event {
            return (bytes, files);
        }
    };

    assert_eq!(
        session.export_with(ask("dump.pair")),
        Ok(Outcome::Exporting)
    );
    let (bytes, files) = answer(&events);
    assert_eq!(bytes, Ok(Vec::new()));
    let got: Vec<_> = files.into_iter().map(|f| (f.name, f.bytes)).collect();
    let want: Vec<_> = want.into_iter().map(|f| (f.name, f.bytes)).collect();
    assert_eq!(got, want);

    // A refusal is the answer, not a hang: here a name outside the directory.
    assert_eq!(session.export_with(ask("dump.bad")), Ok(Outcome::Exporting));
    let (bytes, files) = answer(&events);
    assert_eq!(bytes.unwrap_err().code.as_str(), "export.path");
    assert!(files.is_empty());
}
