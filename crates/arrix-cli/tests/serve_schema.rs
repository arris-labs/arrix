//! `arrix serve --schema` (ADR-0023 §5): the session protocol's schemas,
//! held to `tests/golden/protocol.schema.json` and rewritten under
//! `UPDATE_SNAPSHOTS=1`, so a change to a protocol type shows up as a
//! reviewed diff of what an agent reads.

use std::path::Path;
use std::process::Command;

const GOLDEN: &str = "tests/golden/protocol.schema.json";

#[test]
fn the_protocol_schemas_are_fresh() {
    let out = Command::new(env!("CARGO_BIN_EXE_arrix"))
        .args(["serve", "--schema"])
        .output()
        .expect("run arrix");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).expect("UTF-8");
    let golden = Path::new(env!("CARGO_MANIFEST_DIR")).join(GOLDEN);
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&golden, &text).expect("the golden is written");
    }
    assert!(
        text == std::fs::read_to_string(&golden).unwrap_or_default(),
        "{GOLDEN} differs from `arrix serve --schema`; rerun with UPDATE_SNAPSHOTS=1 and read the diff"
    );
    let schemas: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(schemas["protocol"], arrix_doc::PROTOCOL_VERSION);
    for kind in ["request", "outcome", "event"] {
        assert_eq!(
            schemas[kind]["$schema"], "https://json-schema.org/draft/2020-12/schema",
            "{kind}"
        );
    }
}

#[test]
fn serve_without_a_mode_is_a_usage_error() {
    let out = Command::new(env!("CARGO_BIN_EXE_arrix"))
        .arg("serve")
        .output()
        .expect("run arrix");
    assert_eq!(out.status.code(), Some(2));
}
