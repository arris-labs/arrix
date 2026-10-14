//! Commands a plugin contributes (the world's `commands`; ADR-0038;
//! docs/PLUGINS.md §Contribution points, 3).
//!
//! A command is declared like a feature's form. Its expansion is a pure
//! function of its arguments and the snapshot, and yields one
//! [`CommandGroup`]: plain mirrors of the core commands a plugin may issue,
//! which the host checks, translates into one `Command::Group` and submits
//! like any other. Nothing here applies anything.

use arrix_core::{AssemblyId, ComponentId, DVec3, Diagnostic, FeatureId, Id, JointId, MaterialId};
use arrix_core::{PartId, PersistentName, Quantity, Ref};
use serde::{Deserialize, Serialize};

use crate::{
    BodyRef, ChoiceSpec, ChoiceValue, ComponentOf, InputSpec, JointKind, ParamSpec, ParamValue,
    Rgb, Snapshot,
};

/// A command: its form (a feature's fields) and where it sits.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CommandSpec {
    /// `<plugin-id>.<name>`.
    pub id: String,
    pub title: String,
    /// The ribbon group it sits in, on the plugin's tab.
    pub group: String,
    pub params: Vec<ParamSpec>,
    pub choices: Vec<ChoiceSpec>,
    /// At 0.8 every input's kind is [`InputKind::Edge`](crate::InputKind).
    pub inputs: Vec<InputSpec>,
    /// The most ids one expansion uses; 64 when none.
    pub max_new: Option<u32>,
}

/// What the client's tab looks at, and the frame inputs arrive in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Part(PartId),
    Assembly(AssemblyId),
}

/// A circular edge, in the context's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CircleEdge {
    pub centre: DVec3,
    /// The circle's normal as the kernel orients it: not a stated property;
    /// the side is read from [`ResolvedEdge::face_normals`].
    pub axis: DVec3,
    pub radius: f64,
}

/// A straight edge, in the context's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineEdge {
    pub start: DVec3,
    pub end: DVec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EdgeCurve {
    Circle(CircleEdge),
    Line(LineEdge),
    /// Any other curve: no geometry is resolved.
    Other,
}

/// An edge input, resolved against the base generation.
#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedEdge {
    /// The component ids from the assembly down; empty in a part.
    pub path: Vec<ComponentId>,
    pub name: PersistentName,
    pub curve: EdgeCurve,
    /// The outward normals of the planar faces the edge bounds, in the
    /// order of their names.
    pub face_normals: Vec<DVec3>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CommandInputValue {
    Edge(ResolvedEdge),
}

/// A resolved input of a command. A `list` input arrives as one per
/// element, in the order picked, each with the input's name.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandInput {
    pub name: String,
    pub value: CommandInputValue,
}

/// A value in a record: SI, or an expression's text the host parses.
#[derive(Clone, Debug, PartialEq)]
pub enum Expression {
    Si(Quantity),
    Text(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct NamedExpression {
    pub name: String,
    pub expression: Expression,
}

/// An input of a drafted feature. `many` marks a list input.
#[derive(Clone, Debug, PartialEq)]
pub struct NamedInput {
    pub name: String,
    pub refs: Vec<Ref>,
    pub many: bool,
}

/// A feature as a plugin drafts it; the host fills the type's version.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureDraft {
    pub id: FeatureId,
    pub type_id: String,
    pub name: String,
    pub params: Vec<NamedExpression>,
    pub choices: Vec<ChoiceValue>,
    pub inputs: Vec<NamedInput>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PartDraft {
    pub id: PartId,
    pub name: String,
    pub features: Vec<FeatureDraft>,
    pub material: Option<MaterialId>,
}

/// A feature added to a part's history: at the end when `at` is `None`.
#[derive(Clone, Debug, PartialEq)]
pub struct AddFeature {
    pub part: PartId,
    pub at: Option<u32>,
    pub feature: FeatureDraft,
}

/// A position (three lengths) and URDF's fixed-axis roll, pitch and yaw
/// (three angles).
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub position: [Expression; 3],
    pub rpy: [Expression; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentDraft {
    pub id: ComponentId,
    pub name: String,
    pub of: ComponentOf,
    pub pose: Option<Pose>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ComponentChange {
    pub assembly: AssemblyId,
    pub component: ComponentDraft,
}

/// Where a joint attaches: `axis` and `origin` name a face or an edge of
/// the part the path ends at (ADR-0029).
#[derive(Clone, Debug, PartialEq)]
pub struct Anchor {
    pub path: Vec<ComponentId>,
    pub axis: PersistentName,
    pub origin: Option<PersistentName>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JointDraft {
    pub id: JointId,
    pub name: String,
    pub kind: JointKind,
    pub parent: Anchor,
    pub child: Anchor,
    pub value: Expression,
    pub offset: Expression,
    pub limits: Option<[Expression; 2]>,
    pub flip: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct JointChange {
    pub assembly: AssemblyId,
    pub joint: JointDraft,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaterialDraft {
    pub id: MaterialId,
    pub name: String,
    pub density: Option<Expression>,
    pub color: Rgb,
}

/// Sets what a part, or one of its final bodies, is made of; `None`
/// clears it.
#[derive(Clone, Debug, PartialEq)]
pub struct AssignMaterial {
    pub part: PartId,
    pub body: Option<BodyRef>,
    pub material: Option<MaterialId>,
}

/// The core commands a plugin may issue. Deletes and parameter edits wait
/// for a plugin that needs them (ADR-0038 §5).
#[derive(Clone, Debug, PartialEq)]
pub enum PluginCommand {
    AddPart(PartDraft),
    AddFeature(AddFeature),
    AddComponent(ComponentChange),
    EditComponent(ComponentChange),
    AddJoint(JointChange),
    EditJoint(JointChange),
    AddMaterial(MaterialDraft),
    AssignMaterial(AssignMaterial),
}

/// One expansion's result: one undo entry under `label`.
#[derive(Clone, Debug, PartialEq)]
pub struct CommandGroup {
    pub label: String,
    pub commands: Vec<PluginCommand>,
}

/// A plugin's commands. `expand` is deterministic: a pure function of its
/// arguments and the snapshot, with no clock, randomness, files or network.
/// It takes `ids` in order; a record naming another id is refused by the
/// host (`command.invalid`).
pub trait Commands: Send + Sync {
    fn commands(&self) -> Vec<CommandSpec>;

    #[allow(clippy::too_many_arguments)]
    fn expand(
        &self,
        snapshot: &mut dyn Snapshot,
        command: &str,
        context: &Context,
        params: &[ParamValue],
        choices: &[ChoiceValue],
        inputs: &[CommandInput],
        ids: &[Id],
    ) -> Result<CommandGroup, Diagnostic>;
}
