//! First-party plugin `robotics`: the URDF exporter (docs/ROADMAP.md §C3).
//! Depends on `arrix-plugin-api` alone, like any third-party plugin.
//!
//! `robotics.urdf` (0.1) exports an assembly: a `base_link` that every
//! grounded component hangs from by a fixed joint, a link per part
//! component at every depth, a joint per assembly joint (`revolute` with
//! limits, `continuous` without, `prismatic`, `fixed`) with its axis on z
//! of the anchor, one binary STL per part in metres, and `<inertial>` from
//! the part's mass. A joint's `q` is the document's value plus offset; the
//! document's limits are written as given.

mod model;
mod names;
mod rigid;
mod stl;
#[cfg(test)]
mod tests;
mod urdf;

use arrix_plugin_api::{
    ChoiceOption, ChoiceSpec, ChoiceValue, Diagnostic, ExportFile, Exporter, ExporterSpec,
    ParamSpec, ParamValue, QuantityKind, Severity, Snapshot, Subject, SubjectKind,
};

/// The plugin's manifest, as a host reads it.
pub const MANIFEST: &str = include_str!("../arrix-plugin.toml");

/// The exporter's id.
pub const URDF: &str = "robotics.urdf";

/// The plugin's exporters.
pub struct Robotics;

fn spec() -> ExporterSpec {
    let param = |name: &str, title: &str, default: &str| ParamSpec {
        name: name.into(),
        title: title.into(),
        kind: QuantityKind::Ratio,
        default: default.into(),
        shown_when: None,
    };
    ExporterSpec {
        id: URDF.into(),
        title: "URDF".into(),
        extension: "urdf".into(),
        subject: SubjectKind::Assembly,
        params: vec![
            param("effort", "Joint effort limit (N or N m)", "100"),
            param("velocity", "Joint velocity limit (m/s or rad/s)", "1"),
        ],
        choices: vec![ChoiceSpec {
            name: "mesh-prefix".into(),
            title: "Mesh paths".into(),
            options: vec![
                ChoiceOption {
                    name: "relative".into(),
                    title: "Relative to the URDF".into(),
                },
                ChoiceOption {
                    name: "package".into(),
                    title: "package://<robot>/".into(),
                },
            ],
            default: Some("relative".into()),
        }],
    }
}

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

impl Exporter for Robotics {
    fn exporters(&self) -> Vec<ExporterSpec> {
        vec![spec()]
    }

    fn export(
        &self,
        snapshot: &mut dyn Snapshot,
        exporter: &str,
        subject: &Subject,
        params: &[ParamValue],
        choices: &[ChoiceValue],
    ) -> Result<Vec<ExportFile>, Diagnostic> {
        if exporter != URDF {
            return Err(error(
                "robotics.unknown-exporter",
                format!("robotics has no exporter {exporter}"),
            ));
        }
        let Subject::Assembly(assembly) = subject else {
            return Err(error(
                "robotics.subject",
                "robotics.urdf exports an assembly, not a part",
            ));
        };
        let param = |name: &str, default: f64| {
            params
                .iter()
                .find(|p| p.name == name)
                .map_or(default, |p| p.value.si)
        };
        let opts = urdf::Options {
            effort: param("effort", 100.0),
            velocity: param("velocity", 1.0),
            package: choices
                .iter()
                .any(|c| c.name == "mesh-prefix" && c.value == "package"),
        };
        let robot = model::read(snapshot, *assembly).ok_or_else(|| {
            error(
                "robotics.no-assembly",
                format!("the document has no assembly {assembly}"),
            )
        })?;
        Ok(urdf::files(robot, snapshot, &opts))
    }
}

// Tier 1: the world's `exchange` export, for the component build only.
#[cfg(all(feature = "guest", target_arch = "wasm32"))]
arrix_plugin_api::export_plugin!(exporters = Robotics);
