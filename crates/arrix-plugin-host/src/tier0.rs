//! Tier 0 (docs/PLUGINS.md §Three tiers): a plugin compiled into the
//! binary, called directly through the plugin API's traits and nothing
//! else. Each type it describes is adapted onto the registry's
//! `FeatureType`, so it evaluates on the built-ins' one path.

use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;

use arrix_core::{Diagnostic, Severity};
use arrix_doc::{FeatureArgs, FeatureType, FeatureTypeId, Registry, RegistryError, TypeOutput};
use arrix_plugin_api::{ChoiceValue, Feature, FeatureTypeSpec, Kernel};

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
    #[error(transparent)]
    Registry(#[from] RegistryError),
    /// A Tier 2 plugin that could not be started or read, or that broke
    /// the protocol before its types were known.
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
