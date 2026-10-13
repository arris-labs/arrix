//! The `dump` exporters as a Tier 1 component.

mod dump;

#[cfg(all(feature = "guest", target_arch = "wasm32"))]
arrix_plugin_api::export_plugin!(exporters = dump::Dump);
