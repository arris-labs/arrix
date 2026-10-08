//! What `eval` and `export` share: the plugins a build without some would
//! have, and a document path (a directory or an `.arrx`) as a source.

use std::path::Path;

use arrix_doc::{DocumentSource, MemorySource, Registry};

use crate::plugins;
use crate::source::DirSource;

/// The registry with each plugin of `without` left out; the message names
/// the first that this build does not have.
pub fn registry_without(without: &[String]) -> Result<Registry, String> {
    let mut registry = plugins::registry();
    for id in without {
        let removed = arrix_core::PluginId::new(id.as_str())
            .map(|plugin| registry.remove_plugin(&plugin))
            .unwrap_or(false);
        if !removed {
            return Err(format!(
                "--without-plugin {id}: no such plugin in this build"
            ));
        }
    }
    Ok(registry)
}

/// A document on disk: a directory or the files of an `.arrx`.
pub enum Loaded {
    Dir(DirSource),
    Zip(MemorySource),
}

impl Loaded {
    /// Opens `path`; the error says why it could not be read.
    pub fn open(path: &Path) -> Result<Self, String> {
        if path.is_file() {
            let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
            let files = arrix_doc::from_zip(&bytes).map_err(|e| e.to_string())?;
            Ok(Loaded::Zip(files))
        } else if path.is_dir() {
            Ok(Loaded::Dir(DirSource::new(path)))
        } else {
            Err("no such directory or file".into())
        }
    }
}

impl DocumentSource for Loaded {
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        match self {
            Loaded::Dir(source) => DocumentSource::read(source, path),
            Loaded::Zip(files) => files.read(path),
        }
    }

    fn list(&self, dir: &str) -> std::io::Result<Vec<String>> {
        match self {
            Loaded::Dir(source) => source.list(dir),
            Loaded::Zip(files) => files.list(dir),
        }
    }
}
