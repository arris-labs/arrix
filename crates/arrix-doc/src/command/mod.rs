//! Commands: the only way a document changes (docs/DATA-MODEL.md §Commands
//! and undo). Plain, serialisable data; applying one is a pure function of
//! the document and the command, returning the new document, the inverse
//! and the nodes touched, whole or not at all.

mod apply;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{Diagnostic, FeatureId, Id, ParamId, PartId, Ref, Severity};
use arrix_sketch::{SketchEdit, SketchError};
use serde::{Deserialize, Serialize};

use crate::dag::Node;
use crate::document::{Document, FeatureRecord, Invalid, Param, Part};
use crate::expr::Expr;

/// One edit of the document. One gesture is one command; a multi-step
/// gesture is a `Group`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    AddParam {
        param: ParamId,
        record: Param,
    },
    SetParam {
        param: ParamId,
        expr: Expr,
    },
    DeleteParam {
        param: ParamId,
    },
    /// A part with its features: the inverse of `DeletePart`.
    AddPart {
        part: Part,
    },
    DeletePart {
        part: PartId,
    },
    /// Inserts `record` at history index `at`. Inserting before the
    /// rollback index moves the index with the features after it.
    AddFeature {
        part: PartId,
        at: usize,
        record: FeatureRecord,
    },
    EditFeature {
        feature: FeatureId,
        edit: FeatureEdit,
    },
    DeleteFeature {
        feature: FeatureId,
    },
    /// Moves a feature to history index `to`, counted after the move.
    ReorderFeature {
        feature: FeatureId,
        to: usize,
    },
    SetRollback {
        part: PartId,
        at: Option<usize>,
    },
    /// Puts or removes records of a `core.sketch` feature's sketch. It
    /// carries the positions its author solved for; applying it never
    /// solves (ADR-0005).
    SketchEdit {
        feature: FeatureId,
        edit: SketchEdit,
    },
    /// One undo entry.
    Group {
        label: String,
        commands: Vec<Command>,
    },
}

/// What an `EditFeature` changes; a field it leaves out is kept. In
/// `params`, `choices` and `inputs`, `null` removes the entry.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FeatureEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppressed: Option<bool>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub params: BTreeMap<String, Option<Expr>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub choices: BTreeMap<String, Option<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, Option<Ref>>,
}

/// Why a command was refused. The document is unchanged.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum CommandError {
    #[error("no parameter {0}")]
    NoParam(ParamId),
    #[error("no part {0}")]
    NoPart(PartId),
    #[error("no feature {0}")]
    NoFeature(FeatureId),
    #[error("the id {0} is already in use")]
    IdTaken(Id),
    #[error("index {at} is past the {len} places there are")]
    Index { at: usize, len: usize },
    #[error("feature {0} holds no sketch")]
    NoSketch(FeatureId),
    #[error("feature {feature}'s sketch: {error}")]
    Sketch {
        feature: FeatureId,
        error: SketchError,
    },
    #[error("the result would be malformed: {0}")]
    Invalid(#[from] Invalid),
}

impl CommandError {
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            CommandError::Invalid(i) => i.diagnostic(),
            other => Diagnostic::new(Severity::Error, "command.refused", other.to_string())
                .expect("a well-formed code"),
        }
    }
}

/// A command applied: the new document, the command that undoes it and
/// the nodes it wrote.
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    pub document: Document,
    pub inverse: Command,
    pub touched: BTreeSet<Node>,
}

/// Applies `command` to `doc`, whole or not at all.
pub fn apply(doc: &Document, command: &Command) -> Result<Applied, CommandError> {
    let mut document = doc.clone();
    let mut touched = BTreeSet::new();
    let inverse = apply::apply_mut(&mut document, command, &mut touched)?;
    document.validate()?;
    Ok(Applied {
        document,
        inverse,
        touched,
    })
}
