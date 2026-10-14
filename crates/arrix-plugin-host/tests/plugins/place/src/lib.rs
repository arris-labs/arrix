//! The `place` commands as a Tier 1 component.

mod place;

#[cfg(all(feature = "guest", target_arch = "wasm32"))]
arrix_plugin_api::export_plugin!(commands = place::Place);
