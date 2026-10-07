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

/// The feature tree: one entry per part, in document order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TreeView {
    pub parts: Vec<TreePart>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TreePart {
    pub id: String,
    pub name: String,
    /// The part's history, in order.
    pub features: Vec<TreeFeature>,
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
    },
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
    /// The pointer is over this face, edge or vertex, or over nothing: the
    /// hover becomes it. Session state, no command.
    Hovered {
        name: Option<arrix_core::PersistentName>,
    },
}
