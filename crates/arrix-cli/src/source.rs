//! A document directory on disk, as a [`DocumentSource`]. The CLI is
//! native-only, so this is where the files are read.

use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use arrix_doc::DocumentSource;

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
