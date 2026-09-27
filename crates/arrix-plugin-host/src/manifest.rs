//! `arrix-plugin.toml` (docs/PLUGINS.md §The manifest): who a plugin is,
//! whatever its tier. Its `version` is the plugin's own and goes into
//! every input hash; its `api` range decides whether the host loads it.

use arrix_core::{InvalidPluginId, PluginId};
use serde::Deserialize;

/// A plugin's manifest, checked.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub id: PluginId,
    pub version: semver::Version,
    /// The `arrix-plugin-api` range it was built against.
    pub api: semver::VersionReq,
    pub tier: u8,
    pub title: String,
    pub licence: String,
    /// Tier 2's command line.
    pub command: Vec<String>,
    pub capabilities: Capabilities,
    pub contributes: Contributes,
}

/// What a plugin may reach (docs/PLUGINS.md §Capabilities). Everything is
/// off unless the manifest turns it on. Recorded here; enforced by the
/// tiers that sandbox (C3).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Capabilities {
    pub kernel: bool,
    pub document_read: bool,
    pub document_write: bool,
    pub files: Files,
    pub network: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Files {
    #[default]
    None,
    /// Only files the user picked in a host dialog.
    Picked,
}

/// What the plugin says it contributes. Informative: its exports are the
/// authority.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Contributes {
    pub features: Vec<String>,
    pub ribbon: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    plugin: RawPlugin,
    tier2: Option<RawTier2>,
    #[serde(default)]
    capabilities: Capabilities,
    #[serde(default)]
    contributes: Contributes,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawPlugin {
    id: String,
    version: String,
    api: String,
    tier: u8,
    title: String,
    licence: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTier2 {
    command: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum ManifestError {
    #[error("the manifest is not valid: {0}")]
    Toml(String),
    #[error(transparent)]
    Id(#[from] InvalidPluginId),
    #[error("`version` {0:?} is not a semantic version")]
    Version(String),
    #[error("`api` {0:?} is not a version range")]
    Api(String),
    #[error("tier {0} is not 0, 1 or 2")]
    Tier(u8),
    #[error("a tier 2 plugin needs a [tier2] command, and only tier 2 has one")]
    Command,
}

impl Manifest {
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        let raw: Raw = toml::from_str(text).map_err(|e| ManifestError::Toml(e.to_string()))?;
        let p = raw.plugin;
        let version =
            semver::Version::parse(&p.version).map_err(|_| ManifestError::Version(p.version))?;
        let api = semver::VersionReq::parse(&p.api).map_err(|_| ManifestError::Api(p.api))?;
        if p.tier > 2 {
            return Err(ManifestError::Tier(p.tier));
        }
        let command = match (p.tier, raw.tier2) {
            (2, Some(t)) if !t.command.is_empty() => t.command,
            (0 | 1, None) => Vec::new(),
            _ => return Err(ManifestError::Command),
        };
        Ok(Manifest {
            id: PluginId::new(p.id)?,
            version,
            api,
            tier: p.tier,
            title: p.title,
            licence: p.licence,
            command,
            capabilities: raw.capabilities,
            contributes: raw.contributes,
        })
    }

    /// Whether this host's plugin API is in the plugin's `api` range.
    pub fn api_matches(&self) -> bool {
        let host = semver::Version::parse(arrix_plugin_api::API_VERSION)
            .expect("the crate's version is semver");
        self.api.matches(&host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GEARS: &str = r#"
[plugin]
id = "gears"
version = "0.1.0"
api = "^0.3"
tier = 0
title = "Gears"
licence = "MIT OR Apache-2.0"

[capabilities]
kernel = true

[contributes]
features = ["gears.spur"]
"#;

    #[test]
    fn reads_the_documented_manifest() {
        let m = Manifest::parse(GEARS).unwrap();
        assert_eq!(m.id.as_str(), "gears");
        assert_eq!(m.version, semver::Version::new(0, 1, 0));
        assert_eq!(m.tier, 0);
        assert!(m.capabilities.kernel);
        assert!(!m.capabilities.network);
        assert_eq!(m.capabilities.files, Files::None);
        assert_eq!(m.contributes.features, ["gears.spur"]);
        assert!(m.api_matches());
    }

    #[test]
    fn refuses_what_is_malformed() {
        let with = |from: &str, to: &str| Manifest::parse(&GEARS.replace(from, to));
        assert!(matches!(
            with("id = \"gears\"", "id = \"Gears\""),
            Err(ManifestError::Id(_))
        ));
        assert!(matches!(
            with("0.1.0", "one"),
            Err(ManifestError::Version(_))
        ));
        assert!(matches!(
            with("^0.3", "about 1"),
            Err(ManifestError::Api(_))
        ));
        assert!(matches!(
            with("tier = 0", "tier = 3"),
            Err(ManifestError::Tier(3))
        ));
        assert!(matches!(
            with("tier = 0", "tier = 2"),
            Err(ManifestError::Command)
        ));
        assert!(matches!(
            with("kernel = true", "kernel = true\nclock = true"),
            Err(ManifestError::Toml(_))
        ));
        // A plugin built for 0.2 does not load on 0.3 (ADR-0007): 0.x
        // minors break.
        assert!(!with("^0.3", "^0.2").unwrap().api_matches());
    }
}
