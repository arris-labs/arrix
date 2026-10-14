//! The guest SDK (ADR-0035): a Tier 1 plugin written against the Rust
//! traits. Behind the `guest` feature, because the bindings are generated
//! code a Tier 0 plugin never needs.
//!
//! A plugin enables the feature, keeps `forbid(unsafe_code)`, and writes one
//! line: `arrix_plugin_api::export_plugin!(MyFeature);` for a unit struct
//! implementing [`Feature`], or `export_plugin!(features = A, exporters =
//! B)` naming a [`Feature`], an [`Exporter`] or both (ADR-0037 §2). Built as
//! a `cdylib` for `wasm32-unknown-unknown`, the host componentises it at
//! load. The export glue expands in the plugin's crate from this crate's
//! macro, which is how no crate here or there needs an `unsafe` exception.
//!
//! A part the plugin does not name is supplied empty, and generates no call
//! to the import it would use: a plugin with no exporter does not import
//! `document`, and one with no feature does not import `kernel`, so the
//! host can refuse a component by its imports.
//!
//! The submodules are the world and the Rust types converted both ways, the
//! Rust `Kernel` over the world's `kernel` import and the Rust `Snapshot`
//! over `document`; `tests/wit_equality.rs` holds them equal to the world.

use std::marker::PhantomData;

use arrix_core::{Diagnostic, Severity};

use crate::{Commands, Exporter, Feature};

mod command;
mod convert;
mod kernel;
mod snapshot;

#[doc(hidden)]
pub use convert::{Mirror, input_from_wit, input_to_wit, output_from_wit, output_to_wit};
#[doc(hidden)]
pub use kernel::WitKernel;
#[doc(hidden)]
pub use snapshot::WitSnapshot;

/// The world's bindings, generated from `wit/arrix-plugin.wit`.
#[doc(hidden)]
pub mod bindings {
    wit_bindgen::generate!({
        path: "wit",
        world: "plugin",
        pub_export_macro: true,
        export_macro_name: "__arrix_export_world",
    });

    pub use arrix::plugin::{document, kernel, types};
    pub use exports::arrix::plugin::{commands, exchange, feature};
}

use bindings::arrix::plugin::types as wt;
use bindings::commands as wc;
use bindings::exchange as wx;
use bindings::feature as wf;

/// What [`export_plugin!`](crate::export_plugin) names of a plugin: the
/// glue of its features, exporters and commands, each [`NoFeatures`],
/// [`NoExporters`] or [`NoCommands`] when it names none.
pub trait Plugin {
    type Features: FeatureGlue;
    type Exporters: ExporterGlue;
    type Commands: CommandGlue;
}

/// A plugin's [`Feature`], named by the macro.
pub trait FeatureSource {
    fn feature() -> &'static dyn Feature;
}

/// A plugin's [`Exporter`], named by the macro.
pub trait ExporterSource {
    fn exporter() -> &'static dyn Exporter;
}

/// A plugin's [`Commands`], named by the macro.
pub trait CommandSource {
    fn commands() -> &'static dyn Commands;
}

/// The world's `feature` export over a plugin's features.
pub trait FeatureGlue {
    fn describe() -> Vec<wf::FeatureTypeSpec>;

    fn evaluate(
        type_id: String,
        params: Vec<wf::ParamValue>,
        choices: Vec<wf::ChoiceValue>,
        inputs: Vec<wf::ResolvedInput>,
    ) -> Result<wf::FeatureOutput, wt::Diagnostic>;
}

/// The world's `exchange` export over a plugin's exporters.
pub trait ExporterGlue {
    fn exporters() -> Vec<wx::ExporterSpec>;

    fn run(
        exporter: String,
        subject: wx::Subject,
        params: Vec<wf::ParamValue>,
        choices: Vec<wf::ChoiceValue>,
    ) -> Result<Vec<wx::File>, wt::Diagnostic>;
}

/// The world's `commands` export over a plugin's commands.
pub trait CommandGlue {
    fn commands() -> Vec<wc::CommandSpec>;

    fn expand(
        command: String,
        context: wc::Context,
        params: Vec<wt::ParamValue>,
        choices: Vec<wt::ChoiceValue>,
        inputs: Vec<wc::CommandInput>,
        ids: Vec<u64>,
    ) -> Result<wc::CommandGroup, wt::Diagnostic>;
}

fn nothing(code: &str, what: &str) -> wt::Diagnostic {
    Diagnostic::new(Severity::Error, code, format!("this plugin has no {what}"))
        .unwrap()
        .to_wit()
}

/// A plugin with no feature types. Calls no import.
pub struct NoFeatures;

impl FeatureGlue for NoFeatures {
    fn describe() -> Vec<wf::FeatureTypeSpec> {
        Vec::new()
    }

    fn evaluate(
        _: String,
        _: Vec<wf::ParamValue>,
        _: Vec<wf::ChoiceValue>,
        _: Vec<wf::ResolvedInput>,
    ) -> Result<wf::FeatureOutput, wt::Diagnostic> {
        Err(nothing("plugin.no-feature", "feature types"))
    }
}

/// A plugin with no exporters. Calls no import.
pub struct NoExporters;

impl ExporterGlue for NoExporters {
    fn exporters() -> Vec<wx::ExporterSpec> {
        Vec::new()
    }

    fn run(
        _: String,
        _: wx::Subject,
        _: Vec<wf::ParamValue>,
        _: Vec<wf::ChoiceValue>,
    ) -> Result<Vec<wx::File>, wt::Diagnostic> {
        Err(nothing("plugin.no-exporter", "exporters"))
    }
}

/// A plugin with no commands. Calls no import.
pub struct NoCommands;

impl CommandGlue for NoCommands {
    fn commands() -> Vec<wc::CommandSpec> {
        Vec::new()
    }

    fn expand(
        _: String,
        _: wc::Context,
        _: Vec<wt::ParamValue>,
        _: Vec<wt::ChoiceValue>,
        _: Vec<wc::CommandInput>,
        _: Vec<u64>,
    ) -> Result<wc::CommandGroup, wt::Diagnostic> {
        Err(nothing("plugin.no-command", "commands"))
    }
}

/// The glue of a plugin's [`Feature`]: each `evaluate` runs on a kernel of
/// its own, whose body table dies with the call.
pub struct WithFeatures<S>(PhantomData<S>);

impl<S: FeatureSource> FeatureGlue for WithFeatures<S> {
    fn describe() -> Vec<wf::FeatureTypeSpec> {
        S::feature().describe().to_wit()
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
        let out = S::feature().evaluate(
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

/// The glue of a plugin's [`Exporter`]: each export reads the host's
/// snapshot through [`WitSnapshot`].
pub struct WithExporters<S>(PhantomData<S>);

impl<S: ExporterSource> ExporterGlue for WithExporters<S> {
    fn exporters() -> Vec<wx::ExporterSpec> {
        S::exporter().exporters().to_wit()
    }

    fn run(
        exporter: String,
        subject: wx::Subject,
        params: Vec<wf::ParamValue>,
        choices: Vec<wf::ChoiceValue>,
    ) -> Result<Vec<wx::File>, wt::Diagnostic> {
        S::exporter()
            .export(
                &mut WitSnapshot,
                &exporter,
                &crate::Subject::from_wit(subject),
                &Vec::from_wit(params),
                &Vec::from_wit(choices),
            )
            .map(|files| files.to_wit())
            .map_err(|d| d.to_wit())
    }
}

/// The glue of a plugin's [`Commands`]: each expansion reads the host's
/// snapshot through [`WitSnapshot`].
pub struct WithCommands<S>(PhantomData<S>);

impl<S: CommandSource> CommandGlue for WithCommands<S> {
    fn commands() -> Vec<wc::CommandSpec> {
        S::commands().commands().to_wit()
    }

    fn expand(
        command: String,
        context: wc::Context,
        params: Vec<wt::ParamValue>,
        choices: Vec<wt::ChoiceValue>,
        inputs: Vec<wc::CommandInput>,
        ids: Vec<u64>,
    ) -> Result<wc::CommandGroup, wt::Diagnostic> {
        let ids: Vec<_> = ids.into_iter().map(arrix_core::Id).collect();
        S::commands()
            .expand(
                &mut WitSnapshot,
                &command,
                &crate::Context::from_wit(context),
                &Vec::from_wit(params),
                &Vec::from_wit(choices),
                &Vec::from_wit(inputs),
                &ids,
            )
            .map(|group| group.to_wit())
            .map_err(|d| d.to_wit())
    }
}

/// The world's `feature`, `exchange` and `commands` exports over a
/// [`Plugin`].
pub struct Exported<P>(PhantomData<P>);

impl<P: Plugin> wf::Guest for Exported<P> {
    fn describe() -> Vec<wf::FeatureTypeSpec> {
        P::Features::describe()
    }

    fn evaluate(
        type_id: String,
        params: Vec<wf::ParamValue>,
        choices: Vec<wf::ChoiceValue>,
        inputs: Vec<wf::ResolvedInput>,
    ) -> Result<wf::FeatureOutput, wt::Diagnostic> {
        P::Features::evaluate(type_id, params, choices, inputs)
    }
}

impl<P: Plugin> wx::Guest for Exported<P> {
    fn exporters() -> Vec<wx::ExporterSpec> {
        P::Exporters::exporters()
    }

    fn run(
        exporter: String,
        subject: wx::Subject,
        params: Vec<wf::ParamValue>,
        choices: Vec<wf::ChoiceValue>,
    ) -> Result<Vec<wx::File>, wt::Diagnostic> {
        P::Exporters::run(exporter, subject, params, choices)
    }
}

impl<P: Plugin> wc::Guest for Exported<P> {
    fn commands() -> Vec<wc::CommandSpec> {
        P::Commands::commands()
    }

    fn expand(
        command: String,
        context: wc::Context,
        params: Vec<wt::ParamValue>,
        choices: Vec<wt::ChoiceValue>,
        inputs: Vec<wc::CommandInput>,
        ids: Vec<u64>,
    ) -> Result<wc::CommandGroup, wt::Diagnostic> {
        P::Commands::expand(command, context, params, choices, inputs, ids)
    }
}

/// Exports a plugin as the world's `feature`, `exchange` and `commands`
/// interfaces, once per cdylib. `export_plugin!(Spur)` names a unit struct
/// implementing [`Feature`]; `export_plugin!(features = Spur, exporters =
/// Urdf, commands = Place)` names any of a [`Feature`], an [`Exporter`] and
/// [`Commands`], in that order. What it does not name is empty.
#[macro_export]
macro_rules! export_plugin {
    (@feature) => {
        type __ArrixFeatures = $crate::guest::NoFeatures;
    };
    (@feature $feature:path) => {
        struct __ArrixFeature;
        impl $crate::guest::FeatureSource for __ArrixFeature {
            fn feature() -> &'static dyn $crate::Feature {
                &$feature
            }
        }
        type __ArrixFeatures = $crate::guest::WithFeatures<__ArrixFeature>;
    };
    (@exporter) => {
        type __ArrixExporters = $crate::guest::NoExporters;
    };
    (@exporter $exporter:path) => {
        struct __ArrixExporter;
        impl $crate::guest::ExporterSource for __ArrixExporter {
            fn exporter() -> &'static dyn $crate::Exporter {
                &$exporter
            }
        }
        type __ArrixExporters = $crate::guest::WithExporters<__ArrixExporter>;
    };
    (@command) => {
        type __ArrixCommands = $crate::guest::NoCommands;
    };
    (@command $command:path) => {
        struct __ArrixCommand;
        impl $crate::guest::CommandSource for __ArrixCommand {
            fn commands() -> &'static dyn $crate::Commands {
                &$command
            }
        }
        type __ArrixCommands = $crate::guest::WithCommands<__ArrixCommand>;
    };
    (@go [$($f:tt)*] [$($e:tt)*] [$($c:tt)*]) => {
        $crate::export_plugin!(@feature $($f)*);
        $crate::export_plugin!(@exporter $($e)*);
        $crate::export_plugin!(@command $($c)*);
        struct __ArrixPlugin;
        impl $crate::guest::Plugin for __ArrixPlugin {
            type Features = __ArrixFeatures;
            type Exporters = __ArrixExporters;
            type Commands = __ArrixCommands;
        }
        type __ArrixExported = $crate::guest::Exported<__ArrixPlugin>;
        $crate::guest::bindings::__arrix_export_world!(
            __ArrixExported with_types_in $crate::guest::bindings
        );
    };
    (@go [$($f:tt)*] [$($e:tt)*] [$($c:tt)*] features = $v:path $(, $($rest:tt)*)?) => {
        $crate::export_plugin!(@go [$v] [$($e)*] [$($c)*] $($($rest)*)?);
    };
    (@go [$($f:tt)*] [$($e:tt)*] [$($c:tt)*] exporters = $v:path $(, $($rest:tt)*)?) => {
        $crate::export_plugin!(@go [$($f)*] [$v] [$($c)*] $($($rest)*)?);
    };
    (@go [$($f:tt)*] [$($e:tt)*] [$($c:tt)*] commands = $v:path $(, $($rest:tt)*)?) => {
        $crate::export_plugin!(@go [$($f)*] [$($e)*] [$v] $($($rest)*)?);
    };
    (features = $($rest:tt)*) => {
        $crate::export_plugin!(@go [] [] [] features = $($rest)*);
    };
    (exporters = $($rest:tt)*) => {
        $crate::export_plugin!(@go [] [] [] exporters = $($rest)*);
    };
    (commands = $($rest:tt)*) => {
        $crate::export_plugin!(@go [] [] [] commands = $($rest)*);
    };
    ($feature:path $(,)?) => {
        $crate::export_plugin!(features = $feature);
    };
}
