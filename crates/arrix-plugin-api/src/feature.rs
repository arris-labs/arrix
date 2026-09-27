//! Feature types a plugin contributes (the world's `feature`;
//! docs/PLUGINS.md §Contribution points, 1).

use arrix_core::{Diagnostic, Frame, Profile, Quantity, QuantityKind, SlotName};

use crate::{Body, Kernel};

/// A numeric field of a feature type's form.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamSpec {
    pub name: String,
    pub title: String,
    pub kind: QuantityKind,
    /// An expression, as the user would type it: `20`, `1 mm`.
    pub default: String,
}

/// What an input resolves to before `evaluate` sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputKind {
    /// A datum plane, a planar face or a world plane, as a `Frame`.
    Plane,
    /// A sketch region, referenced by its key (`Ref::Region`), as a
    /// `Profile` on the sketch's plane whose curves are keyed by the
    /// entities that bound it.
    Region,
    /// A body slot, referenced by the feature that made it (`Ref::Slot`),
    /// as a `Body` handle to the slot's version current at the feature
    /// (ADR-0007, ADR-0008).
    Body,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputSpec {
    pub name: String,
    pub title: String,
    pub kind: InputKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SlotKind {
    Body,
    Plane,
}

/// An output slot a feature type declares. A body slot that `modifies`
/// one of the type's body inputs, by the input's name, is that input's
/// slot's next version, not a slot of its own; a body input no slot
/// modifies is a tool, and its slot ends there (ADR-0007).
#[derive(Clone, Debug, PartialEq)]
pub struct SlotSpec {
    pub name: SlotName,
    pub kind: SlotKind,
    pub modifies: Option<String>,
}

/// A feature type: its form, inputs and output slots.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureTypeSpec {
    /// `<plugin-id>.<name>`.
    pub id: String,
    /// The type's own schema version (`type_version` in the document).
    pub version: u32,
    pub title: String,
    pub params: Vec<ParamSpec>,
    pub inputs: Vec<InputSpec>,
    pub outputs: Vec<SlotSpec>,
}

/// A parameter's expression, evaluated by the host to SI.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamValue {
    pub name: String,
    pub value: Quantity,
}

/// A resolved input. Not `Clone`, since a `Body` is not: a body input is a
/// handle the host gives the feature for its evaluation (ADR-0008), read
/// through `&[ResolvedInput]`.
#[derive(Debug, PartialEq)]
pub enum InputValue {
    Plane(Frame),
    Region(Profile),
    Body(Body),
}

#[derive(Debug, PartialEq)]
pub struct ResolvedInput {
    pub name: String,
    pub value: InputValue,
}

#[derive(Debug, PartialEq)]
pub enum OutputValue {
    Body(Body),
    Plane(Frame),
}

#[derive(Debug, PartialEq)]
pub struct SlotOutput {
    pub name: SlotName,
    pub value: OutputValue,
}

/// What `evaluate` produced, slot by slot.
#[derive(Debug, Default, PartialEq)]
pub struct FeatureOutput {
    pub slots: Vec<SlotOutput>,
}

/// A plugin's feature types. `evaluate` is deterministic: a pure function
/// of its arguments, the kernel's results and the plugin's version; no
/// clock, randomness, files or network (docs/PLUGINS.md §Features).
pub trait Feature: Send + Sync {
    fn describe(&self) -> Vec<FeatureTypeSpec>;

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        type_id: &str,
        params: &[ParamValue],
        inputs: &[ResolvedInput],
    ) -> Result<FeatureOutput, Diagnostic>;
}
