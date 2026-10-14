//! A document's parts and assemblies, and an exporter's or command's options,
//! as the command line and a manifest's fixtures name them.

use std::collections::BTreeMap;

use arrix_core::{AssemblyId, PartId};
use arrix_doc::Document;

/// `--option name=value`, a choice of the exporter when it has one of that
/// name and a parameter otherwise (an unknown name is the evaluator's
/// `feature.unknown-field`).
#[allow(clippy::type_complexity)]
pub fn split_options(
    choices_of_form: &[arrix_plugin_api::ChoiceSpec],
    options: &[String],
) -> Result<(BTreeMap<String, String>, BTreeMap<String, String>), String> {
    let (mut params, mut choices) = (BTreeMap::new(), BTreeMap::new());
    for option in options {
        let (key, value) = option
            .split_once('=')
            .ok_or_else(|| format!("--option {option}: expected <name>=<value>"))?;
        let into = if choices_of_form.iter().any(|c| c.name == key) {
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

/// A part by its id or its name.
pub fn part_id(document: &Document, spec: &str) -> Result<PartId, String> {
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

/// An assembly by its id or its name.
pub fn assembly_id(document: &Document, spec: &str) -> Result<AssemblyId, String> {
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
