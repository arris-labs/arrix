//! How one record points at another (docs/DATA-MODEL.md §References):
//! plain data that resolves or fails, and never guesses. Every `Ref` is an
//! edge of the dependency DAG.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::diagnostic::is_segment;
use crate::{FeatureId, ParamId, PersistentName, PluginId, RecordId, SketchEntityId};

/// A feature's named output: `body`, `plane`, `sketch`. One lower-kebab
/// segment, as a diagnostic code's segments are.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SlotName(String);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "{0:?} is not a slot name: lower-case letters and digits in hyphen-joined words, \
     starting with a letter"
)]
pub struct InvalidSlotName(pub String);

impl SlotName {
    pub fn new(name: impl Into<String>) -> Result<Self, InvalidSlotName> {
        let name = name.into();
        if is_segment(&name) {
            Ok(Self(name))
        } else {
            Err(InvalidSlotName(name))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SlotName {
    type Error = InvalidSlotName;

    fn try_from(name: String) -> Result<Self, InvalidSlotName> {
        Self::new(name)
    }
}

impl From<SlotName> for String {
    fn from(name: SlotName) -> String {
        name.0
    }
}

impl fmt::Display for SlotName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A reference from one record to another. `Topo` is the one naming
/// solves; the rest are by id.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Ref {
    Param(ParamId),
    Feature(FeatureId),
    /// A body, datum or sketch output of a feature.
    Slot {
        feature: FeatureId,
        slot: SlotName,
    },
    /// A face, edge or vertex.
    Topo(PersistentName),
    /// A point or curve of a sketch.
    Sketch {
        feature: FeatureId,
        entity: SketchEntityId,
    },
    /// A plugin's document record.
    Plugin {
        plugin: PluginId,
        record: RecordId,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Id;

    #[test]
    fn slot_names_follow_the_segment_grammar() {
        for ok in ["body", "plane", "tool-2"] {
            assert_eq!(SlotName::new(ok).unwrap().as_str(), ok);
        }
        for bad in ["", "Body", "2d", "a.b", "a_b", "a-"] {
            assert_eq!(SlotName::new(bad), Err(InvalidSlotName(bad.into())));
        }
        assert!(serde_json::from_str::<SlotName>("\"Body\"").is_err());
    }

    #[test]
    fn every_ref_crosses_as_json() {
        let f = FeatureId(Id(1));
        let refs = [
            (Ref::Param(ParamId(Id(2))), r#"{"param":"0000000000002"}"#),
            (Ref::Feature(f), r#"{"feature":"0000000000001"}"#),
            (
                Ref::Slot {
                    feature: f,
                    slot: SlotName::new("body").unwrap(),
                },
                r#"{"slot":{"feature":"0000000000001","slot":"body"}}"#,
            ),
            (
                Ref::Topo("face:sweep.0000000000001.end-cap".parse().unwrap()),
                r#"{"topo":"face:sweep.0000000000001.end-cap"}"#,
            ),
            (
                Ref::Sketch {
                    feature: f,
                    entity: SketchEntityId(Id(3)),
                },
                r#"{"sketch":{"feature":"0000000000001","entity":"0000000000003"}}"#,
            ),
            (
                Ref::Plugin {
                    plugin: PluginId::new("gears").unwrap(),
                    record: RecordId(Id(4)),
                },
                r#"{"plugin":{"plugin":"gears","record":"0000000000004"}}"#,
            ),
        ];
        for (r, json) in refs {
            assert_eq!(serde_json::to_string(&r).unwrap(), json);
            assert_eq!(serde_json::from_str::<Ref>(json).unwrap(), r);
        }
    }
}
