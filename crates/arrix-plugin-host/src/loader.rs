//! The `PluginLoader` of a native session (ADR-0024): Tier 2 plugins
//! started from their directories, and started again on a reload so an
//! edited source is what runs.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use arrix_core::{Diagnostic, PluginId};
use arrix_doc::{FeatureTypeId, PluginLoader, Registry};

use crate::tier0::error;
use crate::tier2::register_tier2;

/// Remembers where each loaded plugin came from, so a reload can find it.
#[derive(Default)]
pub struct Tier2Loader {
    dirs: BTreeMap<PluginId, PathBuf>,
}

impl Tier2Loader {
    /// A loader that already knows `plugins`, registered before the session
    /// opened (`serve --plugin`).
    pub fn with_plugins(plugins: impl IntoIterator<Item = (PluginId, PathBuf)>) -> Self {
        Self {
            dirs: plugins.into_iter().collect(),
        }
    }

    fn register(
        &mut self,
        registry: &mut Registry,
        dir: &Path,
    ) -> Result<Vec<FeatureTypeId>, Diagnostic> {
        let ids = register_tier2(registry, dir)
            .map_err(|e| error("plugin.load", format!("{}: {e}", dir.display())))?;
        if let Some(plugin) = ids.first().and_then(FeatureTypeId::plugin) {
            self.dirs.insert(plugin, dir.to_owned());
        }
        Ok(ids)
    }
}

impl PluginLoader for Tier2Loader {
    fn load(
        &mut self,
        registry: &mut Registry,
        dir: &Path,
    ) -> Result<Vec<FeatureTypeId>, Diagnostic> {
        self.register(registry, dir)
    }

    fn reload(
        &mut self,
        registry: &mut Registry,
        plugin: &PluginId,
    ) -> Result<Vec<FeatureTypeId>, Diagnostic> {
        let Some(dir) = self.dirs.get(plugin).cloned() else {
            return Err(error(
                "plugin.load",
                format!("plugin {plugin} is not loaded"),
            ));
        };
        registry.remove_plugin(plugin);
        self.register(registry, &dir)
    }
}
