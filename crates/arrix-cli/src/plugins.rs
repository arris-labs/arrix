//! The Tier 0 plugins this binary is built with: an explicit list, since
//! linker registration does not work on wasm (docs/PLUGINS.md §Three
//! tiers). A build without a plugin leaves it off this list.

use std::path::PathBuf;
use std::sync::Arc;

use arrix_doc::Registry;
use arrix_plugin_api::{Exporter, Feature, FeatureTypeSpec};
use arrix_plugin_host::{
    Manifest, manifest_in, register_dir, register_tier0, register_tier0_exporters,
};

/// A compiled-in plugin: its manifest text and what it contributes.
enum Compiled {
    Feature(&'static str, Arc<dyn Feature>),
    Exporter(&'static str, Arc<dyn Exporter>),
}

impl Compiled {
    fn manifest(&self) -> &'static str {
        match self {
            Self::Feature(text, _) | Self::Exporter(text, _) => text,
        }
    }
}

/// Each compiled-in plugin.
fn tier0() -> Vec<Compiled> {
    vec![
        Compiled::Feature(arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears)),
        Compiled::Exporter(arrix_robotics::MANIFEST, Arc::new(arrix_robotics::Robotics)),
    ]
}

/// The core feature types and every compiled-in plugin's.
pub fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    for plugin in tier0() {
        match plugin {
            Compiled::Feature(manifest, feature) => register_tier0(&mut r, manifest, feature),
            Compiled::Exporter(manifest, exporter) => {
                register_tier0_exporters(&mut r, manifest, exporter)
            }
        }
        .expect("a first-party plugin fits the host it is built with");
    }
    r
}

/// Loads each plugin directory of `dirs` (`--plugin`) at the tier its
/// manifest names and registers its feature types; returns their
/// manifests. A directory takes the place of a compiled-in plugin of the
/// same id (a Tier 1 `gears` for the Tier 0 one). The message names the
/// directory that could not be.
pub fn register_dirs(registry: &mut Registry, dirs: &[PathBuf]) -> Result<Vec<Manifest>, String> {
    let compiled = manifests();
    dirs.iter()
        .map(|dir| {
            let named = |e: &dyn std::fmt::Display| format!("--plugin {}: {e}", dir.display());
            let manifest = manifest_in(dir).map_err(|e| named(&e))?;
            if compiled.iter().any(|m| m.id == manifest.id) {
                registry.remove_plugin(&manifest.id);
            }
            register_dir(registry, dir).map_err(|e| named(&e))?;
            Ok(manifest)
        })
        .collect()
}

/// The manifests of the compiled-in plugins.
pub fn manifests() -> Vec<Manifest> {
    tier0()
        .iter()
        .map(|p| Manifest::parse(p.manifest()).expect("a first-party manifest parses"))
        .collect()
}

/// The spec of every feature type a command creates, in id order.
pub fn feature_types(registry: &Registry) -> Vec<FeatureTypeSpec> {
    registry.creatable_specs()
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_compiled_in_plugin_registers() {
        let ids: Vec<_> = super::registry()
            .ids()
            .map(|id| id.as_str().to_owned())
            .collect();
        assert_eq!(ids[0], "core.boolean");
        assert!(ids.iter().any(|id| id == "gears.spur"));
        assert_eq!(super::manifests()[0].id.as_str(), "gears");
        assert_eq!(super::manifests()[1].id.as_str(), "robotics");
        assert!(super::registry().exporter("robotics.urdf").is_some());
    }
}
