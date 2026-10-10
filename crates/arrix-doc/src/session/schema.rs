//! The session protocol's JSON Schemas (ADR-0023 §5), derived from the
//! types themselves so they describe the serde forms the round-trip tests
//! already check. A request is described as the session reads it; an
//! outcome and an event as the session writes them. A variant behind a
//! sketch feature a document never holds (`conics`, `snells-law`,
//! `unreached`) is left out, so every build describes the same protocol.

use schemars::JsonSchema;
use schemars::generate::SchemaSettings;
use serde_json::{Value, json};

use super::{PROTOCOL_VERSION, Request, SessionEvent};
use crate::authority::Outcome;

/// `{"protocol": 1, "request": …, "outcome": …, "event": …}`, each a
/// standalone draft 2020-12 schema with its own `$defs`.
pub fn protocol_schemas() -> Value {
    json!({
        "protocol": PROTOCOL_VERSION,
        "request": read::<Request>(),
        "outcome": written::<Outcome>(),
        "event": written::<SessionEvent>(),
    })
}

/// `T` as the session reads it.
fn read<T: JsonSchema>() -> Value {
    let settings = SchemaSettings::draft2020_12().for_deserialize();
    settings
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value()
}

/// `T` as the session writes it.
fn written<T: JsonSchema>() -> Value {
    let settings = SchemaSettings::draft2020_12().for_serialize();
    settings
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value()
}

/// Fails unless the JSON `json` is valid against the schema of `kind`
/// (`request`, `outcome` or `event`).
#[cfg(test)]
pub(crate) fn validate(kind: &str, json: &str) {
    let errors = errors(kind, json);
    assert!(errors.is_empty(), "{kind} {json}\n{}", errors.join("\n"));
}

/// What the schema of `kind` finds wrong with `json`.
#[cfg(test)]
fn errors(kind: &str, json: &str) -> Vec<String> {
    use std::collections::BTreeMap;
    use std::sync::{LazyLock, Mutex};

    static VALIDATORS: LazyLock<Mutex<BTreeMap<String, jsonschema::Validator>>> =
        LazyLock::new(Mutex::default);

    let mut validators = VALIDATORS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let validator = validators.entry(kind.to_owned()).or_insert_with(|| {
        jsonschema::validator_for(&protocol_schemas()[kind]).expect("the schema compiles")
    });
    let instance: Value = serde_json::from_str(json).expect("it is JSON");
    validator
        .iter_errors(&instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::errors;

    /// The schemas refuse what the session would refuse to read, so the
    /// round-trip cases passing them says something.
    #[test]
    fn a_wrong_shape_is_refused() {
        let wrong = [
            ("request", r#"{"bogus":{"author":"0000000000001"}}"#),
            ("request", r#"{"undo":{"author":"not an id"}}"#),
            ("request", r#"{"undo":{}}"#),
            (
                "request",
                r#"{"submit":{"author":"0000000000001","command":{"add_part":{}}}}"#,
            ),
            ("request", r#"{"export":{"bodies":[],"format":"iges"}}"#),
            ("outcome", r#""done""#),
            (
                "event",
                r#"{"kind":"finished","generation":-1,"params":{}}"#,
            ),
            ("event", r#"{"kind":"opened"}"#),
        ];
        for (kind, json) in wrong {
            assert!(!errors(kind, json).is_empty(), "{kind} {json} passed");
        }
        assert_eq!(
            errors("request", r#"{"undo":{"author":"0000000000001"}}"#),
            Vec::<String>::new()
        );
        assert_eq!(errors("outcome", r#""no_op""#), Vec::<String>::new());
    }
}
