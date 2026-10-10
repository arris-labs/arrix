//! Plugin hosting (tiers 0, 1 and 2), manifests, capabilities and the frozen
//! fallback (docs/PLUGINS.md §Three tiers).
//!
//! Built so far: manifests, the wire's codec and framing (ADR-0022), Tier 0
//! (a compiled-in plugin's feature types adapted onto the registry, a panic
//! caught as a diagnostic) and Tier 2 (a child process's feature types,
//! building by callback on the host's kernel, ADR-0021; native only).

#[cfg(not(target_arch = "wasm32"))]
mod loader;
mod manifest;
mod tier0;
#[cfg(not(target_arch = "wasm32"))]
mod tier2;
pub mod wire;

#[cfg(not(target_arch = "wasm32"))]
pub use loader::Tier2Loader;
pub use manifest::{Capabilities, Contributes, Files, Manifest, ManifestError};
pub use tier0::{HostError, register_tier0};
#[cfg(not(target_arch = "wasm32"))]
pub use tier2::{DEFAULT_TIMEOUT, MANIFEST, register_tier2};
