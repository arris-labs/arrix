//! The config directory on native: where it is, its files as a
//! [`ConfigSource`], and a fingerprint of their modification times for the
//! app's poll (ADR-0015). The one place `arrix-app` reads config from disk;
//! the browser build gets a store behind the same seam in C4.

use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use arrix_config::{ConfigSource, Os, THEMES_DIR, config_dir};

/// A directory of config files. It need not exist: its files are then all
/// unset, which is the built-in configuration.
pub struct ConfigDir {
    root: PathBuf,
}

impl ConfigDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// `$ARRIX_CONFIG_DIR`, else the platform's directory for ArriX.
    pub fn from_environment() -> Option<Self> {
        config_dir(Os::current(), |name| std::env::var(name).ok()).map(Self::new)
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    fn file(&self, rel: &str) -> PathBuf {
        rel.split('/')
            .fold(self.root.clone(), |p, part| p.join(part))
    }
}

impl ConfigSource for ConfigDir {
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.file(path)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// The names, sizes and modification times of every file the loader
    /// can read: the two files and the themes directory's.
    fn fingerprint(&self) -> Option<u64> {
        let mut files = vec![
            self.file(arrix_config::SETTINGS_TOML),
            self.file(arrix_config::KEYBINDINGS_TOML),
        ];
        if let Ok(entries) = std::fs::read_dir(self.file(THEMES_DIR)) {
            files.extend(entries.flatten().map(|e| e.path()));
        }
        files.sort();
        let mut hasher = std::hash::DefaultHasher::new();
        for file in files {
            file.hash(&mut hasher);
            match std::fs::metadata(&file) {
                Ok(meta) => {
                    meta.len().hash(&mut hasher);
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok());
                    modified.map(|d| d.as_nanos()).hash(&mut hasher);
                }
                Err(_) => 0u8.hash(&mut hasher),
            }
        }
        Some(hasher.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_directory_is_all_unset_and_a_change_moves_the_fingerprint() {
        let dir = std::env::temp_dir().join(format!("arrix-config-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let files = ConfigDir::new(&dir);
        assert!(matches!(files.read("settings.toml"), Ok(None)));
        let before = files.fingerprint();
        std::fs::create_dir_all(dir.join("themes")).unwrap();
        std::fs::write(dir.join("settings.toml"), "[ui]\ntheme = \"light\"\n").unwrap();
        assert!(files.read("settings.toml").unwrap().is_some());
        assert_ne!(files.fingerprint(), before);
        let with_settings = files.fingerprint();
        std::fs::write(dir.join("themes/a.toml"), "base = \"dark\"\n").unwrap();
        assert_ne!(files.fingerprint(), with_settings, "a theme file counts");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
