//! The plugin API: the WIT world, its Rust traits and plain types, versioned
//! apart from the app (docs/PLUGINS.md). A plugin depends on this crate alone.
//!
//! `wit/arrix-plugin.wit` is the authority. The traits here are what a Tier
//! 0 plugin implements directly; a test holds them and their types equal to
//! what `wit-bindgen` generates from the world. Where a type of the world is
//! one of `arrix-core`'s, it is re-exported rather than mirrored, so a plugin
//! names it through this crate and the host passes it without conversion.
//!
//! Built so far (0.3.0): the `types`, `kernel` (extrude, fuse, cut, names,
//! face frames, mass properties) and `feature` interfaces; 0.2 added region
//! references and region inputs (ADR-0006), 0.3 body inputs, slots that
//! modify them, and `fuse` and `cut` (ADR-0007).

mod feature;
mod kernel;

pub use arrix_core::geom::{FrameError, ProfileError};
pub use arrix_core::{
    CurveKey, DVec2, DVec3, Diagnostic, DiagnosticCode, FeatureId, Frame, Id, InvalidCode,
    InvalidPluginId, InvalidSlotName, NameError, ParamId, PersistentName, PluginId, Profile,
    ProfileLoop, ProfileSegment, Quantity, QuantityKind, RecordId, Ref, RegionKey, Severity,
    SketchEntityId, SlotName, TopoKind, Unit,
};
pub use feature::{
    Feature, FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, OutputValue,
    ParamSpec, ParamValue, ResolvedInput, SlotKind, SlotOutput, SlotSpec,
};
pub use kernel::{Body, Kernel, MassProperties};

/// This API's version: the crate's, and the WIT package's.
pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The WIT world's source, `package arrix:plugin@<API_VERSION>`.
pub const WIT: &str = include_str!("../wit/arrix-plugin.wit");
