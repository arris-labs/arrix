//! The user's configuration of the shell (ADR-0015): setting specs, the
//! layered model, and the files' diagnostics with their file and line.
//!
//! Config is client state. It never enters the document and never reaches
//! a feature's `evaluate` (`SEED.md` §6.1 rule 4, §6.3). This crate reads
//! no files: a [`ConfigSource`] hands it bytes, a directory on native
//! (`arrix-app`'s `config_files`), memory in tests and the browser.
//!
//! Built so far: the settings and `settings.toml`, and the command registry.

mod commands;
mod diagnostic;
mod settings;
mod source;
mod spanned;

pub use commands::{
    CONFIG_RELOAD, CORE_PART_TAB, CORE_SKETCH_TAB, CommandRegistry, CommandSpec, PALETTE_OPEN,
    PANEL_PROPERTIES_TOGGLE, PANEL_TREE_TOGGLE, RibbonPlacement, TabSpec,
};
pub use diagnostic::ConfigDiagnostic;
pub use settings::{DefaultValue, SETTINGS, SettingKind, SettingSpec, SettingValue, Settings};
pub use source::{ConfigSource, KEYBINDINGS_TOML, MemorySource, SETTINGS_TOML, THEMES_DIR};
