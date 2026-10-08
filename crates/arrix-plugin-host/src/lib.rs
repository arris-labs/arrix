//! Plugin hosting (tiers 0, 1 and 2), manifests, capabilities and the frozen
//! fallback (docs/PLUGINS.md §Three tiers).
//!
//! Built so far: manifests, the wire's codec and framing (ADR-0022), and Tier 0 (a compiled-in plugin's feature
//! types adapted onto the registry, a panic caught as a diagnostic).

mod manifest;
mod tier0;
pub mod wire;

pub use manifest::{Capabilities, Contributes, Files, Manifest, ManifestError};
pub use tier0::{HostError, register_tier0};
