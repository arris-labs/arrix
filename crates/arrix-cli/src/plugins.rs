//! The Tier 0 plugins this binary is built with: an explicit list, since
//! linker registration does not work on wasm (docs/PLUGINS.md §Three
//! tiers). A build without a plugin leaves it off this list.

use std::sync::Arc;

use arrix_doc::Registry;
use arrix_plugin_host::register_tier0;

/// The core feature types and every compiled-in plugin's.
pub fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears))
        .expect("a first-party plugin fits the host it is built with");
    r
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_compiled_in_plugin_registers() {
        let ids: Vec<_> = super::registry()
            .ids()
            .map(|id| id.as_str().to_owned())
            .collect();
        assert_eq!(ids[0], "core.datum-plane");
    }
}
