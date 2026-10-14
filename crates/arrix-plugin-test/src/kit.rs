//! The kit's checks: a plugin's manifest and `[test]` table run against its
//! fixtures (docs/PLUGINS.md §The test kit, ADR-0026). The tier a plugin
//! has decides only how [`Env`] makes a fresh registry.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use arrix_core::{ComponentId, Id, PluginId};
use arrix_doc::{
    Command, ComponentOf, DEFAULT_MAX_NEW, Document, DocumentSource, EdgePick, EvalLine, Evaluator,
    Expansion, FeatureLine, MemorySource, Registry, apply, open, save,
};
use arrix_plugin_api::{Context, Subject};
use arrix_plugin_host::{CommandFixture, ExportFixture, MANIFEST, Manifest};

use crate::refs::{assembly_id, part_id, split_options};
use crate::source::Loaded;

/// How a run makes its registries: a fresh one on a fresh kernel (and a
/// fresh Tier 2 process or Tier 1 store) with the plugin directories of
/// `with` loaded and the plugins of `without` left out; and which plugins
/// are compiled in.
/// Makes a registry: the plugin directories to load, the plugin ids to leave out.
pub type MakeRegistry = Box<dyn Fn(&[PathBuf], &[String]) -> Result<Registry, String>>;

pub struct Env {
    pub registry: MakeRegistry,
    pub compiled: Vec<PluginId>,
}

/// One check's outcome, on the fixture (or the manifest) it was made on.
pub struct Check {
    pub check: &'static str,
    pub doc: String,
    pub result: Result<String, String>,
}

/// A plugin under test, its manifest checked.
pub struct Kit {
    env: Env,
    dir: PathBuf,
    manifest: Manifest,
    /// The directories (Tier 1 or 2) every fresh registry loads: the plugin's
    /// own, and `--plugin` ones it depends on.
    dirs: Vec<PathBuf>,
    /// The `--plugin` directories alone: what a run without the plugin keeps.
    extra: Vec<PathBuf>,
}

impl Kit {
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Reads the manifest and checks everything a run needs; the
    /// diagnostics name the manifest's file and line.
    pub fn open(env: Env, dir: &Path, plugins: &[PathBuf]) -> Result<Kit, Vec<String>> {
        let file = dir.join(MANIFEST);
        let shown = file.display();
        let text = std::fs::read_to_string(&file).map_err(|e| vec![format!("{shown}: {e}")])?;
        let manifest =
            Manifest::parse(&text).map_err(|e| vec![format!("{shown}: {}", one_line(&e))])?;
        let mut diagnostics = Vec::new();
        let exported = manifest.test.exports.iter().map(|e| &e.document);
        let commanded = manifest.test.commands.iter().map(|c| &c.document);
        for fixture in manifest
            .test
            .documents
            .iter()
            .chain(&manifest.test.fail)
            .chain(exported)
            .chain(commanded)
        {
            if !dir.join(&fixture.path).exists() {
                diagnostics.push(format!(
                    "{shown}:{}: the fixture `{}` does not exist",
                    fixture.line, fixture.path
                ));
            }
        }
        let mut dirs = Vec::new();
        match manifest.tier {
            0 => {
                if !env.compiled.contains(&manifest.id) {
                    diagnostics.push(format!(
                        "{shown}: the Tier 0 plugin `{}` is not built into this `arrix`",
                        manifest.id
                    ));
                }
            }
            1 | 2 => dirs.push(dir.to_path_buf()),
            tier => diagnostics.push(format!("{shown}: tier {tier} is not a tier")),
        }
        dirs.extend(plugins.iter().cloned());
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        Ok(Kit {
            env,
            dir: dir.to_path_buf(),
            manifest,
            dirs,
            extra: plugins.to_vec(),
        })
    }

    /// A registry on a fresh kernel, with a fresh Tier 2 process or Tier 1 store.
    pub fn fresh(&self) -> Result<Registry, String> {
        (self.env.registry)(&self.dirs, &[])
    }

    /// Every check, in order: the plugin's specs, each fixture document's
    /// checks, the migration coverage, then each `fail` fixture.
    pub fn checks(&self) -> Vec<Check> {
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
        for export in &self.manifest.test.exports {
            out.push(Check {
                check: "export",
                doc: export.document.path.clone(),
                result: self.export_twice(export),
            });
        }
        for command in &self.manifest.test.commands {
            out.push(Check {
                check: "command",
                doc: command.document.path.clone(),
                result: self.command_twice(command),
            });
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
        if self.env.compiled.contains(&self.manifest.id) {
            (self.env.registry)(&self.extra, &[self.manifest.id.to_string()])
        } else {
            (self.env.registry)(&self.extra, &[])
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
        let exporters: Vec<String> = registry
            .exporter_specs()
            .into_iter()
            .map(|e| e.id)
            .filter(|id| self.owns(id))
            .collect();
        let declared = &self.manifest.contributes.exporters;
        if let Some(id) = declared.iter().find(|id| !exporters.contains(id)) {
            return Err(format!(
                "the manifest contributes the exporter `{id}`, which the plugin does not register"
            ));
        }
        if let Some(id) = exporters.iter().find(|id| !declared.contains(id)) {
            return Err(format!(
                "the plugin registers the exporter `{id}`, which the manifest's `contributes.exporters` does not list"
            ));
        }
        let commands: Vec<String> = registry
            .command_specs()
            .into_iter()
            .map(|c| c.id)
            .filter(|id| self.owns(id))
            .collect();
        let declared = &self.manifest.contributes.commands;
        if let Some(id) = declared.iter().find(|id| !commands.contains(id)) {
            return Err(format!(
                "the manifest contributes the command `{id}`, which the plugin does not register"
            ));
        }
        if let Some(id) = commands.iter().find(|id| !declared.contains(id)) {
            return Err(format!(
                "the plugin registers the command `{id}`, which the manifest's `contributes.commands` does not list"
            ));
        }
        Ok(format!(
            "{} feature type(s), {} exporter(s) and {} command(s) register with a valid spec",
            registered.len(),
            exporters.len(),
            commands.len()
        ))
    }

    /// An export of the manifest runs twice, each on a fresh registry (a
    /// fresh Tier 1 store or Tier 2 process), to identical files
    /// (ADR-0037 §9); the detail names the files and their hashes.
    fn export_twice(&self, export: &ExportFixture) -> Result<String, String> {
        let id = export.exporter.as_str();
        if !self.owns(id) {
            return Err(format!("`{id}` is not an exporter of this plugin"));
        }
        let source = self.fixture_source(&export.document.path)?;
        let document = open(&source).map_err(|e| e.to_string())?;
        let registry = self.fresh()?;
        let Some(exporter) = registry.exporter(id) else {
            return Err(format!("the plugin registers no exporter `{id}`"));
        };
        let subject = match (&export.assembly, &export.part) {
            (Some(a), None) => Subject::Assembly(assembly_id(&document, a)?),
            (None, Some(p)) => Subject::Part(part_id(&document, p)?),
            _ => return Err("an export names its `assembly` or its `part`, one of them".into()),
        };
        let options: Vec<String> = export
            .options
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect();
        let (params, choices) = split_options(&exporter.spec().choices, &options)?;
        let run = || {
            arrix_doc::export_with_exporter(
                &export.document.path,
                &source,
                &self.fresh()?,
                id,
                subject,
                &params,
                &choices,
            )
            .map(|done| done.files)
            .map_err(|e| e.to_string())
        };
        let (first, second) = (run()?, run()?);
        if first != second {
            let names = |files: &[arrix_plugin_api::ExportFile]| {
                files.iter().map(|f| f.name.clone()).collect::<Vec<_>>()
            };
            return Err(match (names(&first), names(&second)) {
                (a, b) if a != b => format!("two exports write different files: {a:?}, then {b:?}"),
                _ => {
                    let at = first.iter().zip(&second).find(|(a, b)| a != b);
                    format!(
                        "two exports differ in `{}`",
                        at.map_or("?", |(a, _)| a.name.as_str())
                    )
                }
            });
        }
        let listed: Vec<String> = first
            .iter()
            .map(|f| {
                let hash = blake3::hash(&f.bytes).to_hex();
                format!(
                    "{} ({} bytes, blake3 {})",
                    f.name,
                    f.bytes.len(),
                    &hash[..16]
                )
            })
            .collect();
        Ok(format!(
            "two exports write the same {} file(s): {}",
            first.len(),
            listed.join("; ")
        ))
    }

    /// A command of the manifest expands twice, each on a fresh registry
    /// and evaluation, to identical groups; the group applies to the
    /// fixture, and its inverse takes the document back to the fixture's
    /// bytes (ADR-0038 §6).
    fn command_twice(&self, fixture: &CommandFixture) -> Result<String, String> {
        if !self.owns(&fixture.command) {
            return Err(format!(
                "`{}` is not a command of this plugin",
                fixture.command
            ));
        }
        let source = self.fixture_source(&fixture.document.path)?;
        expand_twice_and_undo(&|| self.fresh(), &source, fixture)
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

/// The kit's command check over any way of making a fresh registry: the
/// fixture's command expands twice to the same group, which applies and
/// undoes to the fixture's bytes.
pub fn expand_twice_and_undo(
    fresh: &dyn Fn() -> Result<Registry, String>,
    source: &impl DocumentSource,
    fixture: &CommandFixture,
) -> Result<String, String> {
    let id = fixture.command.as_str();
    let document = open(source).map_err(|e| e.to_string())?;
    let registry = fresh()?;
    let Some(command) = registry.command(id) else {
        return Err(format!("the plugin registers no command `{id}`"));
    };
    let context = match (&fixture.assembly, &fixture.part) {
        (Some(a), None) => Context::Assembly(assembly_id(&document, a)?),
        (None, Some(p)) => Context::Part(part_id(&document, p)?),
        _ => return Err("a command names its `assembly` or its `part`, one of them".into()),
    };
    let options: Vec<String> = fixture
        .options
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    let (params, choices) = split_options(&command.spec().choices, &options)?;
    let mut picks = BTreeMap::new();
    for (name, list) in &fixture.inputs {
        let mut one = Vec::new();
        for pick in list {
            one.push(EdgePick {
                path: component_path(&document, context, &pick.path)?,
                edge: pick
                    .edge
                    .parse()
                    .map_err(|e| format!("input `{name}`: {}: {e}", pick.edge))?,
            });
        }
        picks.insert(name.clone(), one);
    }
    let ids = kit_ids(command.spec().max_new);
    let run = || -> Result<Command, String> {
        let registry = fresh()?;
        let command = registry
            .command(id)
            .ok_or_else(|| format!("the plugin registers no command `{id}`"))?
            .clone();
        let mut evaluator = Evaluator::new(registry);
        let evaluation = evaluator.evaluate(&document);
        evaluator
            .expand(
                &document,
                &evaluation,
                &Expansion {
                    command: command.as_ref(),
                    context,
                    params: &params,
                    choices: &choices,
                    picks: &picks,
                    ids: &ids,
                },
            )
            .map_err(|d| format!("{}: {}", d.code.as_str(), d.message))
    };
    let (first, second) = (run()?, run()?);
    if first != second {
        return Err("two expansions differ".into());
    }
    let applied = apply(&document, &first).map_err(|e| format!("the group is refused: {e}"))?;
    let back = apply(&applied.document, &applied.inverse)
        .map_err(|e| format!("the group's inverse is refused: {e}"))?;
    if let Some(what) = differ(&save(&document), &save(&back.document)) {
        return Err(format!(
            "undoing the group does not restore the document: {what}"
        ));
    }
    let records = match &first {
        Command::Group { commands, .. } => commands.len(),
        _ => 1,
    };
    Ok(format!(
        "two expansions give the same group of {records} record(s); it applies, and undoes to the fixture's bytes"
    ))
}

/// The ids a kit expansion mints with: fixed, so two runs are one.
fn kit_ids(max_new: Option<u32>) -> Vec<Id> {
    let n = max_new.map_or(DEFAULT_MAX_NEW, |n| n as usize);
    (1..=n as u64).map(|i| Id(0x5EED_0000_0000 + i)).collect()
}

/// A picked edge's components, each a name or an id in the assembly the
/// level before it ends in (a part has none).
fn component_path(
    document: &Document,
    context: Context,
    path: &[String],
) -> Result<Vec<ComponentId>, String> {
    let Context::Assembly(mut at) = context else {
        return if path.is_empty() {
            Ok(Vec::new())
        } else {
            Err("a pick in a part has no component path".into())
        };
    };
    let mut out = Vec::new();
    for step in path {
        let assembly = document
            .assemblies()
            .get(&at)
            .ok_or_else(|| format!("no assembly {at}"))?;
        let by_id = step
            .parse::<ComponentId>()
            .ok()
            .filter(|id| assembly.components.contains_key(id));
        let mut named = assembly
            .components
            .values()
            .filter(|c| &c.name == step)
            .map(|c| c.id);
        let id = match (by_id, named.next(), named.next()) {
            (Some(id), _, _) | (None, Some(id), None) => id,
            (None, Some(_), Some(_)) => {
                return Err(format!("`{step}` names several components; use its id"));
            }
            (None, None, _) => return Err(format!("no component `{step}` in {}", assembly.name)),
        };
        out.push(id);
        if let ComponentOf::Assembly(next) = assembly.components[&id].of {
            at = next;
        }
    }
    Ok(out)
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

#[cfg(test)]
#[path = "kit_tests.rs"]
mod tests;
