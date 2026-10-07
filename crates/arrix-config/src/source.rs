//! Where the config files come from: the user's directory on native, memory
//! in tests and, in C4, a browser store.

use std::collections::BTreeMap;

/// The settings file.
pub const SETTINGS_TOML: &str = "settings.toml";

/// The keymap file.
pub const KEYBINDINGS_TOML: &str = "keybindings.toml";

/// The user's themes, one `<name>.toml` each.
pub const THEMES_DIR: &str = "themes";

/// The config directory's files. Paths are relative and `/`-separated.
pub trait ConfigSource {
    /// The bytes of `path`, or `None` when there is no such file.
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>>;
}

/// Config files in memory, keyed by path.
#[derive(Clone, Debug, Default)]
pub struct MemorySource(pub BTreeMap<String, Vec<u8>>);

impl MemorySource {
    /// A source holding `files`, as `(path, text)`.
    pub fn with<'a>(files: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        Self(
            files
                .into_iter()
                .map(|(path, text)| (path.to_owned(), text.as_bytes().to_vec()))
                .collect(),
        )
    }
}

impl ConfigSource for MemorySource {
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        Ok(self.0.get(path).cloned())
    }
}
