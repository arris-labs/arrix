//! The plugin API: the WIT world, its Rust traits and plain types, versioned
//! apart from the app (docs/PLUGINS.md). A plugin depends on this crate alone.
//!
//! `wit/arrix-plugin.wit` is the authority. The traits here are what a Tier
//! 0 plugin implements directly; a test holds them and their types equal to
//! what `wit-bindgen` generates from the world. Where a type of the world is
//! one of `arrix-core`'s, it is re-exported rather than mirrored, so a plugin
//! names it through this crate and the host passes it without conversion.
//!
//! `wire/` holds the JSON Schema of every type and function of the world and
//! the method table, generated from the WIT (ADR-0022) and held fresh by
//! `tests/wire.rs`.
//!
//! Built so far (0.6.0): the `types`, `kernel` (extrude, revolve, fuse,
//! cut, common, fillet, chamfer, copy, mirror, names, face frames, mass
//! properties) and `feature` interfaces; 0.2 added region references and
//! region inputs (ADR-0006), 0.3 body inputs, slots that modify them, and
//! `fuse` and `cut` (ADR-0007), 0.4 list inputs and the `edge`, `point`,
//! `axis` and `tool` kinds (ADR-0012), the tool a feature keeps
//! (ADR-0013), and the part-feature operations of the kernel, 0.5 choices
//! a feature type declares, fields shown only under a choice, and the
//! choices an `evaluate` receives (ADR-0018).
//!
//! `math` is a plugin's transcendental math, the same bits at every tier
//! (ADR-0036).

mod feature;
#[cfg(feature = "guest")]
pub mod guest;
mod kernel;
pub mod math;

pub use arrix_core::geom::{FrameError, ProfileError};
pub use arrix_core::{
    Axis, CurveKey, DVec2, DVec3, Diagnostic, DiagnosticCode, FeatureId, Frame, Id, InvalidCode,
    InvalidPluginId, InvalidSlotName, Motion, NameError, ParamId, PersistentName, PluginId,
    Profile, ProfileLoop, ProfileSegment, Quantity, QuantityKind, RecordId, Ref, RegionKey,
    Severity, SketchEntityId, SlotName, TopoKind, Unit,
};
pub use feature::{
    ChoiceOption, ChoiceSpec, ChoiceValue, Feature, FeatureOutput, FeatureTypeSpec, InputKind,
    InputSpec, InputValue, OutputValue, ParamSpec, ParamValue, Point, ResolvedInput, ShownWhen,
    SlotKind, SlotOutput, SlotSpec, Tool, ToolOp,
};
pub use kernel::{Body, Kernel, MassProperties};

/// This API's version: the crate's, and the WIT package's.
pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The WIT world's source, `package arrix:plugin@<API_VERSION>`.
pub const WIT: &str = include_str!("../wit/arrix-plugin.wit");
