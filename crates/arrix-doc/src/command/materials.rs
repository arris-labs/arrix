//! The material commands (ADR-0031 §3). Each returns its inverse; a
//! material a part still names cannot be deleted (`material.in-use`), and
//! a part left naming one that is gone is caught by validation.

use std::collections::BTreeSet;

use arrix_core::MaterialId;

use super::{Command, CommandError};
use crate::dag::Node;
use crate::document::Document;

pub(super) fn apply(
    doc: &mut Document,
    command: &Command,
    touched: &mut BTreeSet<Node>,
) -> Result<Command, CommandError> {
    match command {
        Command::AddMaterial { material } => {
            if doc.materials.contains_key(&material.id) {
                return Err(CommandError::IdTaken(material.id.0));
            }
            doc.materials.insert(material.id, material.clone());
            touched.insert(Node::Material(material.id));
            Ok(Command::DeleteMaterial {
                material: material.id,
            })
        }
        Command::EditMaterial { material } => {
            let slot = doc
                .materials
                .get_mut(&material.id)
                .ok_or(CommandError::NoMaterial(material.id))?;
            let old = std::mem::replace(slot, material.clone());
            touched.insert(Node::Material(material.id));
            Ok(Command::EditMaterial { material: old })
        }
        Command::DeleteMaterial { material } => delete(doc, *material, touched),
        Command::AssignMaterial {
            part,
            body,
            material,
        } => {
            if let Some(m) = material
                && !doc.materials.contains_key(m)
            {
                return Err(CommandError::NoMaterial(*m));
            }
            let p = doc.parts.get_mut(part).ok_or(CommandError::NoPart(*part))?;
            let old = match body {
                None => std::mem::replace(&mut p.material, *material),
                Some(body) => match material {
                    Some(m) => p.body_materials.insert(body.clone(), *m),
                    None => p.body_materials.remove(body),
                },
            };
            touched.insert(Node::Part(*part));
            Ok(Command::AssignMaterial {
                part: *part,
                body: body.clone(),
                material: old,
            })
        }
        _ => unreachable!("only the material commands are routed here"),
    }
}

fn delete(
    doc: &mut Document,
    id: MaterialId,
    touched: &mut BTreeSet<Node>,
) -> Result<Command, CommandError> {
    if !doc.materials.contains_key(&id) {
        return Err(CommandError::NoMaterial(id));
    }
    let user = doc
        .parts
        .values()
        .find(|p| p.material == Some(id) || p.body_materials.values().any(|m| *m == id));
    if let Some(part) = user {
        return Err(CommandError::MaterialInUse {
            material: id,
            part: part.id,
        });
    }
    let old = doc.materials.remove(&id).expect("checked above");
    touched.insert(Node::Material(id));
    Ok(Command::AddMaterial { material: old })
}
