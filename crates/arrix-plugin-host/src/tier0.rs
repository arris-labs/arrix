//! Tier 0 (docs/PLUGINS.md §Three tiers): a plugin compiled into the
//! binary, called directly through the plugin API's traits and nothing
//! else. Each type it describes is adapted onto the registry's
//! `FeatureType`, so it evaluates on the built-ins' one path.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use arrix_core::{Diagnostic, Severity};
use arrix_doc::{
    ExporterType, FeatureArgs, FeatureType, FeatureTypeId, Registry, RegistryError, TypeOutput,
};
use arrix_plugin_api::{
    ChoiceValue, ExportFile, Exporter, ExporterSpec, Feature, FeatureTypeSpec, Kernel, ParamValue,
    Snapshot, Subject,
};

use crate::manifest::{Manifest, ManifestError};

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("plugin {plugin} is tier {tier}, not tier {expected}")]
    Tier {
        plugin: String,
        tier: u8,
        expected: u8,
    },
    #[error(
        "plugin {plugin} was built against plugin API {api}, and this host has {}",
        arrix_plugin_api::API_VERSION
    )]
    Api { plugin: String, api: String },
    #[error("plugin {plugin} describes {type_id}, outside its namespace `{plugin}.`")]
    Namespace { plugin: String, type_id: String },
    #[error("plugin {plugin} describes exporters, and its manifest does not grant `document-read`")]
    Capability { plugin: String },
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// A Tier 1 or Tier 2 plugin that could not be loaded, started or
    /// read, or that broke the protocol before its types were known.
    #[error("plugin {plugin}: {message}")]
    Process { plugin: String, message: String },
}

/// One feature type of a Tier 0 plugin.
struct Tier0Type {
    spec: FeatureTypeSpec,
    version: String,
    feature: Arc<dyn Feature>,
}

impl FeatureType for Tier0Type {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    /// The plugin's `evaluate`, with a panic caught and made the feature's
    /// diagnostic. Native only: a wasm32 build aborts on a panic.
    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let choices: Vec<_> = args
            .choices
            .iter()
            .map(|(name, value)| ChoiceValue {
                name: name.clone(),
                value: value.clone(),
            })
            .collect();
        let run = AssertUnwindSafe(|| {
            self.feature
                .evaluate(kernel, &self.spec.id, &args.params, &choices, &args.inputs)
        });
        catch_unwind(run)
            .unwrap_or_else(|payload| {
                Err(error(
                    "plugin.panic",
                    format!("{} panicked: {}", self.spec.id, panic_message(&*payload)),
                ))
            })
            .map(TypeOutput::from)
    }

    fn plugin_version(&self) -> Option<&str> {
        Some(&self.version)
    }
}

/// One exporter of a Tier 0 plugin.
struct Tier0Exporter {
    spec: ExporterSpec,
    exporter: Arc<dyn Exporter>,
}

impl ExporterType for Tier0Exporter {
    fn spec(&self) -> &ExporterSpec {
        &self.spec
    }

    /// The plugin's `export`, with a panic caught and made the export's
    /// diagnostic. Native only.
    fn export(
        &self,
        snapshot: &mut dyn Snapshot,
        subject: &Subject,
        params: &[ParamValue],
        choices: &[ChoiceValue],
    ) -> Result<Vec<ExportFile>, Diagnostic> {
        let run = AssertUnwindSafe(|| {
            self.exporter
                .export(snapshot, &self.spec.id, subject, params, choices)
        });
        catch_unwind(run).unwrap_or_else(|payload| {
            Err(error(
                "plugin.panic",
                format!("{} panicked: {}", self.spec.id, panic_message(&*payload)),
            ))
        })
    }
}

pub(crate) fn error(code: &str, message: String) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

fn panic_message(payload: &(dyn Any + Send)) -> &str {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("a non-text panic payload")
}

/// Registers every feature type a Tier 0 plugin describes, or none of
/// them. `manifest` is the plugin's `arrix-plugin.toml`.
pub fn register_tier0(
    registry: &mut Registry,
    manifest: &str,
    feature: Arc<dyn Feature>,
) -> Result<Vec<FeatureTypeId>, HostError> {
    let m = Manifest::parse(manifest)?;
    check(&m, 0)?;
    let version = m.version.to_string();
    register_types(registry, &m, feature.describe(), |spec| {
        Arc::new(Tier0Type {
            spec,
            version: version.clone(),
            feature: feature.clone(),
        })
    })
}

/// Registers every exporter a Tier 0 plugin describes, or none of them
/// (ADR-0037). The manifest must grant `document-read`: a Tier 0 plugin is
/// trusted, so its capabilities are declarative, and the host refuses the
/// plugin that would read the snapshot without declaring it.
pub fn register_tier0_exporters(
    registry: &mut Registry,
    manifest: &str,
    exporter: Arc<dyn Exporter>,
) -> Result<Vec<FeatureTypeId>, HostError> {
    let m = Manifest::parse(manifest)?;
    check(&m, 0)?;
    register_exporters(registry, &m, exporter.exporters(), |spec| {
        Arc::new(Tier0Exporter {
            spec,
            exporter: exporter.clone(),
        })
    })
}

/// Registers `specs`, each adapted by `adapt`, all under the plugin's
/// namespace, or none of them.
pub(crate) fn register_exporters(
    registry: &mut Registry,
    m: &Manifest,
    specs: Vec<ExporterSpec>,
    adapt: impl Fn(ExporterSpec) -> Arc<dyn ExporterType>,
) -> Result<Vec<FeatureTypeId>, HostError> {
    if !specs.is_empty() && !m.capabilities.document_read {
        return Err(HostError::Capability {
            plugin: m.id.to_string(),
        });
    }
    let mut next = registry.clone();
    let mut ids = Vec::new();
    for spec in specs {
        let id = FeatureTypeId::new(spec.id.clone()).map_err(RegistryError::from)?;
        if id.plugin().as_ref() != Some(&m.id) {
            return Err(HostError::Namespace {
                plugin: m.id.to_string(),
                type_id: spec.id,
            });
        }
        ids.push(next.register_exporter(adapt(spec))?);
    }
    *registry = next;
    Ok(ids)
}

/// Refuses a manifest of another tier, or built against another plugin API.
pub(crate) fn check(m: &Manifest, tier: u8) -> Result<(), HostError> {
    let plugin = m.id.to_string();
    if m.tier != tier {
        return Err(HostError::Tier {
            plugin,
            tier: m.tier,
            expected: tier,
        });
    }
    if !m.api_matches() {
        return Err(HostError::Api {
            plugin,
            api: m.api.to_string(),
        });
    }
    Ok(())
}

/// Registers `specs`, each adapted by `adapt`, all under the plugin's
/// namespace, or none of them.
pub(crate) fn register_types(
    registry: &mut Registry,
    m: &Manifest,
    specs: Vec<FeatureTypeSpec>,
    adapt: impl Fn(FeatureTypeSpec) -> Arc<dyn FeatureType>,
) -> Result<Vec<FeatureTypeId>, HostError> {
    let mut next = registry.clone();
    let mut ids = Vec::new();
    for spec in specs {
        let id = FeatureTypeId::new(spec.id.clone()).map_err(RegistryError::from)?;
        if id.plugin().as_ref() != Some(&m.id) {
            return Err(HostError::Namespace {
                plugin: m.id.to_string(),
                type_id: spec.id,
            });
        }
        ids.push(next.register(adapt(spec))?);
    }
    *registry = next;
    Ok(ids)
}
