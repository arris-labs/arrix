//! The plugin test kit (docs/PLUGINS.md §The test kit, ADR-0026): the checks
//! `arrix test` runs on a plugin of any tier, and that an author's own
//! `cargo test` runs on a Tier 0 or Tier 1 plugin with [`run_tier0`] and
//! [`run_tier1`]. One implementation, so the two cannot drift.

mod kit;
pub mod refs;
mod source;
mod tiers;

pub use kit::{Check, Env, Kit, MakeRegistry};
pub use source::{DirSource, Loaded};
pub use tiers::{Report, Tier0, run_tier0, run_tier1};
