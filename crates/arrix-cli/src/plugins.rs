//! The Tier 0 plugins this binary is built with: an explicit list, since
//! linker registration does not work on wasm (docs/PLUGINS.md §Three
//! tiers). A build without a plugin leaves it off this list.

use std::sync::Arc;

use arrix_doc::Registry;
use arrix_plugin_api::{Feature, FeatureTypeSpec};
use arrix_plugin_host::{Manifest, register_tier0};

/// Each compiled-in plugin: its manifest text and its feature.
fn tier0() -> Vec<(&'static str, Arc<dyn Feature>)> {
    vec![(arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears))]
}

/// The core feature types and every compiled-in plugin's.
pub fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    for (manifest, feature) in tier0() {
        register_tier0(&mut r, manifest, feature)
            .expect("a first-party plugin fits the host it is built with");
    }
    r
}

/// The manifests of the compiled-in plugins.
pub fn manifests() -> Vec<Manifest> {
    tier0()
        .iter()
        .map(|(text, _)| Manifest::parse(text).expect("a first-party manifest parses"))
        .collect()
}

/// The spec of every registered feature type, in registration order.
pub fn feature_types(registry: &Registry) -> Vec<FeatureTypeSpec> {
    registry
        .ids()
        .filter_map(|id| registry.get(id))
        .map(|ty| ty.spec().clone())
        .collect()
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
        assert_eq!(super::manifests()[0].id.as_str(), "gears");
    }
}
