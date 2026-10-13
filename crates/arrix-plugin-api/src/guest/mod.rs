//! The guest SDK (ADR-0035): a Tier 1 plugin written against the Rust
//! traits. Behind the `guest` feature, because the bindings are generated
//! code a Tier 0 plugin never needs.
//!
//! A plugin enables the feature, keeps `forbid(unsafe_code)`, and writes one
//! line, `arrix_plugin_api::export_plugin!(MyFeature);`, for a unit struct
//! implementing [`Feature`]. Built as a `cdylib` for
//! `wasm32-unknown-unknown`, the host componentises it at load. The export
//! glue expands in the plugin's crate from this crate's macro, which is how
//! no crate here or there needs an `unsafe` exception.
//!
//! The submodules are the world and the Rust types converted both ways, and
//! the Rust `Kernel` over the world's imports; `tests/wit_equality.rs`
//! holds them equal to the world.

use std::marker::PhantomData;

use crate::Feature;

mod convert;
mod kernel;

#[doc(hidden)]
pub use convert::{Mirror, input_from_wit, input_to_wit, output_from_wit, output_to_wit};
#[doc(hidden)]
pub use kernel::WitKernel;

/// The world's bindings, generated from `wit/arrix-plugin.wit`.
#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "plugin",
        pub_export_macro: true,
        export_macro_name: "__arrix_export_world",
    });

    pub use arrix::plugin::{kernel, types};
    pub use exports::arrix::plugin::feature;
}

use bindings::arrix::plugin::types as wt;
use bindings::exports::arrix::plugin::feature as wf;

/// What [`export_plugin!`](crate::export_plugin) names: the plugin's
/// [`Feature`].
pub trait Plugin {
    fn feature() -> &'static dyn Feature;
}

/// The world's `feature` export over a [`Plugin`]: each `evaluate` runs on a
/// kernel of its own, whose body table dies with the call.
pub struct Exported<P>(PhantomData<P>);

impl<P: Plugin> wf::Guest for Exported<P> {
    fn describe() -> Vec<wf::FeatureTypeSpec> {
        P::feature().describe().to_wit()
    }

    fn evaluate(
        type_id: String,
        params: Vec<wf::ParamValue>,
        choices: Vec<wf::ChoiceValue>,
        inputs: Vec<wf::ResolvedInput>,
    ) -> Result<wf::FeatureOutput, wt::Diagnostic> {
        let mut kernel = WitKernel::default();
        let inputs: Vec<_> = inputs
            .into_iter()
            .map(|i| input_from_wit(i, &mut kernel))
            .collect();
        let out = P::feature().evaluate(
            &mut kernel,
            &type_id,
            &Vec::from_wit(params),
            &Vec::from_wit(choices),
            &inputs,
        );
        match out {
            Ok(out) => Ok(output_to_wit(out, &mut kernel)),
            Err(d) => Err(d.to_wit()),
        }
    }
}

/// Exports `$plugin`, a unit struct implementing [`Feature`], as the world's
/// `feature` interface. Once per cdylib.
#[macro_export]
macro_rules! export_plugin {
    ($plugin:path) => {
        struct __ArrixPlugin;
        impl $crate::guest::Plugin for __ArrixPlugin {
            fn feature() -> &'static dyn $crate::Feature {
                &$plugin
            }
        }
        type __ArrixExported = $crate::guest::Exported<__ArrixPlugin>;
        $crate::guest::bindings::__arrix_export_world!(
            __ArrixExported with_types_in $crate::guest::bindings
        );
    };
}
