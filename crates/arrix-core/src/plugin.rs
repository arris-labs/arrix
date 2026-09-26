//! A plugin's id: the namespace of everything it contributes, so
//! `gears` owns `gears.spur` (docs/PLUGINS.md §The manifest).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// `[a-z][a-z0-9-]*`, and never `core`, which the built-ins own.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PluginId(String);

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InvalidPluginId {
    #[error(
        "{0:?} is not a plugin id: a lower-case letter, then lower-case letters, digits or hyphens"
    )]
    Grammar(String),
    #[error("`core` is the built-ins' namespace, not a plugin's")]
    Reserved,
}

impl PluginId {
    pub fn new(id: impl Into<String>) -> Result<Self, InvalidPluginId> {
        let id = id.into();
        let well_formed = id.starts_with(|c: char| c.is_ascii_lowercase())
            && id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !well_formed {
            return Err(InvalidPluginId::Grammar(id));
        }
        if id == "core" {
            return Err(InvalidPluginId::Reserved);
        }
        Ok(Self(id))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The id of something this plugin contributes: `<plugin>.<name>`.
    pub fn qualify(&self, name: &str) -> String {
        format!("{}.{name}", self.0)
    }
}

impl FromStr for PluginId {
    type Err = InvalidPluginId;

    fn from_str(s: &str) -> Result<Self, InvalidPluginId> {
        Self::new(s)
    }
}

impl TryFrom<String> for PluginId {
    type Error = InvalidPluginId;

    fn try_from(id: String) -> Result<Self, InvalidPluginId> {
        Self::new(id)
    }
}

impl From<PluginId> for String {
    fn from(id: PluginId) -> String {
        id.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_the_manifest_grammar() {
        for id in ["gears", "robotics", "acme-fasteners", "a2"] {
            assert_eq!(PluginId::new(id).unwrap().as_str(), id);
        }
        let gears: PluginId = "gears".parse().unwrap();
        assert_eq!(gears.qualify("spur"), "gears.spur");
    }

    #[test]
    fn refuses_the_rest() {
        for id in ["", "Gears", "2d", "-x", "gears.spur", "gear_s", "gears "] {
            assert_eq!(PluginId::new(id), Err(InvalidPluginId::Grammar(id.into())));
        }
        assert_eq!(PluginId::new("core"), Err(InvalidPluginId::Reserved));
        assert!(serde_json::from_str::<PluginId>("\"core\"").is_err());
        assert_eq!(
            serde_json::from_str::<PluginId>("\"gears\"").unwrap(),
            PluginId::new("gears").unwrap()
        );
    }
}
