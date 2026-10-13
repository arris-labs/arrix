//! The `PluginLoader` of a native session (ADR-0024): Tier 1 and Tier 2
//! plugins loaded from their directories by the tier their manifest names,
//! and loaded again on a reload so an edited source or a rebuilt component
//! is what runs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use arrix_core::{Diagnostic, PluginId};
use arrix_doc::{FeatureTypeId, PluginLoader, Registry};

use crate::manifest::Manifest;
use crate::tier0::HostError;
use crate::tier0::error;
use crate::tier1::register_tier1;
use crate::tier2::{MANIFEST, register_tier2};

/// Loads the plugin in `dir` at the tier its manifest names: a Tier 1
/// component or a Tier 2 process, registering every feature type it
/// describes, or none of them.
pub fn register_dir(registry: &mut Registry, dir: &Path) -> Result<Vec<FeatureTypeId>, HostError> {
    match manifest_in(dir)?.tier {
        1 => register_tier1(registry, dir),
        _ => register_tier2(registry, dir),
    }
}

/// The manifest of the plugin directory `dir`.
pub fn manifest_in(dir: &Path) -> Result<Manifest, HostError> {
    let text = std::fs::read_to_string(dir.join(MANIFEST)).map_err(|e| HostError::Process {
        plugin: dir.display().to_string(),
        message: format!("its {MANIFEST} could not be read: {e}"),
    })?;
    Ok(Manifest::parse(&text)?)
}

/// Remembers where each loaded plugin came from, so a reload can find it.
#[derive(Default)]
pub struct DirLoader {
    dirs: BTreeMap<PluginId, PathBuf>,
    /// The compiled-in plugins a directory of the same id takes the place
    /// of (a Tier 1 `gears` for the Tier 0 one).
    compiled: BTreeSet<PluginId>,
}

impl DirLoader {
    /// A loader that already knows `plugins`, registered before the session
    /// opened (`serve --plugin`).
    pub fn with_plugins(plugins: impl IntoIterator<Item = (PluginId, PathBuf)>) -> Self {
        Self {
            dirs: plugins.into_iter().collect(),
            compiled: BTreeSet::new(),
        }
    }

    /// Lets a loaded directory take the place of a compiled-in plugin of
    /// `ids`, as `--plugin` does.
    pub fn replacing(mut self, ids: impl IntoIterator<Item = PluginId>) -> Self {
        self.compiled.extend(ids);
        self
    }

    fn register(
        &mut self,
        registry: &mut Registry,
        dir: &Path,
    ) -> Result<Vec<FeatureTypeId>, Diagnostic> {
        let failed = |e: HostError| error("plugin.load", format!("{}: {e}", dir.display()));
        let manifest = manifest_in(dir).map_err(failed)?;
        if self.compiled.contains(&manifest.id) {
            registry.remove_plugin(&manifest.id);
        }
        let ids = register_dir(registry, dir).map_err(failed)?;
        if let Some(plugin) = ids.first().and_then(FeatureTypeId::plugin) {
            self.dirs.insert(plugin, dir.to_owned());
        }
        Ok(ids)
    }
}

impl PluginLoader for DirLoader {
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
