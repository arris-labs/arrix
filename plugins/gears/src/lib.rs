//! First-party plugin `gears`: the spur gear feature (docs/ROADMAP.md §M1).
//! Depends on `arrix-plugin-api` alone, like any third-party plugin.

use arrix_plugin_api::{
    Diagnostic, Feature, FeatureOutput, FeatureTypeSpec, Kernel, ParamValue, ResolvedInput,
    Severity,
};

/// The plugin's manifest, as a host reads it.
pub const MANIFEST: &str = include_str!("../arrix-plugin.toml");

/// The plugin's feature types.
pub struct Gears;

impl Feature for Gears {
    fn describe(&self) -> Vec<FeatureTypeSpec> {
        Vec::new()
    }

    fn evaluate(
        &self,
        _kernel: &mut dyn Kernel,
        type_id: &str,
        _params: &[ParamValue],
        _inputs: &[ResolvedInput],
    ) -> Result<FeatureOutput, Diagnostic> {
        Err(Diagnostic::new(
            Severity::Error,
            "gears.unknown-type",
            format!("gears has no feature type {type_id}"),
        )
        .expect("a literal code is well-formed"))
    }
}
