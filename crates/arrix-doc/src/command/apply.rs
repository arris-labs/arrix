//! Each command on a working copy, returning its inverse. `apply` validates
//! the copy once at the end, so a group may pass through a state that is
//! only whole again when it finishes.

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{FeatureId, PartId};

use super::{Command, CommandError, FeatureEdit};
use crate::dag::Node;
use crate::document::{Document, FeatureRecord, Part};

pub(super) fn apply_mut(
    doc: &mut Document,
    command: &Command,
    touched: &mut BTreeSet<Node>,
) -> Result<Command, CommandError> {
    Ok(match command {
        Command::AddParam { param, record } => {
            if doc.params.contains_key(param) {
                return Err(CommandError::IdTaken(param.0));
            }
            doc.params.insert(*param, record.clone());
            touched.insert(Node::Param(*param));
            Command::DeleteParam { param: *param }
        }
        Command::SetParam { param, expr } => {
            let p = doc
                .params
                .get_mut(param)
                .ok_or(CommandError::NoParam(*param))?;
            let old = std::mem::replace(&mut p.expr, expr.clone());
            touched.insert(Node::Param(*param));
            Command::SetParam {
                param: *param,
                expr: old,
            }
        }
        Command::DeleteParam { param } => {
            let record = doc
                .params
                .remove(param)
                .ok_or(CommandError::NoParam(*param))?;
            touched.insert(Node::Param(*param));
            Command::AddParam {
                param: *param,
                record,
            }
        }
        Command::AddPart { part } => add_part(doc, part, touched)?,
        Command::DeletePart { part } => {
            let part = doc.parts.remove(part).ok_or(CommandError::NoPart(*part))?;
            touch_part(&part, touched);
            Command::AddPart { part }
        }
        Command::AddFeature { part, at, record } => add_feature(doc, *part, *at, record, touched)?,
        Command::EditFeature { feature, edit } => {
            let record = feature_mut(doc, *feature)?;
            let inverse = edit_feature(record, edit);
            touched.insert(Node::Feature(*feature));
            Command::EditFeature {
                feature: *feature,
                edit: inverse,
            }
        }
        Command::DeleteFeature { feature } => delete_feature(doc, *feature, touched)?,
        Command::ReorderFeature { feature, to } => {
            let part = part_of_mut(doc, *feature)?;
            let from = position(part, *feature)?;
            if *to >= part.history.len() {
                return Err(CommandError::Index {
                    at: *to,
                    len: part.history.len(),
                });
            }
            let f = part.history.remove(from);
            part.history.insert(*to, f);
            touched.extend([Node::Part(part.id), Node::Feature(*feature)]);
            Command::ReorderFeature {
                feature: *feature,
                to: from,
            }
        }
        Command::SetRollback { part, at } => {
            let p = doc.parts.get_mut(part).ok_or(CommandError::NoPart(*part))?;
            let old = std::mem::replace(&mut p.rollback, *at);
            touched.insert(Node::Part(*part));
            Command::SetRollback {
                part: *part,
                at: old,
            }
        }
        Command::Group { label, commands } => {
            let mut inverses = Vec::with_capacity(commands.len());
            for c in commands {
                inverses.push(apply_mut(doc, c, touched)?);
            }
            inverses.reverse();
            Command::Group {
                label: label.clone(),
                commands: inverses,
            }
        }
    })
}

fn touch_part(part: &Part, touched: &mut BTreeSet<Node>) {
    touched.insert(Node::Part(part.id));
    touched.extend(part.history.iter().map(|f| Node::Feature(*f)));
}

fn taken(doc: &Document, feature: FeatureId) -> Result<(), CommandError> {
    if doc
        .parts
        .values()
        .any(|p| p.features.contains_key(&feature))
    {
        return Err(CommandError::IdTaken(feature.0));
    }
    Ok(())
}

fn add_part(
    doc: &mut Document,
    part: &Part,
    touched: &mut BTreeSet<Node>,
) -> Result<Command, CommandError> {
    if doc.parts.contains_key(&part.id) {
        return Err(CommandError::IdTaken(part.id.0));
    }
    for f in part.features.keys() {
        taken(doc, *f)?;
    }
    doc.parts.insert(part.id, part.clone());
    touch_part(part, touched);
    Ok(Command::DeletePart { part: part.id })
}

fn add_feature(
    doc: &mut Document,
    part: PartId,
    at: usize,
    record: &FeatureRecord,
    touched: &mut BTreeSet<Node>,
) -> Result<Command, CommandError> {
    taken(doc, record.id)?;
    let p = doc.parts.get_mut(&part).ok_or(CommandError::NoPart(part))?;
    if at > p.history.len() {
        return Err(CommandError::Index {
            at,
            len: p.history.len(),
        });
    }
    p.history.insert(at, record.id);
    p.features.insert(record.id, record.clone());
    if let Some(r) = p.rollback.as_mut()
        && at < *r
    {
        *r += 1;
    }
    touched.extend([Node::Part(part), Node::Feature(record.id)]);
    Ok(Command::DeleteFeature { feature: record.id })
}

fn delete_feature(
    doc: &mut Document,
    feature: FeatureId,
    touched: &mut BTreeSet<Node>,
) -> Result<Command, CommandError> {
    let p = part_of_mut(doc, feature)?;
    let at = position(p, feature)?;
    p.history.remove(at);
    let record = p
        .features
        .remove(&feature)
        .ok_or(CommandError::NoFeature(feature))?;
    let before = p.rollback;
    if let Some(r) = p.rollback.as_mut()
        && at < *r
    {
        *r -= 1;
    }
    touched.extend([Node::Part(p.id), Node::Feature(feature)]);
    let add = Command::AddFeature {
        part: p.id,
        at,
        record,
    };
    // Re-inserting moves the index only when `at` is before it, so deleting
    // the last feature before the rollback index needs the index restored.
    let after_add = p.rollback.map(|r| if at < r { r + 1 } else { r });
    if after_add == before {
        return Ok(add);
    }
    Ok(Command::Group {
        label: "restore a feature".into(),
        commands: vec![
            add,
            Command::SetRollback {
                part: p.id,
                at: before,
            },
        ],
    })
}

fn edit_feature(record: &mut FeatureRecord, edit: &FeatureEdit) -> FeatureEdit {
    let mut inverse = FeatureEdit::default();
    if let Some(name) = &edit.name {
        inverse.name = Some(std::mem::replace(&mut record.name, name.clone()));
    }
    if let Some(s) = edit.suppressed {
        inverse.suppressed = Some(std::mem::replace(&mut record.suppressed, s));
    }
    inverse.params = swap_entries(&mut record.params, &edit.params);
    inverse.inputs = swap_entries(&mut record.inputs, &edit.inputs);
    inverse
}

/// Sets or removes each entry, returning what each was before.
fn swap_entries<V: Clone>(
    map: &mut BTreeMap<String, V>,
    edits: &BTreeMap<String, Option<V>>,
) -> BTreeMap<String, Option<V>> {
    edits
        .iter()
        .map(|(k, v)| {
            let old = match v {
                Some(v) => map.insert(k.clone(), v.clone()),
                None => map.remove(k),
            };
            (k.clone(), old)
        })
        .collect()
}

fn part_of_mut(doc: &mut Document, feature: FeatureId) -> Result<&mut Part, CommandError> {
    doc.parts
        .values_mut()
        .find(|p| p.features.contains_key(&feature))
        .ok_or(CommandError::NoFeature(feature))
}

fn feature_mut(doc: &mut Document, feature: FeatureId) -> Result<&mut FeatureRecord, CommandError> {
    part_of_mut(doc, feature)?
        .features
        .get_mut(&feature)
        .ok_or(CommandError::NoFeature(feature))
}

/// Where a feature is in its part's history. A part added mid-group with
/// a history that omits it is refused here rather than trusted.
fn position(part: &Part, feature: FeatureId) -> Result<usize, CommandError> {
    part.history
        .iter()
        .position(|f| *f == feature)
        .ok_or(CommandError::NoFeature(feature))
}
