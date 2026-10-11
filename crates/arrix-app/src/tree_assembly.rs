//! The tree card's assemblies: each one's components, nested, with how each
//! is placed, and its joints with how each solved (ADR-0030, plans/c2-m2
//! step 5), from the replica's document and the `Assembled` outcomes it
//! has heard.

use arrix_core::{AssemblyId, ComponentId};
use arrix_doc::{
    Assembly, AssemblyOutcome, Component, ComponentOf, Document, JointKind, JointOutcome, Replica,
};
use arrix_ui::{JointState, Placement, TreeAssembly, TreeComponent, TreeJoint};

use crate::context::Context;

/// A nested listing stops here; a document's assemblies do not contain
/// themselves, so this only bounds what a corrupt one could ask for.
const MAX_DEPTH: usize = 16;

/// What the tree has selected: a component of an assembly, by its path.
pub type ComponentSelection<'a> = Option<&'a (AssemblyId, Vec<ComponentId>)>;

pub fn tree_assemblies(
    replica: &Replica,
    context: Option<Context>,
    selected: ComponentSelection<'_>,
) -> Vec<TreeAssembly> {
    let doc = replica.document();
    doc.assemblies()
        .values()
        .map(|a| TreeAssembly {
            id: a.id.0.to_string(),
            name: a.name.clone(),
            current: context == Some(Context::Assembly(a.id)),
            components: mark(components(replica, doc, a, &[], 0), a.id, selected),
            joints: joints(replica, a),
        })
        .collect()
}

fn components(
    replica: &Replica,
    doc: &Document,
    assembly: &Assembly,
    above: &[ComponentId],
    depth: usize,
) -> Vec<TreeComponent> {
    let outcome = replica.assembly(assembly.id).map(|(_, o)| o);
    assembly
        .components
        .values()
        .map(|c| {
            let mut path = above.to_vec();
            path.push(c.id);
            let (placement, joint) = placement(assembly, outcome, c);
            let (of, children) = match c.of {
                ComponentOf::Part(p) => (
                    doc.parts()
                        .get(&p)
                        .map_or_else(|| "?".to_owned(), |p| p.name.clone()),
                    Vec::new(),
                ),
                ComponentOf::Assembly(a) => match doc.assemblies().get(&a) {
                    Some(inner) if depth < MAX_DEPTH => (
                        inner.name.clone(),
                        components(replica, doc, inner, &[], depth + 1)
                            .into_iter()
                            .map(|child| nest(child, &path))
                            .collect(),
                    ),
                    Some(inner) => (inner.name.clone(), Vec::new()),
                    None => ("?".to_owned(), Vec::new()),
                },
            };
            TreeComponent {
                path: path.iter().map(|c| c.0.to_string()).collect(),
                name: c.name.clone(),
                of,
                placement,
                joint,
                selected: false,
                children,
            }
        })
        .collect()
}

/// Sets `selected` on the row `selected` names, in the listing of
/// `assembly`, whatever its depth.
fn mark(
    mut rows: Vec<TreeComponent>,
    assembly: AssemblyId,
    selected: ComponentSelection<'_>,
) -> Vec<TreeComponent> {
    let Some((_, path)) = selected.filter(|(a, _)| *a == assembly) else {
        return rows;
    };
    let path: Vec<String> = path.iter().map(|c| c.0.to_string()).collect();
    fn walk(rows: &mut [TreeComponent], path: &[String]) {
        for row in rows {
            row.selected = row.path == path;
            walk(&mut row.children, path);
        }
    }
    walk(&mut rows, &path);
    rows
}

/// A nested assembly's component, its path made from the component that
/// places the assembly.
fn nest(mut child: TreeComponent, above: &[ComponentId]) -> TreeComponent {
    let mut path: Vec<String> = above.iter().map(|c| c.0.to_string()).collect();
    path.append(&mut child.path);
    child.path = path;
    child.children = child.children.into_iter().map(|c| nest(c, above)).collect();
    child
}

/// How `component` is placed in `assembly`, and by which joint: its pose
/// grounds it; else the first joint of it that solved places it; else one
/// that closed a loop; else nothing does.
fn placement(
    assembly: &Assembly,
    outcome: Option<&AssemblyOutcome>,
    component: &Component,
) -> (Placement, Option<String>) {
    if component.pose.is_some() {
        return (Placement::Grounded, None);
    }
    let Some(outcome) = outcome else {
        return (Placement::Pending, None);
    };
    let placing = || {
        assembly
            .joints
            .values()
            .filter(|j| j.child.path.first() == Some(&component.id))
    };
    if let Some(j) =
        placing().find(|j| matches!(outcome.joints.get(&j.id), Some(JointOutcome::Ok { .. })))
    {
        return (Placement::ByJoint, Some(j.name.clone()));
    }
    let looped = placing().any(|j| {
        matches!(
            outcome.joints.get(&j.id),
            Some(JointOutcome::Failed { diagnostic }) if diagnostic.code.as_str() == "assembly.loop"
        )
    });
    (
        if looped {
            Placement::Loop
        } else {
            Placement::Unplaced
        },
        None,
    )
}

fn joints(replica: &Replica, assembly: &Assembly) -> Vec<TreeJoint> {
    let outcome = replica.assembly(assembly.id).map(|(_, o)| o);
    assembly
        .joints
        .values()
        .map(|j| {
            let (state, note) = match outcome.and_then(|o| o.joints.get(&j.id)) {
                None => (JointState::Evaluating, None),
                Some(JointOutcome::Ok { diagnostics, .. }) => {
                    match diagnostics
                        .iter()
                        .find(|d| d.code.as_str() == "assembly.limit")
                    {
                        Some(d) => (JointState::Limit, Some(d.message.clone())),
                        None => (JointState::Ok, None),
                    }
                }
                Some(JointOutcome::Failed { diagnostic }) => {
                    let state = match diagnostic.code.as_str() {
                        "ref.lost" => JointState::Lost,
                        "assembly.loop" => JointState::Loop,
                        _ => JointState::Failed,
                    };
                    (state, Some(diagnostic.message.clone()))
                }
            };
            TreeJoint {
                id: j.id.0.to_string(),
                name: j.name.clone(),
                kind: match j.kind {
                    JointKind::Fixed => "fixed",
                    JointKind::Revolute => "revolute",
                    JointKind::Prismatic => "prismatic",
                }
                .to_owned(),
                state,
                note,
            }
        })
        .collect()
}
