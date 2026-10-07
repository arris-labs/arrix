//! Everything the config directory says, loaded together: the one value
//! the app, `arrix describe` and `arrix config check` all read.

use arrix_plugin_api::FeatureTypeSpec;

use crate::{
    CommandRegistry, ConfigDiagnostic, ConfigSource, ExprCheck, Keymap, ParamDefaults, Settings,
    Theme,
};

/// The effective configuration and what its files got wrong. Each file
/// that is bad falls back to its defaults, key by key.
#[derive(Clone, Debug)]
pub struct Config {
    pub settings: Settings,
    pub keymap: Keymap,
    pub theme: Theme,
    pub defaults: ParamDefaults,
    /// In the order the files are read: settings, defaults, keymap, theme.
    pub diagnostics: Vec<ConfigDiagnostic>,
}

impl Config {
    /// The built-in layer, then the user's files from `source`. `types` are
    /// the registered feature types and `check` the expression checker.
    pub fn load(
        source: &impl ConfigSource,
        registry: &CommandRegistry,
        types: &[FeatureTypeSpec],
        check: ExprCheck<'_>,
    ) -> Self {
        let (settings, mut diagnostics) = Settings::load(source);
        let (defaults, found) = ParamDefaults::load(source, types, check);
        diagnostics.extend(found);
        let (keymap, found) = Keymap::load(source, registry);
        diagnostics.extend(found);
        let (theme, found) = Theme::load(settings.string("ui.theme"), source);
        diagnostics.extend(found);
        Self {
            settings,
            keymap,
            theme,
            defaults,
            diagnostics,
        }
    }
}
