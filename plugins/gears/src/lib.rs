//! First-party plugin `gears`: the spur gear feature (docs/ROADMAP.md §M1).
//! Depends on `arrix-plugin-api` alone, like any third-party plugin.
//!
//! `gears.spur` (0.1): standard involute teeth, no profile shift, no
//! backlash, no bore. Below the base circle a flank is a radial line (no
//! undercut is modelled). Each involute flank is a chain of circular arcs
//! within `module / 1000` of the true involute
//! ([`involute::TOLERANCE_PER_MODULE`]); the tolerance is the plugin's,
//! not a kernel setting.

pub mod involute;
mod spur;

use arrix_plugin_api::{
    Diagnostic, Feature, FeatureOutput, FeatureTypeSpec, Kernel, ParamValue, ResolvedInput,
    Severity,
};

pub use spur::TYPE_ID as SPUR;

/// The plugin's manifest, as a host reads it.
pub const MANIFEST: &str = include_str!("../arrix-plugin.toml");

/// The plugin's feature types.
pub struct Gears;

impl Feature for Gears {
    fn describe(&self) -> Vec<FeatureTypeSpec> {
        vec![spur::spec()]
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        type_id: &str,
        params: &[ParamValue],
        inputs: &[ResolvedInput],
    ) -> Result<FeatureOutput, Diagnostic> {
        match type_id {
            SPUR => spur::evaluate(kernel, params, inputs),
            _ => Err(Diagnostic::new(
                Severity::Error,
                "gears.unknown-type",
                format!("gears has no feature type {type_id}"),
            )
            .expect("a literal code is well-formed")),
        }
    }
}
