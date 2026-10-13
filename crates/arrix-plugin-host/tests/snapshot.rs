//! The snapshot read and exporters at Tier 0 (plans/c3-m2 step 4,
//! ADR-0037): a test exporter writes a text dump of the snapshot of
//! `tests/docs/assembly-exchange`, held to a golden that pins its
//! contents: poses, joints, masses, mesh counts. Regenerate with
//! `UPDATE_SNAPSHOTS=1`, then read the diff.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;

use arrix_doc::{Evaluator, MemorySource, Registry, open};
use arrix_plugin_api::{
    ChoiceOption, ChoiceSpec, ChoiceValue, Diagnostic, ExportFile, Exporter, ExporterSpec,
    JointState, ParamSpec, ParamValue, QuantityKind, Severity, Snapshot, Subject, SubjectKind,
};
use arrix_plugin_host::{HostError, register_tier0_exporters};

fn manifest(capabilities: &str) -> String {
    format!(
        "[plugin]\nid = \"dump\"\nversion = \"0.1.0\"\napi = \"^0.7\"\ntier = 0\ntitle = \"Dump\"\n\
         licence = \"MIT OR Apache-2.0\"\n[capabilities]\n{capabilities}\n"
    )
}

/// `dump.text`: the snapshot as text, one file; `dump.pair` writes two
/// files, and `dump.bad` one with a name the host refuses.
struct Dump;

fn spec(name: &str, subject: SubjectKind) -> ExporterSpec {
    ExporterSpec {
        id: format!("dump.{name}"),
        title: name.into(),
        extension: "txt".into(),
        subject,
        params: vec![ParamSpec {
            name: "scale".into(),
            title: "Scale".into(),
            kind: QuantityKind::Ratio,
            default: "1".into(),
            shown_when: None,
        }],
        choices: vec![ChoiceSpec {
            name: "unit".into(),
            title: "Unit".into(),
            options: vec![ChoiceOption {
                name: "metre".into(),
                title: "Metre".into(),
            }],
            default: Some("metre".into()),
        }],
    }
}

/// A number to nine places, in six significant digits below a millimetre
/// (an inertia), and `0` for the noise about zero.
fn n(x: f64) -> String {
    if x.abs() < 1e-15 {
        "0".into()
    } else if x.abs() < 1e-3 {
        format!("{x:.5e}")
    } else {
        format!("{x:.9}")
    }
}

fn frame(f: &arrix_plugin_api::Frame) -> String {
    let (o, x, z) = (f.origin(), f.x_axis(), f.z_axis());
    format!(
        "o=({}, {}, {}) x=({}, {}, {}) z=({}, {}, {})",
        n(o.x),
        n(o.y),
        n(o.z),
        n(x.x),
        n(x.y),
        n(x.z),
        n(z.x),
        n(z.y),
        n(z.z)
    )
}

fn text(snapshot: &mut dyn Snapshot, params: &[ParamValue], choices: &[ChoiceValue]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "options {params:?} {choices:?}");
    for part in snapshot.parts() {
        let _ = writeln!(out, "part {} {}", part.id, part.name);
        for b in &part.bodies {
            let material = b.material.as_ref().map_or("none".to_owned(), |m| {
                format!(
                    "{} ({}, {}, {})",
                    m.name,
                    n(m.color.red),
                    n(m.color.green),
                    n(m.color.blue)
                )
            });
            match snapshot.mesh(part.id, &b.body) {
                Ok(m) => {
                    assert_eq!(m.positions.len(), m.normals.len());
                    let _ = writeln!(
                        out,
                        "  body {}.{} material {material}: {} vertices, {} triangles",
                        b.body.feature,
                        b.body.slot,
                        m.positions.len() / 3,
                        m.indices.len() / 3
                    );
                }
                Err(d) => {
                    let _ = writeln!(out, "  body {}.{}: {}", b.body.feature, b.body.slot, d.code);
                }
            }
        }
        match snapshot.mass(part.id) {
            Ok(i) => {
                let t = i.inertia;
                let _ = writeln!(
                    out,
                    "  mass {} at ({}, {}, {}) inertia xx={} xy={} xz={} yy={} yz={} zz={}",
                    n(i.mass),
                    n(i.centroid.x),
                    n(i.centroid.y),
                    n(i.centroid.z),
                    n(t.xx),
                    n(t.xy),
                    n(t.xz),
                    n(t.yy),
                    n(t.yz),
                    n(t.zz)
                );
            }
            Err(d) => {
                let _ = writeln!(out, "  mass: {}", d.code);
            }
        }
    }
    for a in snapshot.assemblies() {
        let _ = writeln!(out, "assembly {} {}", a.id, a.name);
        for c in &a.components {
            let path: Vec<_> = c.path.iter().map(ToString::to_string).collect();
            let _ = writeln!(
                out,
                "  component {} {} of {}: {}{}",
                path.join("/"),
                c.name,
                match c.of {
                    arrix_plugin_api::ComponentOf::Part(p) => format!("part {p}"),
                    arrix_plugin_api::ComponentOf::Assembly(a) => format!("assembly {a}"),
                },
                frame(&c.pose),
                if c.diagnostics.is_empty() {
                    ""
                } else {
                    " (diagnostics)"
                }
            );
        }
        for j in &a.joints {
            let limits = j.limits.map_or("none".to_owned(), |[lo, hi]| {
                format!("[{}, {}]", n(lo), n(hi))
            });
            let _ = write!(
                out,
                "  joint {} {} {:?} limits {limits}: ",
                j.id, j.name, j.kind
            );
            match &j.state {
                JointState::Placed(p) => {
                    let _ = writeln!(
                        out,
                        "value {} parent {} child {}",
                        n(p.value),
                        frame(&p.parent_anchor),
                        frame(&p.child_anchor)
                    );
                }
                JointState::Failed(d) => {
                    let _ = writeln!(out, "failed {}", d.code);
                }
            }
        }
    }
    out
}

impl Exporter for Dump {
    fn exporters(&self) -> Vec<ExporterSpec> {
        vec![
            spec("bad", SubjectKind::Assembly),
            spec("pair", SubjectKind::Assembly),
            spec("text", SubjectKind::Assembly),
            spec("part", SubjectKind::Part),
        ]
    }

    fn export(
        &self,
        snapshot: &mut dyn Snapshot,
        exporter: &str,
        _subject: &Subject,
        params: &[ParamValue],
        choices: &[ChoiceValue],
    ) -> Result<Vec<ExportFile>, Diagnostic> {
        let file = |name: &str, bytes: Vec<u8>| ExportFile {
            name: name.into(),
            bytes,
        };
        match exporter {
            "dump.text" => Ok(vec![file(
                "dump.txt",
                text(snapshot, params, choices).into_bytes(),
            )]),
            "dump.pair" => Ok(vec![
                file("dump.txt", b"main".to_vec()),
                file("meshes/a.bin", vec![1, 2, 3]),
            ]),
            "dump.bad" => Ok(vec![file("../escape.txt", vec![])]),
            "dump.part" => panic!("the exporter panics"),
            other => Err(Diagnostic::new(Severity::Error, "dump.unknown", other).unwrap()),
        }
    }
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
    assert_eq!(r.exporter_specs().len(), 4);
    assert!(r.remove_plugin(&"dump".parse().unwrap()));
    assert!(r.exporter("dump.text").is_none());
}
