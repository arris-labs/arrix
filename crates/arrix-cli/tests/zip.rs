//! The `.arrx` zip of the scenario documents (docs/DATA-MODEL.md §File
//! format): deterministic, and its unzipped form is the directory form.

mod common;

use std::collections::BTreeMap;

use arrix_doc::{MemorySource, open, open_zip, save, save_zip};
use common::{read_tree, workspace};

fn directory(name: &str) -> MemorySource {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs").join(name), "", &mut files);
    MemorySource(files)
}

#[test]
fn the_saved_scenarios_zip_deterministically_and_unzips_to_its_directory() {
    for name in ["gear-on-plane", "slice"] {
        let dir = directory(name);
        let doc = open(&dir).unwrap();
        let zip = save_zip(&doc).unwrap();
        assert_eq!(zip, save_zip(&doc).unwrap(), "{name}: saved twice");
        assert_eq!(
            zip,
            save_zip(&open_zip(&zip).unwrap()).unwrap(),
            "{name}: reopened"
        );
        assert_eq!(
            arrix_doc::from_zip(&zip).unwrap().0,
            dir.0,
            "{name}: unzipped"
        );
        assert_eq!(
            save(&open_zip(&zip).unwrap()).0,
            dir.0,
            "{name}: unzip, open, save"
        );
    }
}
