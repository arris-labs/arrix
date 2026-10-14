//! What `eval` and `export` share: the plugins a build without some would
//! have; a document path (a directory or an `.arrx`) as a source is the
//! kit's `Loaded`.

use std::path::PathBuf;

use arrix_doc::Registry;

use crate::plugins;

/// The registry with each Tier 2 plugin directory of `with` started and
/// each plugin of `without` left out (a `--plugin` one too); the message
/// names the first that could not be, or that this run does not have.
pub fn registry_for(with: &[PathBuf], without: &[String]) -> Result<Registry, String> {
    let mut registry = plugins::registry();
    plugins::register_dirs(&mut registry, with)?;
    for id in without {
        let removed = arrix_core::PluginId::new(id.as_str())
            .map(|plugin| registry.remove_plugin(&plugin))
            .unwrap_or(false);
        if !removed {
            return Err(format!("--without-plugin {id}: no such plugin in this run"));
        }
    }
    Ok(registry)
}

pub use arrix_plugin_test::Loaded;
