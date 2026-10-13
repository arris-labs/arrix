//! `arrix export`: a document's final bodies as STEP, STL or OBJ, or an
//! assembly with its structure as STEP (ADR-0032 §5), through
//! `arrix_doc::export` and `export_assembly` (the paths the app's export
//! takes too); or a plugin's exporter on a part or an assembly
//! (`--exporter`, ADR-0037), whose files the host writes.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use arrix_core::{AssemblyId, ExportFormat, FeatureId, PartId, SlotName};
use arrix_doc::{BodySlot, Document, Registry};
use arrix_plugin_api::{ExportFile, ExporterSpec, Subject, SubjectKind};

use crate::load::{Loaded, registry_for};

pub struct Args<'a> {
    pub doc: &'a Path,
    /// Exactly one of `format` and `exporter` is given.
    pub format: Option<ExportFormat>,
    pub exporter: Option<&'a str>,
    pub bodies: &'a [String],
    pub assembly: Option<&'a str>,
    pub part: Option<&'a str>,
    /// `<name>=<value>`: an exporter's parameter (an expression) or choice
    /// (a word).
    pub options: &'a [String],
    pub force: bool,
    pub out: &'a Path,
    pub plugins: &'a [PathBuf],
    pub without: &'a [String],
}

pub fn run(args: &Args) -> ExitCode {
    match export(args) {
        Ok(status) => status,
        Err(err) => {
            eprintln!("arrix export: {err}");
            ExitCode::from(2)
        }
    }
}

fn export(args: &Args) -> Result<ExitCode, String> {
    let name = args.doc.to_string_lossy();
    let registry = registry_for(args.plugins, args.without)?;
    let source = Loaded::open(args.doc).map_err(|e| format!("{name}: {e}"))?;
    let document = arrix_doc::open(&source).map_err(|e| format!("{name}: {e}"))?;
    if args.exporter.is_none() && (!args.options.is_empty() || args.part.is_some() || args.force) {
        return Err("--option, --part and --force go with --exporter".into());
    }
    // The core's formats are exporters by id, `--format` their shorthand.
    let format = match (args.format, args.exporter) {
        (Some(format), _) => format,
        (None, Some(id)) => match id
            .strip_prefix("core.")
            .and_then(ExportFormat::from_extension)
        {
            Some(format) => {
                if !args.options.is_empty() || args.part.is_some() {
                    return Err(format!("{id} takes no --option and no --part"));
                }
                format
            }
            None => return plugin_export(args, &name, &registry, &source, &document),
        },
        (None, None) => return Err("give --format or --exporter".into()),
    };
    let exported = match args.assembly {
        Some(spec) => {
            if format != ExportFormat::Step {
                return Err("--assembly writes a STEP file: use --format step".into());
            }
            let assembly = assembly_id(&document, spec)?;
            arrix_doc::export_assembly(&name, &source, &registry, assembly)
        }
        None => {
            let bodies = args
                .bodies
                .iter()
                .map(|spec| body_slot(&document, spec))
                .collect::<Result<Vec<_>, _>>()?;
            arrix_doc::export(&name, &source, &registry, &bodies, format)
        }
    }
    .map_err(|e| format!("{name}: {e}"))?;
    std::fs::write(args.out, &exported.bytes)
        .map_err(|e| format!("{}: {e}", args.out.display()))?;
    // A feature that failed leaves its bodies out of the file: say so.
    Ok(match exported.line.status {
        arrix_doc::EvalStatus::Ok => ExitCode::SUCCESS,
        arrix_doc::EvalStatus::Failed => {
            eprintln!(
                "arrix export: {name}: some features failed; the file holds the bodies that evaluated"
            );
            ExitCode::from(1)
        }
    })
}

/// A plugin's exporter on a part or an assembly: its files go to `--out`,
/// one as that file and several into that directory (ADR-0037 §8).
fn plugin_export(
    args: &Args,
    name: &str,
    registry: &Registry,
    source: &Loaded,
    document: &Document,
) -> Result<ExitCode, String> {
    let id = args.exporter.expect("a plugin export names its exporter");
    let Some(exporter) = registry.exporter(id) else {
        let there: Vec<_> = registry
            .exporter_specs()
            .into_iter()
            .map(|e| e.id)
            .chain(
                ExportFormat::ALL
                    .iter()
                    .map(|f| format!("core.{}", f.extension())),
            )
            .collect();
        return Err(format!(
            "--exporter {id}: no such exporter; there are {}",
            there.join(", ")
        ));
    };
    let spec = exporter.spec();
    if !args.bodies.is_empty() {
        return Err("--body goes with --format".into());
    }
    let subject = match (spec.subject, args.assembly, args.part) {
        (SubjectKind::Assembly, Some(a), None) => Subject::Assembly(assembly_id(document, a)?),
        (SubjectKind::Part, None, Some(p)) => Subject::Part(part_id(document, p)?),
        (SubjectKind::Assembly, _, _) => {
            return Err(format!("{id} exports an assembly: give --assembly"));
        }
        (SubjectKind::Part, _, _) => return Err(format!("{id} exports a part: give --part")),
    };
    let (params, choices) = split_options(spec, args.options)?;
    let exported =
        arrix_doc::export_with_exporter(name, source, registry, id, subject, &params, &choices)
            .map_err(|e| format!("{name}: {e}"))?;
    write_files(args.out, &exported.files, args.force)?;
    Ok(match exported.line.status {
        arrix_doc::EvalStatus::Ok => ExitCode::SUCCESS,
        arrix_doc::EvalStatus::Failed => {
            eprintln!("arrix export: {name}: some features failed; the files hold what evaluated");
            ExitCode::from(1)
        }
    })
}

/// `--option name=value`, a choice of the exporter when it has one of that
/// name and a parameter otherwise (an unknown name is the evaluator's
/// `feature.unknown-field`).
#[allow(clippy::type_complexity)]
pub(crate) fn split_options(
    spec: &ExporterSpec,
    options: &[String],
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>), String> {
    let (mut params, mut choices) = (BTreeMap::new(), BTreeMap::new());
    for option in options {
        let (key, value) = option
            .split_once('=')
            .ok_or_else(|| format!("--option {option}: expected <name>=<value>"))?;
        let into = if spec.choices.iter().any(|c| c.name == key) {
            &mut choices
        } else {
            &mut params
        };
        if into.insert(key.to_owned(), value.to_owned()).is_some() {
            return Err(format!("--option {key}: given twice"));
        }
    }
    Ok((params, choices))
}

/// One file goes to `out`; several go into the directory `out`, created if
/// missing. Nothing is written when a file of the directory would be
/// overwritten and `force` is not given.
fn write_files(out: &Path, files: &[ExportFile], force: bool) -> Result<(), String> {
    let shown = |p: &Path| p.display().to_string();
    if let [file] = files
        && !file.name.contains('/')
    {
        return std::fs::write(out, &file.bytes).map_err(|e| format!("{}: {e}", shown(out)));
    }
    let targets: Vec<PathBuf> = files.iter().map(|f| out.join(&f.name)).collect();
    if !force && let Some(there) = targets.iter().find(|t| t.exists()) {
        return Err(format!(
            "{}: already there; --force overwrites",
            shown(there)
        ));
    }
    for (file, target) in files.iter().zip(&targets) {
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", shown(dir)))?;
        }
        std::fs::write(target, &file.bytes).map_err(|e| format!("{}: {e}", shown(target)))?;
    }
    Ok(())
}

/// A part by its id or its name.
pub(crate) fn part_id(document: &Document, spec: &str) -> Result<PartId, String> {
    if let Some(id) = spec
        .parse::<PartId>()
        .ok()
        .filter(|id| document.parts().contains_key(id))
    {
        return Ok(id);
    }
    let mut named = document
        .parts()
        .values()
        .filter(|p| p.name == spec)
        .map(|p| p.id);
    match (named.next(), named.next()) {
        (Some(id), None) => Ok(id),
        (Some(_), Some(_)) => Err(format!("--part {spec}: several are named so; use its id")),
        (None, _) => Err(format!("--part {spec}: no such part")),
    }
}

/// `<feature>.<slot>`, the feature by its id or its name in any part.
fn body_slot(document: &Document, spec: &str) -> Result<BodySlot, String> {
    let (feature, slot) = spec
        .rsplit_once('.')
        .ok_or_else(|| format!("--body {spec}: expected <feature>.<slot>"))?;
    let slot = SlotName::new(slot).map_err(|e| format!("--body {spec}: {e}"))?;
    let by_id = feature.parse::<FeatureId>().ok().filter(|id| {
        document
            .parts()
            .values()
            .any(|part| part.features.contains_key(id))
    });
    let mut named = document
        .parts()
        .values()
        .flat_map(|part| part.features.values())
        .filter(|f| f.name == feature)
        .map(|f| f.id);
    let id = match (by_id, named.next(), named.next()) {
        (Some(id), _, _) | (None, Some(id), None) => id,
        (None, Some(_), Some(_)) => {
            return Err(format!(
                "--body {spec}: {feature} names several features; use its id"
            ));
        }
        (None, None, _) => return Err(format!("--body {spec}: no such feature")),
    };
    Ok(BodySlot { feature: id, slot })
}

/// An assembly by its id or its name.
pub(crate) fn assembly_id(document: &Document, spec: &str) -> Result<AssemblyId, String> {
    if let Some(id) = spec
        .parse::<AssemblyId>()
        .ok()
        .filter(|id| document.assemblies().contains_key(id))
    {
        return Ok(id);
    }
    let mut named = document
        .assemblies()
        .values()
        .filter(|a| a.name == spec)
        .map(|a| a.id);
    match (named.next(), named.next()) {
        (Some(id), None) => Ok(id),
        (Some(_), Some(_)) => Err(format!(
            "--assembly {spec}: several are named so; use its id"
        )),
        (None, _) => Err(format!("--assembly {spec}: no such assembly")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrix_plugin_api::{ChoiceOption, ChoiceSpec};

    fn file(name: &str, bytes: &[u8]) -> ExportFile {
        ExportFile {
            name: name.into(),
            bytes: bytes.to_vec(),
        }
    }

    fn dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("arrix-export-unit");
        let dir = dir.join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.parent().unwrap()).unwrap();
        dir
    }

    fn spec() -> ExporterSpec {
        ExporterSpec {
            id: "t.x".into(),
            title: "x".into(),
            extension: "txt".into(),
            subject: SubjectKind::Assembly,
            params: Vec::new(),
            choices: vec![ChoiceSpec {
                name: "style".into(),
                title: "Style".into(),
                options: vec![ChoiceOption {
                    name: "a".into(),
                    title: "A".into(),
                }],
                default: Some("a".into()),
            }],
        }
    }

    #[test]
    fn one_file_goes_to_the_path_given() {
        let out = dir("one.txt");
        std::fs::create_dir_all(out.parent().unwrap()).unwrap();
        write_files(&out, &[file("main.txt", b"x")], false).unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"x");
    }

    #[test]
    fn several_files_go_into_a_directory_made_for_them() {
        let out = dir("many");
        let files = [file("a.urdf", b"a"), file("meshes/b.stl", b"b")];
        write_files(&out, &files, false).unwrap();
        assert_eq!(std::fs::read(out.join("a.urdf")).unwrap(), b"a");
        assert_eq!(std::fs::read(out.join("meshes/b.stl")).unwrap(), b"b");
    }

    #[test]
    fn a_file_already_there_stops_the_write_unless_forced() {
        let out = dir("again");
        let files = [file("a.urdf", b"new"), file("b.stl", b"new")];
        std::fs::create_dir_all(&out).unwrap();
        std::fs::write(out.join("b.stl"), b"old").unwrap();
        let err = write_files(&out, &files, false).unwrap_err();
        assert!(err.contains("b.stl") && err.contains("--force"), "{err}");
        assert!(!out.join("a.urdf").exists(), "nothing is written");
        write_files(&out, &files, true).unwrap();
        assert_eq!(std::fs::read(out.join("b.stl")).unwrap(), b"new");
    }

    #[test]
    fn an_option_is_a_choice_when_the_spec_has_one_of_that_name() {
        let given = ["style=a".to_owned(), "effort=2 N".to_owned()];
        let (params, choices) = split_options(&spec(), &given).unwrap();
        assert_eq!(choices["style"], "a");
        assert_eq!(params["effort"], "2 N");
        assert!(split_options(&spec(), &["style".to_owned()]).is_err());
        let twice = ["a=1".to_owned(), "a=2".to_owned()];
        assert!(split_options(&spec(), &twice).is_err());
    }
}
