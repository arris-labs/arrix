//! The assembly and imported-file commands (ADR-0029 §9). Each returns its
//! inverse; what a deleted record leaves dangling is caught by the
//! document's validation at the end of the command or group, as
//! `assembly.dangling`.

use std::collections::BTreeSet;

use arrix_core::{AssemblyId, Id};

use super::{Command, CommandError};
use crate::assembly::Assembly;
use crate::dag::Node;
use crate::document::Document;
use crate::frozen::BlobRef;

pub(super) fn apply(
    doc: &mut Document,
    command: &Command,
    touched: &mut BTreeSet<Node>,
) -> Result<Command, CommandError> {
    match command {
        Command::AddFile { bytes } => Ok(add_file(doc, bytes)),
        Command::RemoveFile { blob } => remove_file(doc, blob),
        Command::AddAssembly { assembly } => add_assembly(doc, assembly, touched),
        Command::DeleteAssembly { assembly } => {
            let removed = doc
                .assemblies
                .remove(assembly)
                .ok_or(CommandError::NoAssembly(*assembly))?;
            touch_assembly(&removed, touched);
            Ok(Command::AddAssembly { assembly: removed })
        }
        Command::AddComponent {
            assembly,
            component,
        } => {
            component_id_free(doc, component.id.0)?;
            let a = assembly_mut(doc, *assembly)?;
            a.components.insert(component.id, (**component).clone());
            touched.extend([Node::Assembly(*assembly), Node::Component(component.id)]);
            Ok(Command::DeleteComponent {
                assembly: *assembly,
                component: component.id,
            })
        }
        Command::EditComponent {
            assembly,
            component,
        } => {
            let a = assembly_mut(doc, *assembly)?;
            let slot = a
                .components
                .get_mut(&component.id)
                .ok_or(CommandError::NoComponent {
                    assembly: *assembly,
                    component: component.id,
                })?;
            let old = std::mem::replace(slot, (**component).clone());
            touched.extend([Node::Assembly(*assembly), Node::Component(component.id)]);
            Ok(Command::EditComponent {
                assembly: *assembly,
                component: Box::new(old),
            })
        }
        Command::DeleteComponent {
            assembly,
            component,
        } => {
            let a = assembly_mut(doc, *assembly)?;
            let old = a
                .components
                .remove(component)
                .ok_or(CommandError::NoComponent {
                    assembly: *assembly,
                    component: *component,
                })?;
            touched.extend([Node::Assembly(*assembly), Node::Component(*component)]);
            Ok(Command::AddComponent {
                assembly: *assembly,
                component: Box::new(old),
            })
        }
        Command::AddJoint { assembly, joint } => {
            component_id_free(doc, joint.id.0)?;
            let a = assembly_mut(doc, *assembly)?;
            a.joints.insert(joint.id, (**joint).clone());
            touched.extend([Node::Assembly(*assembly), Node::Joint(joint.id)]);
            Ok(Command::DeleteJoint {
                assembly: *assembly,
                joint: joint.id,
            })
        }
        Command::EditJoint { assembly, joint } => {
            let a = assembly_mut(doc, *assembly)?;
            let slot = a.joints.get_mut(&joint.id).ok_or(CommandError::NoJoint {
                assembly: *assembly,
                joint: joint.id,
            })?;
            let old = std::mem::replace(slot, (**joint).clone());
            touched.extend([Node::Assembly(*assembly), Node::Joint(joint.id)]);
            Ok(Command::EditJoint {
                assembly: *assembly,
                joint: Box::new(old),
            })
        }
        Command::DeleteJoint { assembly, joint } => {
            let a = assembly_mut(doc, *assembly)?;
            let old = a.joints.remove(joint).ok_or(CommandError::NoJoint {
                assembly: *assembly,
                joint: *joint,
            })?;
            touched.extend([Node::Assembly(*assembly), Node::Joint(*joint)]);
            Ok(Command::AddJoint {
                assembly: *assembly,
                joint: Box::new(old),
            })
        }
        _ => unreachable!("only the assembly commands are routed here"),
    }
}

fn assembly_mut(doc: &mut Document, id: AssemblyId) -> Result<&mut Assembly, CommandError> {
    doc.assemblies
        .get_mut(&id)
        .ok_or(CommandError::NoAssembly(id))
}

fn touch_assembly(assembly: &Assembly, touched: &mut BTreeSet<Node>) {
    touched.insert(Node::Assembly(assembly.id));
    touched.extend(assembly.components.keys().map(|c| Node::Component(*c)));
    touched.extend(assembly.joints.keys().map(|j| Node::Joint(*j)));
}

/// Whether some component or joint of any assembly already has `id`.
fn component_id_free(doc: &Document, id: Id) -> Result<(), CommandError> {
    let used = doc
        .assemblies
        .values()
        .any(|a| a.components.keys().any(|c| c.0 == id) || a.joints.keys().any(|j| j.0 == id));
    if used {
        return Err(CommandError::IdTaken(id));
    }
    Ok(())
}

fn add_assembly(
    doc: &mut Document,
    assembly: &Assembly,
    touched: &mut BTreeSet<Node>,
) -> Result<Command, CommandError> {
    if doc.assemblies.contains_key(&assembly.id) {
        return Err(CommandError::IdTaken(assembly.id.0));
    }
    for id in assembly
        .components
        .keys()
        .map(|c| c.0)
        .chain(assembly.joints.keys().map(|j| j.0))
    {
        component_id_free(doc, id)?;
    }
    doc.assemblies.insert(assembly.id, assembly.clone());
    touch_assembly(assembly, touched);
    Ok(Command::DeleteAssembly {
        assembly: assembly.id,
    })
}

fn add_file(doc: &mut Document, bytes: &[u8]) -> Command {
    let blob = BlobRef::of(bytes, "step");
    if doc.blobs.contains_key(&blob) {
        return Command::Group {
            label: "file already held".into(),
            commands: Vec::new(),
        };
    }
    doc.blobs.insert(blob.clone(), bytes.to_vec());
    Command::RemoveFile { blob }
}

fn remove_file(doc: &mut Document, blob: &BlobRef) -> Result<Command, CommandError> {
    // Only what `AddFile` holds: its inverse could not name another kind.
    if !blob.path().ends_with(".step") {
        return Err(CommandError::NoFile(blob.clone()));
    }
    let named_by = doc
        .parts
        .values()
        .flat_map(|p| p.features.values())
        .find(|r| r.blobs().any(|b| b == blob));
    if let Some(record) = named_by {
        return Err(CommandError::FileInUse {
            blob: blob.clone(),
            feature: record.id,
        });
    }
    let bytes = doc
        .blobs
        .remove(blob)
        .ok_or_else(|| CommandError::NoFile(blob.clone()))?;
    Ok(Command::AddFile { bytes })
}
