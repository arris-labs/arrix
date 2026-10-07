//! Documents on disk, on native: a directory (the unzipped form) or an
//! `.arrx`. The one place `arrix-app` reads documents from the filesystem;
//! the browser build hands bytes in through the same seam in C4.

use arrix_doc::{DocumentSource, MemorySource};

/// A document directory on disk (the unzipped form of an `.arrx`), as a
/// [`DocumentSource`].
pub struct DirSource {
    root: std::path::PathBuf,
}

impl DirSource {
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn path(&self, rel: &str) -> std::path::PathBuf {
        rel.split('/')
            .fold(self.root.clone(), |p, part| p.join(part))
    }
}

impl DocumentSource for DirSource {
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        use std::io::ErrorKind;
        match std::fs::read(self.path(path)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn list(&self, dir: &str) -> std::io::Result<Vec<String>> {
        use std::io::ErrorKind;
        let entries = match std::fs::read_dir(self.path(dir)) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(err),
        };
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        names.sort();
        Ok(names)
    }
}

/// What `path` names, opened for reading: a directory, or an `.arrx`.
pub enum PathSource {
    Dir(DirSource),
    Zip(MemorySource),
}

impl PathSource {
    pub fn read(path: &std::path::Path) -> Result<Self, String> {
        let name = path.to_string_lossy();
        if path.is_dir() {
            return Ok(Self::Dir(DirSource::new(path)));
        }
        let bytes = std::fs::read(path).map_err(|e| format!("{name}: {e}"))?;
        arrix_doc::from_zip(&bytes)
            .map(Self::Zip)
            .map_err(|e| format!("{name}: {e}"))
    }
}

impl DocumentSource for PathSource {
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        match self {
            Self::Dir(d) => d.read(path),
            Self::Zip(z) => z.read(path),
        }
    }

    fn list(&self, dir: &str) -> std::io::Result<Vec<String>> {
        match self {
            Self::Dir(d) => d.list(dir),
            Self::Zip(z) => z.list(dir),
        }
    }
}
