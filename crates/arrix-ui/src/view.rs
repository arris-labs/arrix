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
    /// The document generation last drawn.
    pub generation: u64,
    pub diagnostics: usize,
    /// Display units, e.g. `mm` and `deg`: a document preference, never a
    /// scale on stored values (docs/DATA-MODEL.md §Parameters and expressions).
    pub length_unit: String,
    pub angle_unit: String,
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
}
