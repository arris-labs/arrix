//! `arrix test <plugin-dir>`: the test kit for a plugin of any tier
//! (docs/PLUGINS.md §The test kit, ADR-0026). It reads the manifest's
//! `[test]` table and runs each check on each fixture document, printing one
//! JSON line per check. The report's `format` is versioned like the eval
//! line's; `--schema` prints its JSON Schema.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arrix_doc::{
    DocumentSource, EvalLine, Evaluator, FeatureLine, MemorySource, Registry, open, save,
};
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
            plugin: kit.manifest.id.as_str(),
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

/// One check's outcome, on the fixture (or the manifest) it was made on.
struct Check {
    check: &'static str,
    doc: String,
    result: Result<String, String>,
}

/// A plugin under test, its manifest checked.
struct Kit {
    dir: PathBuf,
    manifest: Manifest,
    /// The directories (Tier 1 or 2) every fresh registry loads: the plugin's
    /// own, and `--plugin` ones it depends on.
    dirs: Vec<PathBuf>,
    /// The `--plugin` directories alone: what a run without the plugin keeps.
    extra: Vec<PathBuf>,
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
        let mut dirs = Vec::new();
        match manifest.tier {
            0 => {
                if !plugins::manifests().iter().any(|m| m.id == manifest.id) {
                    diagnostics.push(format!(
                        "{shown}: the Tier 0 plugin `{}` is not built into this `arrix`",
                        manifest.id
                    ));
                }
            }
            1 | 2 => dirs.push(args.dir.to_path_buf()),
            tier => diagnostics.push(format!("{shown}: tier {tier} is not a tier")),
        }
        dirs.extend(args.plugins.iter().cloned());
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        Ok(Kit {
            dir: args.dir.to_path_buf(),
            manifest,
            dirs,
            extra: args.plugins.to_vec(),
        })
    }

    /// A registry on a fresh kernel, with a fresh Tier 2 process or Tier 1 store.
    fn fresh(&self) -> Result<Registry, String> {
        registry_for(&self.dirs, &[])
    }

    /// Every check, in order: the plugin's specs, each fixture document's
    /// checks, the migration coverage, then each `fail` fixture.
    fn checks(&self) -> Vec<Check> {
        let manifest_file = MANIFEST.to_string();
        let mut out = vec![Check {
            check: "spec",
            doc: manifest_file.clone(),
            result: self.spec(),
        }];
        for fixture in &self.manifest.test.documents {
            let path = &fixture.path;
            for (check, result) in [
                ("determinism", self.determinism(path)),
                ("round_trip", self.round_trip(path)),
                ("frozen", self.frozen(path)),
            ] {
                out.push(Check {
                    check,
                    doc: path.clone(),
                    result,
                });
            }
        }
        out.extend(self.migration(&manifest_file));
        for fixture in &self.manifest.test.fail {
            out.push(Check {
                check: "kernel_failure",
                doc: fixture.path.clone(),
                result: self.kernel_failure(&fixture.path),
            });
        }
        out
    }

    /// Whether `type_id` is one of this plugin's.
    fn owns(&self, type_id: &str) -> bool {
        type_id.split('.').next() == Some(self.manifest.id.as_str())
    }

    /// The registry of a run without the plugin.
    fn without(&self) -> Result<Registry, String> {
        let compiled = plugins::manifests()
            .iter()
            .any(|m| m.id == self.manifest.id);
        if compiled {
            registry_for(&self.extra, &[self.manifest.id.to_string()])
        } else {
            registry_for(&self.extra, &[])
        }
    }

    /// Every feature type the manifest contributes registers, which is
    /// where its spec (choices, `modifies`) is validated (ADR-0018), and the
    /// plugin registers no other.
    fn spec(&self) -> Result<String, String> {
        let registry = self.fresh()?;
        let registered: Vec<String> = registry
            .ids()
            .map(|id| id.to_string())
            .filter(|id| self.owns(id))
            .collect();
        let declared = &self.manifest.contributes.features;
        if let Some(id) = declared.iter().find(|id| !registered.contains(id)) {
            return Err(format!(
                "the manifest contributes `{id}`, which the plugin does not register"
            ));
        }
        if let Some(id) = registered.iter().find(|id| !declared.contains(id)) {
            return Err(format!(
                "the plugin registers `{id}`, which the manifest's `contributes.features` does not list"
            ));
        }
        Ok(format!(
            "{} feature type(s) register with a valid spec",
            registered.len()
        ))
    }

    /// Every `type_version` a spec declares, from 1 up, has a fixture
    /// document that holds a feature at it and evaluates it. The WIT has no
    /// `migrate` yet, so a fixture is the only proof an old record opens.
    fn migration(&self, manifest_file: &str) -> Vec<Check> {
        let Ok(registry) = self.fresh() else {
            return Vec::new();
        };
        let mut held = Vec::new();
        for fixture in &self.manifest.test.documents {
            let Ok(source) = self.fixture_source(&fixture.path) else {
                continue;
            };
            let (Ok(document), Ok(line)) = (
                open(&source),
                arrix_doc::eval(&fixture.path, &source, &registry),
            ) else {
                continue;
            };
            for part in document.parts().values() {
                for record in part.features.values() {
                    let evaluates = line
                        .features
                        .iter()
                        .any(|f| f.id == record.id && f.status != "failed");
                    held.push((
                        &fixture.path,
                        record.type_id.to_string(),
                        record.type_version,
                        evaluates,
                    ));
                }
            }
        }
        let mut out = Vec::new();
        for id in registry.ids().filter(|id| self.owns(&id.to_string())) {
            let version = registry.get(id).map_or(1, |ty| ty.spec().version);
            for v in 1..=version {
                let holders: Vec<_> = held
                    .iter()
                    .filter(|(_, ty, at, _)| *ty == id.to_string() && *at == v)
                    .collect();
                let (doc, result) = match holders.iter().find(|h| h.3) {
                    Some((path, ..)) => (
                        (*path).clone(),
                        Ok(format!("`{id}` at type_version {v} evaluates")),
                    ),
                    None => match holders.first() {
                        Some((path, ..)) => (
                            (*path).clone(),
                            Err(format!("`{id}` at type_version {v} does not evaluate")),
                        ),
                        None => (
                            manifest_file.to_string(),
                            Err(format!("no fixture holds `{id}` at type_version {v}")),
                        ),
                    },
                };
                out.push(Check {
                    check: "migration",
                    doc,
                    result,
                });
            }
        }
        out
    }

    /// Opened without the plugin, each of its features is frozen with a
    /// reason and keeps its bodies, and what is downstream still evaluates.
    fn frozen(&self, fixture: &str) -> Result<String, String> {
        let source = self.fixture_source(fixture)?;
        let live = self.evaluated(fixture, &source)?;
        let mine: Vec<&FeatureLine> = live
            .line
            .features
            .iter()
            .filter(|f| self.owns(&f.feature_type))
            .collect();
        if mine.is_empty() {
            return Err("the fixture holds no feature of the plugin, so nothing freezes".into());
        }
        if let Some(f) = mine.iter().find(|f| f.status != "ok") {
            return Err(format!(
                "`{}` is {} with the plugin, so it has no result to freeze",
                f.feature_type, f.status
            ));
        }
        let frozen =
            arrix_doc::eval(fixture, &live.files, &self.without()?).map_err(|e| e.to_string())?;
        for f in &mine {
            let Some(after) = frozen.features.iter().find(|a| a.id == f.id) else {
                return Err(format!("`{}` is gone without the plugin", f.feature_type));
            };
            if after.status != "frozen" {
                return Err(format!(
                    "`{}` is {} without the plugin, not frozen",
                    f.feature_type, after.status
                ));
            }
            if after.diagnostics.is_empty() {
                return Err(format!("`{}` is frozen with no reason", f.feature_type));
            }
            let bodies = |line: &EvalLine| {
                line.bodies
                    .iter()
                    .filter(|b| b.feature == f.id)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            if bodies(&live.line) != bodies(&frozen) {
                return Err(format!(
                    "`{}` does not keep its bodies when frozen",
                    f.feature_type
                ));
            }
        }
        if let Some(f) = frozen
            .features
            .iter()
            .find(|f| !self.owns(&f.feature_type) && f.status == "failed")
        {
            return Err(format!(
                "`{}` fails without the plugin ({})",
                f.feature_type,
                f.category.as_deref().unwrap_or("no category")
            ));
        }
        Ok(format!(
            "{} feature(s) frozen with a reason and their bodies; the rest evaluates",
            mine.len()
        ))
    }

    /// A `fail` fixture fails at a kernel call of the plugin's with a
    /// `kernel.*` category, and the call's recipe is one Arris can replay
    /// (`SEED.md` §6.6).
    fn kernel_failure(&self, fixture: &str) -> Result<String, String> {
        let source = self.fixture_source(fixture)?;
        let run =
            arrix_doc::run(fixture, &source, &self.fresh()?, None).map_err(|e| e.to_string())?;
        let Some(failed) = run
            .line
            .features
            .iter()
            .find(|f| self.owns(&f.feature_type) && f.status == "failed")
        else {
            return Err("no feature of the plugin fails; a `fail` fixture must".into());
        };
        let category = failed.category.as_deref().unwrap_or("none");
        if !category.starts_with("kernel.") {
            return Err(format!(
                "`{}` fails with `{category}`, not a `kernel.*` category",
                failed.feature_type
            ));
        }
        let Some(call) = run.failures.iter().find(|c| c.feature == failed.id) else {
            return Err(format!("`{category}` left no kernel call to replay"));
        };
        let recipe = call
            .recipe
            .as_ref()
            .map_err(|e| format!("`{category}` has no recipe: {e}"))?;
        let steps = replayable(&recipe.json)?;
        Ok(format!(
            "`{category}` with a recipe of {steps} step(s), `{}`",
            recipe.name
        ))
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
            let json = |l: &EvalLine| serde_json::to_string(l).unwrap_or_default();
            return Err(format!(
                "the eval line changes across a save:\n{}\n{}",
                json(&first.line),
                json(&second.line)
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
    line: EvalLine,
}

fn evaluate_and_save(
    name: &str,
    source: &impl DocumentSource,
    registry: Registry,
) -> Result<Evaluated, String> {
    let document = open(source).map_err(|e| e.to_string())?;
    let mut evaluator = Evaluator::new(registry);
    let evaluation = evaluator.evaluate(&document);
    let line = evaluation.line(name, &document);
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

/// What Arris's interpreter holds a recipe to: step names unique, every
/// step reading only earlier ones, the result a step. Its step count.
fn replayable(json: &str) -> Result<usize, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("the recipe is not JSON: {e}"))?;
    let steps = value["steps"]
        .as_array()
        .filter(|steps| !steps.is_empty())
        .ok_or("the recipe has no steps")?;
    let mut made = std::collections::BTreeSet::new();
    for step in steps {
        for key in ["profile", "of", "a", "b", "target", "tool"] {
            if let Some(read) = step[key].as_str()
                && !made.contains(read)
            {
                return Err(format!(
                    "the recipe's step {step} reads {read} before it is made"
                ));
            }
        }
        let name = step["name"].as_str().ok_or("a recipe step has no name")?;
        if !made.insert(name) {
            return Err(format!("the recipe makes {name} twice"));
        }
    }
    match value["result"].as_str() {
        Some(result) if made.contains(result) => Ok(steps.len()),
        _ => Err("the recipe's result is not one of its steps".into()),
    }
}
