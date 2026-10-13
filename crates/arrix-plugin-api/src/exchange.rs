//! Exporters a plugin contributes (the world's `exchange`; ADR-0037;
//! docs/PLUGINS.md §Contribution points, 4).

use arrix_core::{AssemblyId, Diagnostic, PartId};
use serde::{Deserialize, Serialize};

use crate::{ChoiceSpec, ChoiceValue, ParamSpec, ParamValue, Snapshot};

/// What an exporter takes: one part or one assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SubjectKind {
    Part,
    Assembly,
}

/// The part or assembly an export is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Subject {
    Part(PartId),
    Assembly(AssemblyId),
}

/// An exporter: its form (a feature's fields) and what it writes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExporterSpec {
    /// `<plugin-id>.<name>`.
    pub id: String,
    pub title: String,
    /// The main file's extension, without the dot.
    pub extension: String,
    pub subject: SubjectKind,
    pub params: Vec<ParamSpec>,
    pub choices: Vec<ChoiceSpec>,
}

/// A file an export wrote. `name` is a relative path of `/`-separated
/// parts, none empty, `.` or `..` (ADR-0037 §7); the host writes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// A plugin's exporters. `export` is deterministic: a pure function of the
/// snapshot and the options, with no clock, randomness, files or network.
/// The first file of a result is the main one.
pub trait Exporter: Send + Sync {
    fn exporters(&self) -> Vec<ExporterSpec>;

    fn export(
        &self,
        snapshot: &mut dyn Snapshot,
        exporter: &str,
        subject: &Subject,
        params: &[ParamValue],
        choices: &[ChoiceValue],
    ) -> Result<Vec<ExportFile>, Diagnostic>;
}
