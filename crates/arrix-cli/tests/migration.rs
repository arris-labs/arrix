//! `tests/docs/migrations/schema-1/` is a schema-1 document, the
//! `gear-on-plane` scenario as it was saved before schema 2, and
//! `schema-2/` is `slice` as it was saved before schema 3, and `schema-3/` as it
//! was saved before schema 4. Opening any
//! runs the migration chain, and saving it gives exactly the current
//! scenario's bytes (docs/DATA-MODEL.md §File format).

mod common;

use std::collections::BTreeMap;

use arrix_doc::{MemorySource, open, save};
use common::{read_tree, workspace};

fn tree(dir: &str) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join(dir), "", &mut files);
    files
}

fn migrates_to(old_dir: &str, schema: u32, current_dir: &str) {
    let old = tree(old_dir);
    assert!(
        old["document.json"].ends_with(format!("\"schema\": {schema}\n}}\n").as_bytes()),
        "the fixture stays at schema {schema}"
    );
    let migrated = save(&open(&MemorySource(old)).unwrap()).0;
    assert_eq!(migrated, tree(current_dir));
}

#[test]
fn schema_1_opens_and_saves_as_the_current_scenario() {
    migrates_to(
        "tests/docs/migrations/schema-1",
        1,
        "tests/docs/gear-on-plane",
    );
}

#[test]
fn schema_2_opens_and_saves_as_the_current_slice() {
    migrates_to("tests/docs/migrations/schema-2", 2, "tests/docs/slice");
}

#[test]
fn schema_3_opens_and_saves_as_the_current_slice() {
    migrates_to("tests/docs/migrations/schema-3", 3, "tests/docs/slice");
}
