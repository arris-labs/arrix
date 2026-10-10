//! `arrix test <plugin-dir>`: the test kit for a plugin of any tier
//! (docs/PLUGINS.md §The test kit, ADR-0026). It reads the manifest's
//! `[test]` table and runs each check on each fixture document, printing one
//! JSON line per check. The report's `format` is versioned like the eval
//! line's; `--schema` prints its JSON Schema.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arrix_doc::{DocumentSource, Evaluator, MemorySource, Registry, open, save};
use arrix_plugin_host::{MANIFEST, Manifest};
use serde::Serialize;

use crate::load::{Loaded, registry_for};
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
    let kit = match Kit::open(args) {
        Ok(kit) => kit,
        Err(diagnostics) => {
            for d in diagnostics {
                eprintln!("arrix test: {d}");
            }
            return ExitCode::from(2);
        }
    };
    let mut failed = false;
    for fixture in &kit.manifest.test.documents {
        for (check, result) in kit.document_checks(&fixture.path) {
            let (status, detail) = match result {
                Ok(detail) => ("pass", detail),
                Err(detail) => {
                    failed = true;
                    ("fail", detail)
                }
            };
            let line = Line {
                format: FORMAT,
                plugin: kit.manifest.id.as_str(),
                check,
                doc: &fixture.path,
                status,
                detail,
            };
            let json = serde_json::to_string(&line).expect("a report line is plain data");
            if writeln!(std::io::stdout(), "{json}").is_err() {
                return ExitCode::from(2);
            }
        }
    }
    if failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// A plugin under test, its manifest checked.
struct Kit {
    dir: PathBuf,
    manifest: Manifest,
    /// The Tier 2 directories every fresh registry starts: the plugin's
    /// own, and `--plugin` ones it depends on.
    tier2: Vec<PathBuf>,
}

impl Kit {
    /// Reads the manifest and checks everything a run needs; the
    /// diagnostics name the manifest's file and line.
    fn open(args: &Args) -> Result<Kit, Vec<String>> {
        let file = args.dir.join(MANIFEST);
        let shown = file.display();
        let text = std::fs::read_to_string(&file).map_err(|e| vec![format!("{shown}: {e}")])?;
        let manifest =
            Manifest::parse(&text).map_err(|e| vec![format!("{shown}: {}", one_line(&e))])?;
        let mut diagnostics = Vec::new();
        for fixture in manifest.test.documents.iter().chain(&manifest.test.fail) {
            if !args.dir.join(&fixture.path).exists() {
                diagnostics.push(format!(
                    "{shown}:{}: the fixture `{}` does not exist",
                    fixture.line, fixture.path
                ));
            }
        }
        let mut tier2 = Vec::new();
        match manifest.tier {
            0 => {
                if !plugins::manifests().iter().any(|m| m.id == manifest.id) {
                    diagnostics.push(format!(
                        "{shown}: the Tier 0 plugin `{}` is not built into this `arrix`",
                        manifest.id
                    ));
                }
            }
            2 => tier2.push(args.dir.to_path_buf()),
            tier => diagnostics.push(format!(
                "{shown}: tier {tier} plugins are not testable yet (C3)"
            )),
        }
        tier2.extend(args.plugins.iter().cloned());
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        Ok(Kit {
            dir: args.dir.to_path_buf(),
            manifest,
            tier2,
        })
    }

    /// A registry on a fresh kernel, with a fresh Tier 2 process.
    fn fresh(&self) -> Result<Registry, String> {
        registry_for(&self.tier2, &[])
    }

    /// Every check on one fixture document, in order, each its detail.
    fn document_checks(&self, fixture: &str) -> Vec<(&'static str, Result<String, String>)> {
        vec![
            ("determinism", self.determinism(fixture)),
            ("round_trip", self.round_trip(fixture)),
        ]
    }

    fn evaluated(&self, fixture: &str, source: &impl DocumentSource) -> Result<Evaluated, String> {
        evaluate_and_save(fixture, source, self.fresh()?)
    }

    fn fixture_source(&self, fixture: &str) -> Result<Loaded, String> {
        Loaded::open(&self.dir.join(fixture))
    }

    /// Two evaluations, each on a fresh kernel and a fresh Tier 2 process,
    /// save to identical bytes.
    fn determinism(&self, fixture: &str) -> Result<String, String> {
        let source = self.fixture_source(fixture)?;
        let first = self.evaluated(fixture, &source)?;
        let second = self.evaluated(fixture, &source)?;
        match differ(&first.files, &second.files) {
            Some(what) => Err(format!("two evaluations differ: {what}")),
            None => Ok(format!(
                "two evaluations save the same {} files",
                first.files.0.len()
            )),
        }
    }

    /// Save, open, save gives identical bytes and an identical eval line.
    fn round_trip(&self, fixture: &str) -> Result<String, String> {
        let source = self.fixture_source(fixture)?;
        let first = self.evaluated(fixture, &source)?;
        let second = self.evaluated(fixture, &first.files)?;
        if let Some(what) = differ(&first.files, &second.files) {
            return Err(format!(
                "the saved document does not reopen as saved: {what}"
            ));
        }
        if first.line != second.line {
            return Err(format!(
                "the eval line changes across a save:\n{}\n{}",
                first.line, second.line
            ));
        }
        Ok("save, open and save again are identical, and so is the eval line".into())
    }
}

/// A manifest error on one line, as the diagnostic carries it.
fn one_line(e: &dyn std::fmt::Display) -> String {
    e.to_string().split('\n').collect::<Vec<_>>().join(" ")
}

/// A document evaluated: its saved files with the plugin features' latest
/// results merged in, and its eval line.
struct Evaluated {
    files: MemorySource,
    line: String,
}

fn evaluate_and_save(
    name: &str,
    source: &impl DocumentSource,
    registry: Registry,
) -> Result<Evaluated, String> {
    let document = open(source).map_err(|e| e.to_string())?;
    let mut evaluator = Evaluator::new(registry);
    let evaluation = evaluator.evaluate(&document);
    let line =
        serde_json::to_string(&evaluation.line(name, &document)).map_err(|e| e.to_string())?;
    let mut saved = document;
    for (feature, frozen) in evaluator.frozen() {
        saved.freeze(*feature, frozen);
    }
    Ok(Evaluated {
        files: save(&saved),
        line,
    })
}

/// The first way two saves differ, or `None` when they are the same bytes.
fn differ(a: &MemorySource, b: &MemorySource) -> Option<String> {
    for (path, bytes) in &a.0 {
        match b.0.get(path) {
            None => return Some(format!("`{path}` is only in the first")),
            Some(other) if other != bytes => {
                return Some(format!(
                    "`{path}` differs ({} bytes, then {})",
                    bytes.len(),
                    other.len()
                ));
            }
            Some(_) => {}
        }
    }
    b.0.keys()
        .find(|path| !a.0.contains_key(*path))
        .map(|path| format!("`{path}` is only in the second"))
}
