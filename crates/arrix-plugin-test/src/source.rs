//! A document on disk, as a [`DocumentSource`]: a directory or the files of
//! an `.arrx`. The kit and the CLI are native-only, so this is where the
//! files are read.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use arrix_doc::{DocumentSource, MemorySource};

pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
        }
    }

    fn path(&self, rel: &str) -> PathBuf {
        rel.split('/')
            .fold(self.root.clone(), |p, part| p.join(part))
    }
}

impl DocumentSource for DirSource {
    fn read(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        match std::fs::read(self.path(path)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(err) if err.kind() == ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn list(&self, dir: &str) -> std::io::Result<Vec<String>> {
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
