//! The DAG's assembly, component and joint nodes (ADR-0029 §10). A
//! component reads its part or nested assembly and the parameters of its
//! pose; a joint reads the components its anchors go through, the anchored
//! parts and the features that make up what its names resolve against (a
//! version across parts is the part's final state), and the parameters of
//! its expressions; an assembly reads its components and joints. An
//! assembly containing itself, at any depth, is a cycle.

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{AssemblyId, ComponentId, FeatureId, PartId, PersistentName};

use super::{Node, name_features};
use crate::assembly::{Anchor, Assembly, ComponentOf, Joint};
use crate::bodies;
use crate::document::{Document, Part};
use crate::expr::Expr;
use crate::registry::Registry;

type Reads = BTreeMap<Node, BTreeSet<Node>>;
type Rank = BTreeMap<Node, (u8, u64, usize)>;

/// The components a path goes through, from `assembly` down, and the part
/// it ends at. `None` when a component is missing, the path ends at a
/// nested assembly, or it goes on past a part: the evaluator's lost
/// reference, no edge here.
pub(crate) fn anchor_target(
    doc: &Document,
    assembly: AssemblyId,
    path: &[ComponentId],
) -> Option<(Vec<ComponentId>, PartId)> {
    let mut at = doc.assemblies.get(&assembly)?;
    let mut through = Vec::new();
    for (i, id) in path.iter().enumerate() {
        let component = at.components.get(id)?;
        through.push(*id);
        let last = i + 1 == path.len();
        match (component.of, last) {
            (ComponentOf::Part(part), true) => return Some((through, part)),
            (ComponentOf::Assembly(next), false) => at = doc.assemblies.get(&next)?,
            _ => return None,
        }
    }
    None
}

fn params_of<'e>(doc: &Document, exprs: impl IntoIterator<Item = &'e Expr>) -> BTreeSet<Node> {
    exprs
        .into_iter()
        .flat_map(Expr::names)
        .filter_map(|name| doc.param_by_name(name))
        .map(|(id, _)| Node::Param(id))
        .collect()
}

/// The features of `part` a name resolves against: those it was made or
/// changed by, and the features that touch the body slot it is a face of,
/// to the last one before the rollback: the part's final state of it.
fn name_reads(
    doc: &Document,
    registry: Option<&Registry>,
    part: &Part,
    name: &PersistentName,
) -> BTreeSet<FeatureId> {
    let mut out = BTreeSet::new();
    name_features(name, &mut out);
    if let Some(registry) = registry {
        let end = part
            .rollback
            .map_or(part.history.len(), |r| r.min(part.history.len()));
        let root = name.root.feature();
        if part.features.contains_key(&root) {
            for key in bodies::named_slots(registry, doc, root) {
                for slot in bodies::followed(registry, part, end, &key) {
                    out.insert(slot.feature);
                    out.extend(
                        bodies::touching(registry, part, end, &slot)
                            .iter()
                            .map(|(record, _)| record.id),
                    );
                }
            }
        }
    }
    out.retain(|f| part.features.contains_key(f));
    out
}

fn anchor_reads(
    doc: &Document,
    registry: Option<&Registry>,
    assembly: &Assembly,
    anchor: &Anchor,
) -> BTreeSet<Node> {
    let mut out = BTreeSet::new();
    let Some((through, part)) = anchor_target(doc, assembly.id, &anchor.path) else {
        // The path is lost, but the first component is still read: it is
        // what a repaired path edits.
        out.extend(anchor.path.first().map(|c| Node::Component(*c)));
        return out;
    };
    out.extend(through.into_iter().map(Node::Component));
    out.insert(Node::Part(part));
    if let Some(part) = doc.parts.get(&part) {
        for name in std::iter::once(&anchor.axis).chain(&anchor.origin) {
            out.extend(
                name_reads(doc, registry, part, name)
                    .into_iter()
                    .map(Node::Feature),
            );
        }
    }
    out
}

fn joint_exprs(joint: &Joint) -> impl Iterator<Item = &Expr> {
    [&joint.value, &joint.offset]
        .into_iter()
        .chain(joint.limits.iter().flatten())
}

/// Adds the assembly nodes to `reads` and `rank`. Every part and parameter
/// node is already there.
pub(super) fn add(doc: &Document, registry: Option<&Registry>, reads: &mut Reads, rank: &mut Rank) {
    for (aid, assembly) in &doc.assemblies {
        let mut assembly_reads = BTreeSet::new();
        for (cid, component) in &assembly.components {
            let node = Node::Component(*cid);
            rank.insert(node, (3, cid.0.0, 0));
            let mut set = BTreeSet::from([match component.of {
                ComponentOf::Part(p) => Node::Part(p),
                ComponentOf::Assembly(a) => Node::Assembly(a),
            }]);
            if let Some(pose) = &component.pose {
                set.extend(params_of(doc, pose.position.iter().chain(&pose.rpy)));
            }
            // A target the document lacks is no edge: `validate` refuses it.
            set.retain(|n| match n {
                Node::Part(p) => doc.parts.contains_key(p),
                Node::Assembly(a) => doc.assemblies.contains_key(a),
                _ => true,
            });
            reads.insert(node, set);
            assembly_reads.insert(node);
        }
        for (jid, joint) in &assembly.joints {
            let node = Node::Joint(*jid);
            rank.insert(node, (4, jid.0.0, 0));
            let mut set = params_of(doc, joint_exprs(joint));
            for anchor in [&joint.parent, &joint.child] {
                set.extend(anchor_reads(doc, registry, assembly, anchor));
            }
            reads.insert(node, set);
            assembly_reads.insert(node);
        }
        let node = Node::Assembly(*aid);
        rank.insert(node, (5, aid.0.0, 0));
        reads.insert(node, assembly_reads);
    }
}
