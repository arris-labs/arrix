//! The test exporter `dump` (plans/c3-m2 steps 4 and 6): `dump.text` writes
//! the snapshot as text, `dump.pair` two files, `dump.bad` one with a name
//! the host refuses, `dump.empty` a file with no bytes, and `dump.part`
//! panics. Included by `#[path]` into the Tier 0 test and built as the
//! Tier 1 component, so both tiers run the same code.

use std::fmt::Write as _;

use arrix_plugin_api::{
    ChoiceOption, ChoiceSpec, ChoiceValue, Diagnostic, ExportFile, Exporter, ExporterSpec,
    JointState, ParamSpec, ParamValue, QuantityKind, Severity, Snapshot, Subject, SubjectKind,
};

/// `dump.text`: the snapshot as text, one file; `dump.pair` writes two
/// files, and `dump.bad` one with a name the host refuses.
pub struct Dump;

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
            spec("empty", SubjectKind::Assembly),
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
            "dump.empty" => Ok(vec![file("empty.bin", vec![])]),
            "dump.part" => panic!("the exporter panics"),
            other => Err(Diagnostic::new(Severity::Error, "dump.unknown", other).unwrap()),
        }
    }
}
