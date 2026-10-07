//! The user's configuration of the shell (ADR-0015): setting specs, the
//! layered model, and the files' diagnostics with their file and line.
//!
//! Config is client state. It never enters the document and never reaches
//! a feature's `evaluate` (`SEED.md` §6.1 rule 4, §6.3). This crate reads
//! no files: a [`ConfigSource`] hands it bytes, a directory on native
//! (`arrix-app`'s `config_files`), memory in tests and the browser.
//!
//! Built so far: the settings and `settings.toml`, the command registry,
//! the keymap, the themes and the defaults of feature parameters.

mod commands;
mod config;
mod defaults;
mod diagnostic;
mod dir;
mod keymap;
mod settings;
mod source;
mod spanned;
mod theme;

pub use commands::{
    CONFIG_RELOAD, CONSTRAIN_KINDS, CORE_PART_TAB, CORE_SKETCH_TAB, CommandRegistry, CommandSpec,
    DOCUMENT_CLOSE, DOCUMENT_NEW, DOCUMENT_NEXT, DOCUMENT_PREVIOUS, EDIT_REDO, EDIT_UNDO,
    FORM_CANCEL, FORM_OK, PALETTE_CLOSE, PALETTE_NEXT, PALETTE_OPEN, PALETTE_PREVIOUS, PALETTE_RUN,
    PANEL_PROPERTIES_TOGGLE, PANEL_TREE_TOGGLE, RibbonPlacement, SELECTION_CLEAR, SKETCH_ARC,
    SKETCH_CIRCLE, SKETCH_CONSTRAIN_PREFIX, SKETCH_CONSTRUCTION, SKETCH_CREATE, SKETCH_DELETE,
    SKETCH_DIMENSION, SKETCH_EDIT, SKETCH_EXTEND, SKETCH_FINISH, SKETCH_LINE, SKETCH_MIRROR,
    SKETCH_OFFSET, SKETCH_RECTANGLE, SKETCH_SELECT, SKETCH_TOOL_CANCEL, SKETCH_TRIM, TabSpec,
    VIEW_FRONT, VIEW_HOME, VIEW_ISO, VIEW_PROJECTION_TOGGLE, VIEW_RIGHT, VIEW_TOP,
};
pub use config::Config;
pub use defaults::{ExprCheck, ParamDefaults};
pub use diagnostic::ConfigDiagnostic;
pub use dir::{Os, config_dir};
pub use keymap::{Binding, CONTEXTS, Chord, Keymap};
pub use settings::{DefaultValue, SETTINGS, SettingKind, SettingSpec, SettingValue, Settings};
pub use source::{ConfigSource, KEYBINDINGS_TOML, MemorySource, SETTINGS_TOML, THEMES_DIR};
pub use theme::{BUILTIN_THEMES, Base, Color, THEME_ROLES, Theme};
