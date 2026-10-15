//! What the browser tests of this crate share: the example page built and
//! copied beside `tests/docs/bracket` as `.arrx` bytes (plans/c4 steps 7
//! and 8).
#![cfg(not(target_arch = "wasm32"))]

use std::fs;
use std::path::{Path, PathBuf};

use arrix_doc::{MemorySource, to_zip};
use arrix_web_test::{build_example, copy_files, target_dir, workspace_root};

/// The files of the document directory `dir`, keyed by their
/// `/`-separated path inside it.
pub fn source(dir: &Path) -> MemorySource {
    fn walk(dir: &Path, prefix: &str, out: &mut MemorySource) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            if path.is_dir() {
                walk(&path, &format!("{prefix}{name}/"), out);
            } else {
                out.0
                    .insert(format!("{prefix}{name}"), fs::read(&path).unwrap());
            }
        }
    }
    let mut out = MemorySource::default();
    walk(dir, "", &mut out);
    out
}

/// The page and the bracket, in a directory of `test`'s own: nextest runs
/// each test in a process of its own, and one must not rewrite the page
/// another is serving.
pub fn page(test: &str) -> PathBuf {
    let out = target_dir().join("web-test").join(test.replace(' ', "-"));
    build_example("arrix-plugin-host", "web_session", &out);
    copy_files(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/web_session"),
        &out,
    );
    let bracket = source(&workspace_root().join("tests/docs/bracket"));
    fs::write(out.join("bracket.arrx"), to_zip(&bracket).unwrap()).unwrap();
    out
}
