//! Plugin hosting (tiers 0, 1 and 2), manifests, capabilities and the frozen
//! fallback (docs/PLUGINS.md §Three tiers).
//!
//! Built so far: manifests, the wire's codec and framing (ADR-0022), Tier 0
//! (a compiled-in plugin's feature types adapted onto the registry, a panic
//! caught as a diagnostic), Tier 1 (a WebAssembly component's, run in
//! wasmtime, ADR-0035; native only) and Tier 2 (a child process's feature
//! types, building by callback on the host's kernel, ADR-0021; native
//! only).

#[cfg(not(target_arch = "wasm32"))]
mod loader;
mod manifest;
mod tier0;
#[cfg(not(target_arch = "wasm32"))]
mod tier1;
#[cfg(not(target_arch = "wasm32"))]
mod tier2;
pub mod wire;

#[cfg(not(target_arch = "wasm32"))]
pub use loader::{DirLoader, manifest_in, register_dir};
pub use manifest::{
    Capabilities, Contributes, Files, Fixture, Manifest, ManifestError, TestFixtures,
};
pub use tier0::{HostError, register_tier0};
#[cfg(not(target_arch = "wasm32"))]
pub use tier1::{DEFAULT_FUEL, register_tier1};
#[cfg(not(target_arch = "wasm32"))]
pub use tier2::{DEFAULT_TIMEOUT, MANIFEST, register_tier2};
