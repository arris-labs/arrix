//! The migration chain (docs/DATA-MODEL.md §File format, Versioning). A
//! migration is a pure function over the document's JSON tree, from one
//! schema to the next; `open` runs them in order.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::open::DOCUMENT_JSON;

/// The document's JSON files parsed, by path: `document.json`,
/// `params.json` when present, and `parts/<id>.json`.
pub(crate) type Tree = BTreeMap<String, Value>;

/// One link: schema `n` to `n + 1`, where `n` is its index plus one. It
/// leaves the header's `schema` to [`run`].
pub(crate) type Migration = fn(&mut Tree) -> Result<(), String>;

/// The chain this build ships: entry `i` reads schema `i + 1`.
pub(crate) const CHAIN: &[Migration] = &[schema_1_to_2];

/// Schema 2 added `FeatureRecord.frozen` and `blobs/`, both optional, so a
/// schema-1 tree is a schema-2 tree bar its header.
fn schema_1_to_2(_: &mut Tree) -> Result<(), String> {
    Ok(())
}

/// A migration that failed: which link, and why.
#[derive(Debug)]
pub(crate) struct Failed {
    pub from: u64,
    pub message: String,
}

/// Brings `tree` from schema `from` up through `chain`, setting the
/// header's `schema` after each link.
pub(crate) fn run(tree: &mut Tree, from: u64, chain: &[Migration]) -> Result<(), Failed> {
    for (at, migrate) in chain.iter().enumerate().skip(from as usize - 1) {
        let from = at as u64 + 1;
        migrate(tree).map_err(|message| Failed { from, message })?;
        if let Some(header) = tree.get_mut(DOCUMENT_JSON) {
            header["schema"] = Value::from(from + 1);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(schema: u64) -> Tree {
        Tree::from([(
            DOCUMENT_JSON.to_owned(),
            serde_json::json!({ "schema": schema }),
        )])
    }

    fn mark(tree: &mut Tree) -> Result<(), String> {
        let n = tree.len();
        tree.insert(format!("mark{n}"), Value::Null);
        Ok(())
    }

    fn fail(_: &mut Tree) -> Result<(), String> {
        Err("no".into())
    }

    #[test]
    fn links_run_in_order_from_the_documents_schema() {
        let mut t = tree(1);
        run(&mut t, 1, &[mark, mark, mark]).unwrap();
        assert_eq!(t[DOCUMENT_JSON]["schema"], 4);
        assert_eq!(t.len(), 4);
        let mut t = tree(3);
        run(&mut t, 3, &[mark, mark, mark]).unwrap();
        assert_eq!(t[DOCUMENT_JSON]["schema"], 4);
        assert_eq!(t.len(), 2, "only the last link ran");
    }

    #[test]
    fn a_failing_link_is_named() {
        let mut t = tree(1);
        let e = run(&mut t, 1, &[mark, fail, mark]).unwrap_err();
        assert_eq!((e.from, e.message.as_str()), (2, "no"));
    }

    #[test]
    fn the_chain_reaches_the_current_schema() {
        assert_eq!(CHAIN.len() as u64 + 1, crate::open::SCHEMA);
    }
}
