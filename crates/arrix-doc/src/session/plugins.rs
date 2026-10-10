//! Plugins loaded into a running session (ADR-0024). The authority owns
//! the registry: a load or reload changes a copy held here, and the
//! evaluator is handed the result ahead of the generation it made. The
//! loader is the host's (`arrix-plugin-host` for Tier 2); with none, a
//! session cannot load.

use std::collections::BTreeSet;
use std::path::Path;

use arrix_core::{Diagnostic, PluginId};
use arrix_plugin_api::FeatureTypeSpec;

use crate::authority::Rejected;
use crate::document::FeatureTypeId;
use crate::registry::Registry;

/// Loads plugins into a registry, on the session thread.
pub trait PluginLoader: Send {
    /// Registers the feature types of the plugin directory `dir`, and
    /// returns their ids; on an error the registry is as it was.
    fn load(
        &mut self,
        registry: &mut Registry,
        dir: &Path,
    ) -> Result<Vec<FeatureTypeId>, Diagnostic>;

    /// Restarts the loaded `plugin` and takes its feature types again,
    /// replacing the old ones, and returns their ids.
    fn reload(
        &mut self,
        registry: &mut Registry,
        plugin: &PluginId,
    ) -> Result<Vec<FeatureTypeId>, Diagnostic>;
}

/// What a load or reload changed.
pub(super) struct RegistryChange {
    /// The specs of the types loaded, new or replaced.
    pub added: Vec<FeatureTypeSpec>,
    /// The types the plugin no longer has.
    pub removed: Vec<FeatureTypeId>,
    /// The types whose cached results the evaluator drops.
    pub stale: BTreeSet<FeatureTypeId>,
}

/// The session's registry, its loader, and the types loaded since open.
pub(super) struct Plugins {
    registry: Registry,
    loader: Option<Box<dyn PluginLoader>>,
    loaded: BTreeSet<FeatureTypeId>,
}

impl Plugins {
    pub fn new(registry: Registry, loader: Option<Box<dyn PluginLoader>>) -> Self {
        Self {
            registry,
            loader,
            loaded: BTreeSet::new(),
        }
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// The specs of the types loaded since open that are still there.
    pub fn loaded_specs(&self) -> Vec<FeatureTypeSpec> {
        self.loaded
            .iter()
            .filter_map(|id| self.registry.get(id))
            .map(|ty| ty.spec().clone())
            .collect()
    }

    pub fn load(&mut self, dir: &str) -> Result<RegistryChange, Rejected> {
        self.change(|loader, registry| loader.load(registry, Path::new(dir)))
    }

    pub fn reload(&mut self, plugin: &PluginId) -> Result<RegistryChange, Rejected> {
        self.change(|loader, registry| loader.reload(registry, plugin))
    }

    /// Runs `f` on a copy of the registry, which replaces the session's
    /// only when it succeeds.
    fn change(
        &mut self,
        f: impl FnOnce(&mut dyn PluginLoader, &mut Registry) -> Result<Vec<FeatureTypeId>, Diagnostic>,
    ) -> Result<RegistryChange, Rejected> {
        let loader = self.loader.as_mut().ok_or(Rejected::NoPluginLoader)?;
        let mut next = self.registry.clone();
        let ids = f(loader.as_mut(), &mut next).map_err(Rejected::PluginLoad)?;
        let removed: Vec<FeatureTypeId> = self
            .registry
            .ids()
            .filter(|id| next.get(id).is_none())
            .cloned()
            .collect();
        let added = ids
            .iter()
            .filter_map(|id| next.get(id))
            .map(|ty| ty.spec().clone())
            .collect();
        let mut stale: BTreeSet<FeatureTypeId> = ids.iter().cloned().collect();
        stale.extend(removed.iter().cloned());
        for id in &removed {
            self.loaded.remove(id);
        }
        self.loaded.extend(ids);
        self.registry = next;
        Ok(RegistryChange {
            added,
            removed,
            stale,
        })
    }
}
