//! View types: plain, serialisable data the shell is drawn from, and the
//! intents it returns (docs/UI-RENDERING.md §View types). `arrix-app` builds
//! them from a document snapshot, the latest events and session state; this
//! crate never sees where they came from.

use serde::{Deserialize, Serialize};

use crate::ThemeView;

/// Everything the shell draws in one frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShellView {
    pub theme: ThemeView,
    pub ribbon: RibbonView,
    pub tree: TreeView,
    /// The open documents, as the tab strip.
    pub documents: DocumentTabsView,
    /// Which cards float over the viewport this frame.
    pub cards: CardsView,
    pub status: StatusView,
    /// The property card's form, when one is open.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub form: Option<crate::FormView>,
    /// The command palette, when it is open.
    pub palette: Option<PaletteView>,
}

/// The ribbon: its tabs and which one is open. A plugin's tab is one more
/// entry here, never a mode (docs/UI-RENDERING.md §The shell).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RibbonView {
    pub tabs: Vec<RibbonTab>,
    /// The id of the open tab.
    pub active: String,
    /// The open tab's groups, left to right.
    pub groups: Vec<RibbonGroup>,
}

/// A titled run of buttons in the open tab.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RibbonGroup {
    pub title: String,
    pub commands: Vec<RibbonCommand>,
}

/// A ribbon button: a command and whether it applies right now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RibbonCommand {
    pub id: String,
    pub title: String,
    pub enabled: bool,
    /// The command is a tool, and the one in use: drawn pressed.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RibbonTab {
    pub id: String,
    pub title: String,
}

/// The feature tree: one entry per assembly and per part, in document
/// order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TreeView {
    /// Each assembly with its components, nested, and its joints.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assemblies: Vec<TreeAssembly>,
    pub parts: Vec<TreePart>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreePart {
    pub id: String,
    pub name: String,
    /// The tab looks at this part. Set only in a document that has an
    /// assembly, where there is a choice.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub current: bool,
    /// The part's history, in order.
    pub features: Vec<TreeFeature>,
}

/// An assembly: what places what, and how each joint stands (ADR-0030).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreeAssembly {
    pub id: String,
    pub name: String,
    /// The tab looks at this assembly.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub current: bool,
    pub components: Vec<TreeComponent>,
    pub joints: Vec<TreeJoint>,
}

/// One component of an assembly, with those of the assembly it places.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreeComponent {
    /// The component ids from the assembly the row is listed under.
    pub path: Vec<String>,
    pub name: String,
    /// The name of the part or assembly it places.
    pub of: String,
    pub placement: Placement,
    /// The joint that places it, for `placement` `by-joint`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub joint: Option<String>,
    /// The tree's selection, session state.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub selected: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<TreeComponent>,
}

/// How a component comes to be where it is (ADR-0029 §4), in its own
/// assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Placement {
    /// By a pose of its own.
    Grounded,
    ByJoint,
    /// Neither: it sits at the assembly's origin.
    Unplaced,
    /// Its joint closes a loop and is ignored.
    Loop,
    /// No outcome heard yet.
    Pending,
}

impl Placement {
    /// The short word the tree shows.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Grounded => "grounded",
            Self::ByJoint => "joint",
            Self::Unplaced => "unplaced",
            Self::Loop => "loop",
            Self::Pending => "…",
        }
    }
}

/// One joint of an assembly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreeJoint {
    pub id: String,
    pub name: String,
    /// `fixed`, `revolute` or `prismatic`.
    pub kind: String,
    pub state: JointState,
    /// What is worth saying beside the state: why it is lost or ignored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Where a joint stands, as its last solve ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum JointState {
    Ok,
    /// Its value is outside its limits, and applied as given.
    Limit,
    /// An anchor names nothing any more (`ref.lost`).
    Lost,
    /// It closes a loop and is ignored.
    Loop,
    Failed,
    /// No outcome heard yet.
    Evaluating,
}

impl JointState {
    /// The short word the tree shows; none for a joint that is fine.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Self::Ok => None,
            Self::Limit => Some("over limit"),
            Self::Lost => Some("lost"),
            Self::Loop => Some("loop"),
            Self::Failed => Some("failed"),
            Self::Evaluating => Some("…"),
        }
    }
}

/// One feature of a part, as the tree shows it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreeFeature {
    pub id: String,
    pub name: String,
    /// The feature type's id, for the tooltip.
    pub kind: String,
    pub state: FeatureState,
    /// What is worth saying beside the state: why it failed, or is frozen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// The tree's selection, session state.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub selected: bool,
}

/// Where a feature stands, the first of these that applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeatureState {
    /// At or after the rollback bar: not evaluated.
    RolledBack,
    Suppressed,
    /// No outcome heard yet.
    Evaluating,
    Ok,
    Failed,
    /// Shown from its stored result, its type or plugin being unavailable.
    Frozen,
    /// Frozen, and its inputs now differ from the stored result's.
    FrozenStale,
}

impl FeatureState {
    /// The short word the tree shows; none for a feature that is fine.
    pub fn tag(self) -> Option<&'static str> {
        match self {
            Self::Ok => None,
            Self::RolledBack => Some("rolled back"),
            Self::Suppressed => Some("suppressed"),
            Self::Evaluating => Some("…"),
            Self::Failed => Some("failed"),
            Self::Frozen => Some("frozen"),
            Self::FrozenStale => Some("frozen, stale"),
        }
    }
}

/// The tab strip of open documents, in the order they were opened.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DocumentTabsView {
    pub tabs: Vec<DocumentTab>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DocumentTab {
    pub title: String,
    /// Where the document was opened from; the tab's tooltip.
    pub path: String,
    pub active: bool,
}

/// The cards that float over the viewport. A card is drawn only when it
/// shows; which ones do is the app's decision (`ui.panel.*` and the toggles,
/// ADR-0017).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardsView {
    pub tree: bool,
    pub properties: bool,
}

/// The command palette: the query, what it found, and which result Enter runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaletteView {
    pub query: String,
    pub results: Vec<PaletteEntry>,
    /// An index into `results`; meaningless while there are none.
    pub selected: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaletteEntry {
    pub id: String,
    pub title: String,
    /// The command's effective keys, as the user writes them.
    pub keys: Vec<String>,
    pub enabled: bool,
}

/// The status bar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StatusView {
    /// Help text for whatever is under the pointer, else what to do next.
    pub hint: String,
    /// The document generation last drawn.
    pub generation: u64,
    pub diagnostics: usize,
    /// Display units, e.g. `mm` and `deg`: a document preference, never a
    /// scale on stored values (docs/DATA-MODEL.md §Parameters and expressions).
    pub length_unit: String,
    pub angle_unit: String,
    /// The degrees of freedom and solver state of the sketch open in sketch
    /// mode.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sketch: Option<crate::SketchStatusView>,
}

/// The pointer in the open sketch's plane, as the app's viewport measured
/// it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlanePointer {
    /// Plane-local metres.
    pub at: [f64; 2],
    /// The snap radius at this zoom: the screen's hit tolerance in metres.
    pub tol: f64,
    /// The modifier that suppresses inference (Alt) is held.
    pub suppress: bool,
    /// egui's clock, in seconds: what a dwell is timed by.
    pub time: f64,
}

/// What the user did, in view terms. `arrix-app` turns an intent into a
/// command or into a change of session state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum UiIntent {
    /// A ribbon tab was clicked: session state, no command.
    RibbonTabSelected { tab: String },
    /// A command was invoked, by whatever means (a ribbon button now).
    CommandInvoked { command: String },
    /// The palette's query text was edited: session state, no command.
    PaletteQuery { text: String },
    /// The palette was dismissed by a click outside it.
    PaletteClosed,
    /// A document's tab was clicked: session state, no command.
    DocumentTabSelected { index: usize },
    /// A document's tab was closed.
    DocumentTabClosed { index: usize },
    /// A click in the viewport picked this face, edge or vertex, or
    /// nothing (empty space): the selection becomes it. Session state, no
    /// command; a click on a card over the viewport picks nothing and says
    /// nothing.
    Picked {
        name: Option<arrix_core::PersistentName>,
        /// The component ids from the assembly the entity is of; empty in
        /// a part's context.
        #[serde(default)]
        path: Vec<String>,
    },
    /// A part's name in the tree was clicked: the tab looks at it.
    TreePartActivated { part: String },
    /// A part's name in the tree was double-clicked: its material form
    /// opens in the property card. Session state, no command.
    TreePartEdited { part: String },
    /// An assembly's name in the tree was clicked: the tab looks at it.
    TreeAssemblyActivated { assembly: String },
    /// A component's row in the tree was clicked: it becomes the tree's
    /// selection, and its instances are marked in the viewport. `path` is
    /// the component ids from the assembly. Session state, no command.
    TreeComponentSelected { assembly: String, path: Vec<String> },
    /// A component's row in the tree was double-clicked: its form opens in
    /// the property card. Session state, no command.
    TreeComponentActivated { assembly: String, path: Vec<String> },
    /// A joint's row in the tree was double-clicked.
    TreeJointActivated { assembly: String, joint: String },
    /// A feature's row in the tree was clicked: it becomes the tree's
    /// selection. Session state, no command.
    TreeFeatureSelected { feature: String },
    /// A feature's row in the tree was double-clicked: a sketch opens in
    /// sketch mode, a datum plane gets a sketch.
    TreeFeatureActivated { feature: String },
    /// The pointer is over this item of the open sketch, or over none: the
    /// sketch's hover becomes it. Session state, no command.
    SketchHovered { hit: Option<crate::SketchHit> },
    /// A click in the open sketch, on an item or on none, and where in the
    /// plane. With the select tool it selects: `extend` is shift, and the
    /// item joins or leaves the selection instead of replacing it; session
    /// state, no command. With a drawing tool it places a point, and a
    /// click that adds geometry is one command.
    SketchClicked {
        hit: Option<crate::SketchHit>,
        extend: bool,
        pointer: Option<PlanePointer>,
    },
    /// Where the pointer is in the open sketch's plane, every frame it is
    /// over the viewport, or `None` once it has left: a drawing tool's
    /// snap and preview follow it. Session state, no command.
    SketchPointerMoved { pointer: Option<PlanePointer> },
    /// The left button went down on `hit` and moved: a drag of the point or
    /// curve under it starts, from where the button went down. Session
    /// state, no command.
    SketchDragStarted {
        hit: Option<crate::SketchHit>,
        pointer: Option<PlanePointer>,
    },
    /// The drag's pointer, every frame of it: the solver follows it within
    /// the frame. Session state, no command.
    SketchDragMoved { pointer: PlanePointer },
    /// The button went up: the drag is one command, or none when nothing
    /// moved.
    SketchDragEnded,
    /// A double-click in the open sketch, on an item or on none: on a
    /// dimension's label it opens the field its value is typed in. Session
    /// state, no command.
    SketchDoubleClicked { hit: Option<crate::SketchHit> },
    /// A dimension's field was committed, by Enter or by losing focus: the
    /// text is an expression for the dimension's value. One command, or
    /// none when the text is the value it has or does not make one.
    DimensionCommitted { constraint: String, text: String },
    /// A dimension's field was dismissed with Escape. Session state.
    DimensionCancelled,
    /// The open field's text changed, uncommitted.
    DimensionEdited { constraint: String, text: String },
    /// A form field's text changed, uncommitted. Session state: a create
    /// form's draft follows it, an edit form's document does not.
    FieldEdited { field: String, text: String },
    /// A form field's text was committed, by Enter or by leaving the box:
    /// an expression for the field's value. A create form's draft takes it;
    /// an edit form's is one command, or none when the text is the value it
    /// has or the form's rules refuse it.
    FieldCommitted { field: String, text: String },
    /// A form's name field was committed.
    NameCommitted { text: String },
    /// A choice was picked in a form, or cleared (`None`) where it may be
    /// left unset.
    ChoiceSelected {
        field: String,
        value: Option<String>,
    },
    /// A reference field of a form was clicked: it becomes the field a pick
    /// fills. Session state, no command.
    ReferenceFieldActivated { field: String },
    /// The value at `index` of a reference field was removed.
    ReferenceRemoved { field: String, index: usize },
    /// The candidate at `index` (best first) of a field holding a lost
    /// reference was taken: one command re-binds it.
    CandidatePicked { field: String, index: usize },
    /// A create form's OK: one command that adds the feature.
    FormOk,
    /// A create form's Cancel, or Escape: the draft goes, no command.
    FormCancelled,
    /// The pointer is over this face, edge or vertex, or over nothing: the
    /// hover becomes it. Session state, no command.
    Hovered {
        name: Option<arrix_core::PersistentName>,
        /// As for `Picked`.
        #[serde(default)]
        path: Vec<String>,
    },
}
