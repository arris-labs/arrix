//! Materials: a density and an appearance colour, assigned to a part with
//! a per-body override (docs/DATA-MODEL.md §The document, ADR-0031). The
//! types here are stored data; mass is the evaluator's.

use std::fmt;

use arrix_core::{FeatureId, MaterialId, SlotName};
use serde::{Deserialize, Serialize};

use crate::expr::Expr;

/// A material. Names are unique within the document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub id: MaterialId,
    pub name: String,
    /// An expression of kind density, so a parameter may drive it. Unset
    /// for a material that came with a colour only (an imported STEP's,
    /// ADR-0032): a body of it has `mass.no-material`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Expr>,
    pub appearance: Rgb,
}

/// A colour: red, green and blue in 0 to 1.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Rgb(pub [f64; 3]);

impl Rgb {
    /// Whether every channel is in 0 to 1.
    pub fn is_valid(&self) -> bool {
        self.0.iter().all(|c| (0.0..=1.0).contains(c))
    }
}

/// A final body of a part: the body slot a feature made, named for a
/// per-body material. Written `<feature>.<slot>` in a file.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(try_from = "String", into = "String")]
pub struct SlotRef {
    pub feature: FeatureId,
    pub slot: SlotName,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a body: a feature id, a dot, then a slot name")]
pub struct InvalidSlotRef(pub String);

impl TryFrom<String> for SlotRef {
    type Error = InvalidSlotRef;

    fn try_from(text: String) -> Result<Self, InvalidSlotRef> {
        let bad = || InvalidSlotRef(text.clone());
        let (feature, slot) = text.split_once('.').ok_or_else(bad)?;
        Ok(Self {
            feature: feature.parse().map_err(|_| bad())?,
            slot: SlotName::new(slot).map_err(|_| bad())?,
        })
    }
}

impl From<SlotRef> for String {
    fn from(r: SlotRef) -> String {
        r.to_string()
    }
}

impl fmt::Display for SlotRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.feature, self.slot)
    }
}
