//! The document: a recipe of parameters and parts, each part a feature
//! history (docs/DATA-MODEL.md §The document, §Features). Its fields are
//! this crate's: outside it a document is read, and changed only by
//! commands.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use arrix_core::{
    Diagnostic, FeatureId, ParamId, PartId, PluginId, QuantityKind, Ref, Severity, SketchEntityId,
};
use arrix_sketch::Sketch;
use serde::{Deserialize, Serialize};

use crate::dag::{Dag, DagError};
use crate::expr::Expr;

/// A document: everything needed to regenerate its geometry, and nothing
/// that can be regenerated. Generations are the authority's, not the
/// document's, so an undone document is equal to the one before.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub(crate) params: BTreeMap<ParamId, Param>,
    pub(crate) parts: BTreeMap<PartId, Part>,
}

/// A named, typed parameter.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Param {
    pub name: String,
    pub kind: QuantityKind,
    pub expr: Expr,
}

/// A part: a feature history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub id: PartId,
    pub name: String,
    /// The order the user sees and edits.
    pub history: Vec<FeatureId>,
    pub features: BTreeMap<FeatureId, FeatureRecord>,
    /// Features at and after this index are not evaluated.
    pub rollback: Option<usize>,
}

/// One history entry. Built-in and plugin features have this one shape.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureRecord {
    pub id: FeatureId,
    pub type_id: FeatureTypeId,
    /// The feature type's own schema version.
    pub type_version: u32,
    /// What the tree shows; unique within the part.
    pub name: String,
    /// The form's numeric fields, each an expression.
    pub params: BTreeMap<String, Expr>,
    /// The form's plain values: a word from a list the feature type
    /// names, such as a datum plane's world plane (`xy`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub choices: BTreeMap<String, String>,
    /// Named inputs: a plane, a profile, faces, a body.
    pub inputs: BTreeMap<String, Ref>,
    pub suppressed: bool,
    /// A `core.sketch` feature's sketch: its entities, constraints with
    /// their dimensions' expressions, and last solved positions
    /// (docs/DATA-MODEL.md §Sketches). Changed only by `SketchEdit`.
    /// Boxed: most features hold none, and a command carries records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<Box<Sketch>>,
}

/// `core.<name>` for a built-in, `<plugin>.<name>` for a plugin's; the
/// name is lower-kebab words.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FeatureTypeId(String);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "{0:?} is not a feature type id: `core` or a plugin id, a dot, then lower-case words \
     joined by hyphens"
)]
pub struct InvalidFeatureTypeId(pub String);

impl FeatureTypeId {
    pub fn new(id: impl Into<String>) -> Result<Self, InvalidFeatureTypeId> {
        let id = id.into();
        let ok = id.split_once('.').is_some_and(|(ns, name)| {
            (ns == "core" || PluginId::new(ns).is_ok()) && arrix_core::SlotName::new(name).is_ok()
        });
        if ok {
            Ok(Self(id))
        } else {
            Err(InvalidFeatureTypeId(id))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The plugin that contributes it, `None` for a built-in.
    pub fn plugin(&self) -> Option<PluginId> {
        let (ns, _) = self.0.split_once('.').expect("checked when built");
        PluginId::new(ns).ok()
    }
}

impl TryFrom<String> for FeatureTypeId {
    type Error = InvalidFeatureTypeId;

    fn try_from(id: String) -> Result<Self, InvalidFeatureTypeId> {
        Self::new(id)
    }
}

impl From<FeatureTypeId> for String {
    fn from(id: FeatureTypeId) -> String {
        id.0
    }
}

impl fmt::Display for FeatureTypeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A parameter's name: what an expression reads, so an identifier.
pub fn is_param_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// What makes a document malformed, whoever built it: a command that
/// would leave it so is refused, and a file that holds it is not opened.
#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum Invalid {
    #[error(transparent)]
    Dag(#[from] DagError),
    #[error("{0:?} is not a parameter name: a letter or `_`, then letters, digits or `_`")]
    ParamName(String),
    #[error("two parameters are named `{0}`")]
    DuplicateParam(String),
    #[error("two features of part {part} are named {name:?}")]
    DuplicateFeatureName { part: PartId, name: String },
    #[error("feature {0} is in more than one part")]
    FeatureInTwoParts(FeatureId),
    #[error("part {0}: its history and its features disagree")]
    History(PartId),
    #[error("part {part}: a record is filed under {key} but its id is {id}")]
    IdMismatch {
        part: PartId,
        key: String,
        id: String,
    },
    #[error("part {part}: the rollback index {at} is past its {len} features")]
    Rollback { part: PartId, at: usize, len: usize },
    #[error(
        "feature {feature}: dimension {constraint}'s expression {text:?} does not parse: {error}"
    )]
    SketchExpr {
        feature: FeatureId,
        constraint: SketchEntityId,
        text: String,
        error: String,
    },
}

impl Invalid {
    pub fn diagnostic(&self) -> Diagnostic {
        match self {
            Invalid::Dag(e) => e.diagnostic(),
            other => Diagnostic::new(Severity::Error, "doc.invalid", other.to_string())
                .expect("a well-formed code"),
        }
    }
}

impl Document {
    pub fn params(&self) -> &BTreeMap<ParamId, Param> {
        &self.params
    }

    pub fn parts(&self) -> &BTreeMap<PartId, Part> {
        &self.parts
    }

    pub fn param_by_name(&self, name: &str) -> Option<(ParamId, &Param)> {
        self.params
            .iter()
            .find(|(_, p)| p.name == name)
            .map(|(id, p)| (*id, p))
    }

    /// A feature, its part and its index in the part's history.
    pub fn feature(&self, id: FeatureId) -> Option<(&Part, usize, &FeatureRecord)> {
        self.parts.values().find_map(|part| {
            let record = part.features.get(&id)?;
            let at = part.history.iter().position(|f| *f == id)?;
            Some((part, at, record))
        })
    }

    /// Checks every invariant and returns the DAG it derives.
    pub fn validate(&self) -> Result<Dag, Invalid> {
        let mut names = BTreeSet::new();
        for p in self.params.values() {
            if !is_param_name(&p.name) {
                return Err(Invalid::ParamName(p.name.clone()));
            }
            if !names.insert(&p.name) {
                return Err(Invalid::DuplicateParam(p.name.clone()));
            }
        }
        let mut seen = BTreeSet::new();
        for (key, part) in &self.parts {
            validate_part(*key, part, &mut seen)?;
        }
        Ok(Dag::build(self)?)
    }
}

fn validate_part(key: PartId, part: &Part, seen: &mut BTreeSet<FeatureId>) -> Result<(), Invalid> {
    let mismatch = |key: String, id: String| Invalid::IdMismatch {
        part: part.id,
        key,
        id,
    };
    if key != part.id {
        return Err(mismatch(key.to_string(), part.id.to_string()));
    }
    let listed: BTreeSet<_> = part.history.iter().collect();
    if listed.len() != part.history.len() || !listed.iter().copied().eq(part.features.keys()) {
        return Err(Invalid::History(part.id));
    }
    let mut names = BTreeSet::new();
    for (fid, record) in &part.features {
        if *fid != record.id {
            return Err(mismatch(fid.to_string(), record.id.to_string()));
        }
        if !seen.insert(*fid) {
            return Err(Invalid::FeatureInTwoParts(*fid));
        }
        if !names.insert(&record.name) {
            return Err(Invalid::DuplicateFeatureName {
                part: part.id,
                name: record.name.clone(),
            });
        }
        sketch_exprs(record)?;
    }
    match part.rollback {
        Some(at) if at > part.history.len() => Err(Invalid::Rollback {
            part: part.id,
            at,
            len: part.history.len(),
        }),
        _ => Ok(()),
    }
}

/// A sketch's dimension expressions, parsed, by constraint. One that does
/// not parse makes the document malformed, as a feature's own field would.
pub(crate) fn sketch_exprs(record: &FeatureRecord) -> Result<Vec<(SketchEntityId, Expr)>, Invalid> {
    let Some(sketch) = &record.sketch else {
        return Ok(Vec::new());
    };
    sketch
        .constraints()
        .iter()
        .filter_map(|(id, c)| Some((*id, c.expr.as_deref()?)))
        .map(|(id, text)| {
            Expr::parse(text)
                .map(|e| (id, e))
                .map_err(|e| Invalid::SketchExpr {
                    feature: record.id,
                    constraint: id,
                    text: text.into(),
                    error: e.to_string(),
                })
        })
        .collect()
}
