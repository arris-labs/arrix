//! `arrix serve`: the session protocol (ADR-0023). `--schema` prints the
//! protocol's JSON Schemas, derived from its types (`arrix_doc::
//! protocol_schemas`) and kept fresh against `tests/golden/protocol.schema.json`.

use std::io::Write as _;
use std::process::ExitCode;

pub fn run(schema: bool) -> ExitCode {
    if !schema {
        eprintln!("arrix serve: only --schema is served yet");
        return ExitCode::from(2);
    }
    let text = serde_json::to_string_pretty(&arrix_doc::protocol_schemas())
        .expect("a schema is plain data");
    if writeln!(std::io::stdout(), "{text}").is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}
