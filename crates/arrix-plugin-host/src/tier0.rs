//! Tier 0 (docs/PLUGINS.md §Three tiers): a plugin compiled into the
//! binary, called directly through the plugin API's traits and nothing
//! else. Each type it describes is adapted onto the registry's
//! `FeatureType`, so it evaluates on the built-ins' one path.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use arrix_core::{Diagnostic, Severity};
use arrix_doc::{FeatureArgs, FeatureType, FeatureTypeId, Registry, RegistryError};
use arrix_plugin_api::{Feature, FeatureOutput, FeatureTypeSpec, Kernel};

use crate::manifest::{Manifest, ManifestError};

#[derive(Clone, Debug, PartialEq, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("plugin {plugin} is tier {tier}, not a compiled-in tier 0 plugin")]
    NotTier0 { plugin: String, tier: u8 },
    #[error(
        "plugin {plugin} was built against plugin API {api}, and this host has {}",
        arrix_plugin_api::API_VERSION
    )]
    Api { plugin: String, api: String },
    #[error("plugin {plugin} describes {type_id}, outside its namespace `{plugin}.`")]
    Namespace { plugin: String, type_id: String },
    #[error(transparent)]
    Registry(#[from] RegistryError),
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
    ) -> Result<FeatureOutput, Diagnostic> {
        if let Some(choice) = args.choices.keys().next() {
            return Err(error(
                "feature.unknown-choice",
                format!("{} has no choice `{choice}`", self.spec.id),
            ));
        }
        let run = AssertUnwindSafe(|| {
            self.feature
                .evaluate(kernel, &self.spec.id, &args.params, &args.inputs)
        });
        catch_unwind(run).unwrap_or_else(|payload| {
            Err(error(
                "plugin.panic",
                format!("{} panicked: {}", self.spec.id, panic_message(&*payload)),
            ))
        })
    }

    fn plugin_version(&self) -> Option<&str> {
        Some(&self.version)
    }
}

fn error(code: &str, message: String) -> Diagnostic {
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
    let plugin = m.id.to_string();
    if m.tier != 0 {
        return Err(HostError::NotTier0 {
            plugin,
            tier: m.tier,
        });
    }
    if !m.api_matches() {
        return Err(HostError::Api {
            plugin,
            api: m.api.to_string(),
        });
    }
    let mut next = registry.clone();
    let mut ids = Vec::new();
    for spec in feature.describe() {
        let id = FeatureTypeId::new(spec.id.clone()).map_err(RegistryError::from)?;
        if id.plugin().as_ref() != Some(&m.id) {
            return Err(HostError::Namespace {
                plugin,
                type_id: spec.id,
            });
        }
        ids.push(next.register(Arc::new(Tier0Type {
            spec,
            version: m.version.to_string(),
            feature: feature.clone(),
        }))?);
    }
    *registry = next;
    Ok(ids)
}
