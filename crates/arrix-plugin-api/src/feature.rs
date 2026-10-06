//! Feature types a plugin contributes (the world's `feature`;
//! docs/PLUGINS.md §Contribution points, 1).

use arrix_core::{
    Axis, DVec3, Diagnostic, Frame, PersistentName, Profile, Quantity, QuantityKind, SlotName,
};

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
    /// An edge, referenced by its name (`Ref::Topo`), as a
    /// `PersistentName`. Which body it is on is the kernel call's to check
    /// (ADR-0012).
    Edge,
    /// A sketch point (`Ref::Sketch`) or a vertex (`Ref::Topo`), as a
    /// world `Point`.
    Point,
    /// A sketch line (`Ref::Sketch`) or a straight edge (`Ref::Topo`), as
    /// a world `Axis` with a unit direction.
    Axis,
    /// The tool a joining or cutting feature kept (`Ref::Feature`), as a
    /// `Tool` (ADR-0013).
    Tool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InputSpec {
    pub name: String,
    pub title: String,
    pub kind: InputKind,
    /// The input takes a list of references, and arrives as one
    /// `ResolvedInput` per element in the record's order, each with the
    /// input's name (ADR-0012).
    pub list: bool,
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

/// A point input, in the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub at: DVec3,
    /// The normal of the sketch's plane for a sketch point; a vertex has
    /// none.
    pub normal: Option<DVec3>,
}

/// How a feature applied its tool.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ToolOp {
    Fuse,
    Cut,
}

/// The tool a feature that joins or cuts applied, and the operation:
/// what a pattern or a mirror of that feature copies (ADR-0013). Not
/// `Clone`: its body is a handle.
#[derive(Debug, PartialEq)]
pub struct Tool {
    pub body: Body,
    pub op: ToolOp,
}

/// A resolved input. Not `Clone`, since a `Body` is not: a body input is a
/// handle the host gives the feature for its evaluation (ADR-0008), read
/// through `&[ResolvedInput]`.
#[derive(Debug, PartialEq)]
pub enum InputValue {
    Plane(Frame),
    Region(Profile),
    Body(Body),
    Edge(PersistentName),
    Point(Point),
    Axis(Axis),
    Tool(Tool),
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

/// What `evaluate` produced, slot by slot, and the tool a joining or
/// cutting feature keeps for a pattern or a mirror to copy (ADR-0013).
#[derive(Debug, Default, PartialEq)]
pub struct FeatureOutput {
    pub slots: Vec<SlotOutput>,
    pub tool: Option<Tool>,
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
