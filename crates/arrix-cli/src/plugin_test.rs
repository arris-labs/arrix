//! `arrix test <plugin-dir>`: the test kit for a plugin of any tier
//! (docs/PLUGINS.md §The test kit, ADR-0026), whose checks are
//! `arrix-plugin-test`'s. It reads the manifest's
//! `[test]` table and runs each check on each fixture document, printing one
//! JSON line per check. The report's `format` is versioned like the eval
//! line's; `--schema` prints its JSON Schema.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arrix_plugin_test::{Env, Kit};
use serde::Serialize;

use crate::load::registry_for;
use crate::plugins;

/// The version of a report line, raised when a reader would have to change.
pub const FORMAT: u32 = 1;

pub struct Args<'a> {
    pub dir: &'a Path,
    pub plugins: &'a [PathBuf],
}

/// One check's result.
#[derive(Serialize)]
struct Line<'a> {
    format: u32,
    plugin: &'a str,
    check: &'a str,
    doc: &'a str,
    status: &'static str,
    detail: String,
}

/// `--schema`: the JSON Schema of one report line.
pub fn print_schema() -> ExitCode {
    let schema = serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": "arrix test report line",
        "description": "One line per check on a fixture document. `status` is `pass` or `fail`; `detail` says what was compared, or what differed.",
        "type": "object",
        "properties": {
            "format": {"description": "This schema's version.", "const": FORMAT},
            "plugin": {"description": "The plugin's id.", "type": "string"},
            "check": {
                "description": "The check; a reader tolerates ones it does not know.",
                "type": "string"
            },
            "doc": {"description": "The fixture, as the manifest names it.", "type": "string"},
            "status": {"enum": ["pass", "fail"]},
            "detail": {"type": "string"}
        },
        "required": ["format", "plugin", "check", "doc", "status", "detail"],
        "additionalProperties": true
    });
    let text = serde_json::to_string_pretty(&schema).expect("a schema is plain data");
    if writeln!(std::io::stdout(), "{text}").is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// Exit status: 0 when every check passed, 1 when one failed, 2 when the
/// plugin could not be tested at all: a manifest that is not valid, a
/// fixture that is not there, a plugin this build cannot run.
pub fn run(args: &Args) -> ExitCode {
    let env = Env {
        registry: Box::new(registry_for),
        compiled: plugins::manifests().into_iter().map(|m| m.id).collect(),
    };
    let kit = match Kit::open(env, args.dir, args.plugins) {
        Ok(kit) => kit,
        Err(diagnostics) => {
            for d in diagnostics {
                eprintln!("arrix test: {d}");
            }
            return ExitCode::from(2);
        }
    };
    let mut failed = false;
    for check in kit.checks() {
        let (status, detail) = match check.result {
            Ok(detail) => ("pass", detail),
            Err(detail) => {
                failed = true;
                ("fail", detail)
            }
        };
        let line = Line {
            format: FORMAT,
            plugin: kit.manifest().id.as_str(),
            check: check.check,
            doc: &check.doc,
            status,
            detail,
        };
        let json = serde_json::to_string(&line).expect("a report line is plain data");
        if writeln!(std::io::stdout(), "{json}").is_err() {
            return ExitCode::from(2);
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
