//! The migration chain (docs/DATA-MODEL.md §File format, Versioning). A
//! migration is a pure function over the document's JSON tree, from one
//! schema to the next; `open` runs them in order.

use std::collections::BTreeMap;

use serde_json::Value;

use crate::open::DOCUMENT_JSON;

/// The document's JSON files parsed, by path: `document.json`,
/// `params.json` when present, and `parts/<id>.json` and
/// `assemblies/<id>.json` and `materials/<id>.json`.
pub(crate) type Tree = BTreeMap<String, Value>;

/// One link: schema `n` to `n + 1`, where `n` is its index plus one. It
/// leaves the header's `schema` to [`run`].
pub(crate) type Migration = fn(&mut Tree) -> Result<(), String>;

/// The chain this build ships: entry `i` reads schema `i + 1`.
pub(crate) const CHAIN: &[Migration] =
    &[schema_1_to_2, schema_2_to_3, schema_3_to_4, schema_4_to_5];

/// Schema 2 added `FeatureRecord.frozen` and `blobs/`, both optional, so a
/// schema-1 tree is a schema-2 tree bar its header.
fn schema_1_to_2(_: &mut Tree) -> Result<(), String> {
    Ok(())
}

/// Schema 3 added list inputs (a feature input may be an array of
/// references, ADR-0012) and a sketch entity's `sources`, both new shapes a
/// schema-2 reader cannot parse and a schema-2 document never holds, so a
/// schema-2 tree is a schema-3 tree bar its header.
fn schema_2_to_3(_: &mut Tree) -> Result<(), String> {
    Ok(())
}

/// Schema 4 added `FeatureRecord.file` (omitted when absent) and
/// `assemblies/`, both new to a schema-3 document, so a schema-3 tree is a
/// schema-4 tree bar its header.
fn schema_3_to_4(_: &mut Tree) -> Result<(), String> {
    Ok(())
}

/// Schema 5 added `materials/`, and a part's `material` and
/// `body_materials` (omitted when empty), all new to a schema-4 document,
/// so a schema-4 tree is a schema-5 tree bar its header (ADR-0031).
fn schema_4_to_5(_: &mut Tree) -> Result<(), String> {
    Ok(())
}

/// The core feature types' own migrations, run on every open after the
/// schema chain (a feature record's `type_version` is migrated by its type,
/// docs/DATA-MODEL.md §File format). `core.boolean` 1 had one `tool` body
/// input; 2 has a `tools` list, so a version-1 record's `tool` becomes a
/// list of one. `core.import-step` 1 becomes 2, which only adds a choice.
pub(crate) fn core_types(tree: &mut Tree) {
    for (path, part) in tree.iter_mut() {
        if !path.starts_with("parts/") {
            continue;
        }
        let Some(features) = part.get_mut("features").and_then(Value::as_object_mut) else {
            continue;
        };
        for record in features.values_mut() {
            if record["type_id"] == "core.boolean"
                && record["type_version"] == 1
                && let Some(inputs) = record["inputs"].as_object_mut()
            {
                if let Some(tool) = inputs.remove("tool") {
                    inputs.insert("tools".into(), Value::Array(vec![tool]));
                }
                record["type_version"] = Value::from(2);
            }
            // `core.import-step` 2 added the `frame` choice, whose default
            // is version 1's behaviour.
            if record["type_id"] == "core.import-step" && record["type_version"] == 1 {
                record["type_version"] = Value::from(2);
            }
        }
    }
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
    fn a_boolean_of_version_1_gets_a_list_of_tools() {
        let record = |version: u64| {
            serde_json::json!({
                "type_id": "core.boolean", "type_version": version,
                "inputs": {"target": {"slot": 1}, "tool": {"slot": 2}},
            })
        };
        let mut t = tree(3);
        t.insert(
            "parts/P.json".into(),
            serde_json::json!({"features": {"a": record(1), "b": record(2)}}),
        );
        core_types(&mut t);
        let features = &t["parts/P.json"]["features"];
        assert_eq!(features["a"]["type_version"], 2);
        assert_eq!(
            features["a"]["inputs"]["tools"],
            serde_json::json!([{"slot": 2}])
        );
        assert!(features["a"]["inputs"].get("tool").is_none());
        assert_eq!(features["b"], record(2), "version 2 is left alone");
        let again = t.clone();
        core_types(&mut t);
        assert_eq!(t, again, "it is idempotent");
    }

    #[test]
    fn an_import_of_version_1_reads_in_the_world_frame_as_before() {
        let record = |version: u64| {
            serde_json::json!({
                "type_id": "core.import-step", "type_version": version,
                "params": {"solid": "2"},
            })
        };
        let mut t = tree(5);
        t.insert(
            "parts/P.json".into(),
            serde_json::json!({"features": {"a": record(1), "b": record(2)}}),
        );
        core_types(&mut t);
        let features = &t["parts/P.json"]["features"];
        // Only the version moves: `frame` is absent, so its default, world.
        assert_eq!(features["a"], record(2));
        assert_eq!(features["b"], record(2));
    }

    #[test]
    fn the_chain_reaches_the_current_schema() {
        assert_eq!(CHAIN.len() as u64 + 1, crate::open::SCHEMA);
    }
}
