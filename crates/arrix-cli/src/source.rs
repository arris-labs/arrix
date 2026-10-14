//! The config directory on disk, as a [`arrix_config::ConfigSource`]: the
//! files are `settings.toml`, `keybindings.toml` and `themes/*.toml`, which a
//! missing one leaves unset.

use std::path::Path;

use arrix_doc::DocumentSource;
use arrix_plugin_test::DirSource;

pub struct ConfigDir(DirSource);

impl ConfigDir {
    pub fn new(root: &Path) -> Self {
        Self(DirSource::new(root))
    }
}

impl arrix_config::ConfigSource for ConfigDir {
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        DocumentSource::read(&self.0, path)
    }
}
