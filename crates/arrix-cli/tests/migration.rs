//! `tests/docs/migrations/schema-1/` is a schema-1 document, the
//! `gear-on-plane` scenario as it was saved before schema 2. Opening it
//! runs the migration chain, and saving it gives exactly the schema-2
//! bytes of `tests/docs/gear-on-plane/` (docs/DATA-MODEL.md §File format).

mod common;

use std::collections::BTreeMap;

use arrix_doc::{MemorySource, open, save};
use common::{read_tree, workspace};

fn tree(dir: &str) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join(dir), "", &mut files);
    files
}

#[test]
fn schema_1_opens_and_saves_as_the_schema_2_scenario() {
    let old = tree("tests/docs/migrations/schema-1");
    assert!(
        old["document.json"].ends_with(b"\"schema\": 1\n}\n"),
        "the fixture stays at schema 1"
    );
    let migrated = save(&open(&MemorySource(old)).unwrap()).0;
    assert_eq!(migrated, tree("tests/docs/gear-on-plane"));
}
