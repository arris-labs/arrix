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
    /// Tier 2's longest silence before a call is a `plugin.timeout`; the
    /// host's default when the manifest sets none.
    pub timeout: Option<std::time::Duration>,
    /// Tier 1's `[tier1]` table.
    pub tier1: Option<Tier1>,
    pub capabilities: Capabilities,
    pub contributes: Contributes,
    /// The fixtures `arrix test` runs the plugin on.
    pub test: TestFixtures,
}

/// The `[tier1]` table (ADR-0035): the component, and the fuel it may use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tier1 {
    /// The `.wasm`, relative to the plugin's directory and inside it.
    pub component: String,
    /// 1-based, in the manifest.
    pub line: u32,
    /// The fuel of one evaluation, when the manifest raises the default.
    pub fuel: Option<u64>,
}

/// The most fuel a manifest may ask for (ADR-0035 §5).
pub const MAX_FUEL: u64 = 1 << 36;

/// A path named in the manifest and the line it is on, for the diagnostic
/// of a fixture that is not there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fixture {
    /// Relative to the plugin's directory.
    pub path: String,
    /// 1-based, in the manifest.
    pub line: u32,
}

/// One `[[test.exports]]` entry: a fixture document, an exporter of the
/// plugin, the assembly or part it is run on (by name or id) and its
/// options as `arrix export --option` gives them (ADR-0037 §9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportFixture {
    pub document: Fixture,
    pub exporter: String,
    pub assembly: Option<String>,
    pub part: Option<String>,
    pub options: std::collections::BTreeMap<String, String>,
}

/// The `[test]` table (docs/PLUGINS.md §The test kit).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestFixtures {
    /// Documents that must evaluate, round-trip and open frozen.
    pub documents: Vec<Fixture>,
    /// Documents expected to fail with a `kernel.*` category.
    pub fail: Vec<Fixture>,
    /// Exports that must run twice to identical files.
    pub exports: Vec<ExportFixture>,
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
    /// Exporter ids (ADR-0037).
    pub exporters: Vec<String>,
    pub ribbon: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    plugin: RawPlugin,
    tier2: Option<RawTier2>,
    tier1: Option<RawTier1>,
    #[serde(default)]
    capabilities: Capabilities,
    #[serde(default)]
    contributes: Contributes,
    #[serde(default)]
    test: RawTest,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawTest {
    documents: Vec<toml::Spanned<String>>,
    fail: Vec<toml::Spanned<String>>,
    exports: Vec<RawExport>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawExport {
    document: toml::Spanned<String>,
    exporter: String,
    assembly: Option<String>,
    part: Option<String>,
    #[serde(default)]
    options: std::collections::BTreeMap<String, String>,
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
    #[serde(rename = "timeout-seconds")]
    timeout_seconds: Option<f64>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTier1 {
    component: toml::Spanned<String>,
    fuel: Option<toml::Spanned<i64>>,
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
    #[error("a tier 1 plugin needs a [tier1] component, and only tier 1 has one")]
    Component,
    #[error(
        "line {line}: `component` {path:?} is not a relative `.wasm` path inside the plugin's directory"
    )]
    ComponentPath { path: String, line: u32 },
    #[error("line {line}: `fuel` {fuel} is not between 1 and {MAX_FUEL}")]
    Fuel { fuel: i64, line: u32 },
    #[error("`timeout-seconds` {0} is not a positive number of seconds")]
    Timeout(f64),
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
        let line = |at: usize| 1 + text[..at].matches('\n').count() as u32;
        let tier1 = match (p.tier, raw.tier1) {
            (1, Some(t)) => {
                let component = t.component.get_ref().clone();
                let at = line(t.component.span().start);
                let path = std::path::Path::new(&component);
                let inside = path.is_relative()
                    && path.extension().is_some_and(|e| e == "wasm")
                    && path
                        .components()
                        .all(|c| matches!(c, std::path::Component::Normal(_)));
                if !inside {
                    return Err(ManifestError::ComponentPath {
                        path: component,
                        line: at,
                    });
                }
                let fuel = match t.fuel {
                    None => None,
                    Some(f) => match u64::try_from(*f.get_ref()) {
                        Ok(n) if (1..=MAX_FUEL).contains(&n) => Some(n),
                        _ => {
                            return Err(ManifestError::Fuel {
                                fuel: *f.get_ref(),
                                line: line(f.span().start),
                            });
                        }
                    },
                };
                Some(Tier1 {
                    component,
                    line: at,
                    fuel,
                })
            }
            (1, None) | (_, Some(_)) => return Err(ManifestError::Component),
            (_, None) => None,
        };
        let (command, timeout) = match (p.tier, raw.tier2) {
            (2, Some(t)) if !t.command.is_empty() => {
                let timeout = match t.timeout_seconds {
                    None => None,
                    Some(s) if s.is_finite() && s > 0.0 && s < 86_400.0 => {
                        Some(std::time::Duration::from_secs_f64(s))
                    }
                    Some(s) => return Err(ManifestError::Timeout(s)),
                };
                (t.command, timeout)
            }
            (0 | 1, None) => (Vec::new(), None),
            _ => return Err(ManifestError::Command),
        };
        let fixture = |f: toml::Spanned<String>| Fixture {
            line: 1 + text[..f.span().start].matches('\n').count() as u32,
            path: f.into_inner(),
        };
        let fixtures = |list: Vec<toml::Spanned<String>>| list.into_iter().map(fixture).collect();
        Ok(Manifest {
            id: PluginId::new(p.id)?,
            version,
            api,
            tier: p.tier,
            title: p.title,
            licence: p.licence,
            command,
            timeout,
            tier1,
            capabilities: raw.capabilities,
            contributes: raw.contributes,
            test: TestFixtures {
                documents: fixtures(raw.test.documents),
                fail: fixtures(raw.test.fail),
                exports: raw
                    .test
                    .exports
                    .into_iter()
                    .map(|e| ExportFixture {
                        document: fixture(e.document),
                        exporter: e.exporter,
                        assembly: e.assembly,
                        part: e.part,
                        options: e.options,
                    })
                    .collect(),
            },
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
api = "^0.7"
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
    fn reads_the_test_table_with_the_line_of_each_fixture() {
        let m = Manifest::parse(&format!(
            "{GEARS}\n[test]\ndocuments = [\n  \"a\",\n  \"b/c\",\n]\nfail = [\"f\"]\n"
        ))
        .unwrap();
        let at = |f: &[Fixture]| {
            f.iter()
                .map(|f| (f.path.clone(), f.line))
                .collect::<Vec<_>>()
        };
        let first = GEARS.lines().count() as u32 + 4;
        assert_eq!(
            at(&m.test.documents),
            [("a".into(), first), ("b/c".into(), first + 1)]
        );
        assert_eq!(at(&m.test.fail), [("f".into(), first + 3)]);
        assert!(matches!(
            Manifest::parse(&format!("{GEARS}\n[test]\nbogus = 1\n")),
            Err(ManifestError::Toml(_))
        ));
    }

    #[test]
    fn reads_a_tier2_timeout() {
        let t2 = |extra: &str| {
            Manifest::parse(&GEARS.replace("tier = 0", "tier = 2").replace(
                "[capabilities]",
                &format!("[tier2]\ncommand = [\"p\"]\n{extra}\n[capabilities]"),
            ))
        };
        assert_eq!(t2("").unwrap().timeout, None);
        let m = t2("timeout-seconds = 0.5").unwrap();
        assert_eq!(m.timeout, Some(std::time::Duration::from_millis(500)));
        for bad in ["0", "-1", "1e9"] {
            assert!(matches!(
                t2(&format!("timeout-seconds = {bad}")),
                Err(ManifestError::Timeout(_))
            ));
        }
    }

    #[test]
    fn reads_a_tier1_table_and_refuses_it_elsewhere() {
        let t1 = |table: &str| {
            Manifest::parse(
                &GEARS
                    .replace("tier = 0", "tier = 1")
                    .replace("[capabilities]", &format!("{table}\n[capabilities]")),
            )
        };
        let first = 11; // the line after `[capabilities]`'s old place
        let m = t1("[tier1]\ncomponent = \"gears.wasm\"").unwrap();
        let t = m.tier1.unwrap();
        assert_eq!((t.component.as_str(), t.fuel), ("gears.wasm", None));
        assert_eq!(t.line, first);
        let m = t1("[tier1]\ncomponent = \"lib/g.wasm\"\nfuel = 5000000000").unwrap();
        assert_eq!(m.tier1.unwrap().fuel, Some(5_000_000_000));
        assert!(Manifest::parse(GEARS).unwrap().tier1.is_none());

        // Required on tier 1, refused on any other.
        assert!(matches!(t1(""), Err(ManifestError::Component)));
        let on_tier0 = GEARS.replace(
            "[capabilities]",
            "[tier1]\ncomponent = \"g.wasm\"\n[capabilities]",
        );
        assert!(matches!(
            Manifest::parse(&on_tier0),
            Err(ManifestError::Component)
        ));

        // A path that leaves the directory, is absolute, or is no `.wasm`.
        for bad in ["../g.wasm", "/g.wasm", "g.so", "a/../g.wasm"] {
            assert!(
                matches!(
                    t1(&format!("[tier1]\ncomponent = {bad:?}")),
                    Err(ManifestError::ComponentPath { line, .. }) if line == first
                ),
                "{bad}"
            );
        }
        // Fuel is a positive number within the cap, and the refusal names its line.
        for bad in ["0", "-1", "68719476737"] {
            assert!(
                matches!(
                    t1(&format!("[tier1]\ncomponent = \"g.wasm\"\nfuel = {bad}")),
                    Err(ManifestError::Fuel { line, .. }) if line == first + 1
                ),
                "{bad}"
            );
        }
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
            with("^0.7", "about 1"),
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
        // A plugin built for 0.3 does not load on 0.7 (ADR-0007, ADR-0012):
        // 0.x minors break.
        assert!(!with("^0.7", "^0.3").unwrap().api_matches());
        assert!(!with("^0.7", "^0.2").unwrap().api_matches());
    }
}
