//! The dependency DAG (docs/DATA-MODEL.md §The dependency DAG): derived
//! from the document, never stored. Its edges are every `Ref` in a record
//! and every parameter name in an expression; a feature also reads its
//! part, whose order and rollback place it.

#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use arrix_core::{
    Diagnostic, FeatureId, NameRoot, NameStep, ParamId, PartId, PersistentName, Ref, Severity,
};
use serde::{Deserialize, Serialize};

use crate::document::{Document, FeatureRecord};

/// A node of the DAG: what a command touches and the evaluator orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Node {
    Param(ParamId),
    Part(PartId),
    Feature(FeatureId),
}

impl Node {
    /// As a reference to highlight; a part has none.
    pub fn as_ref(&self) -> Option<Ref> {
        match *self {
            Node::Param(p) => Some(Ref::Param(p)),
            Node::Feature(f) => Some(Ref::Feature(f)),
            Node::Part(_) => None,
        }
    }
}

impl fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Node::Param(id) => write!(f, "parameter {id}"),
            Node::Part(id) => write!(f, "part {id}"),
            Node::Feature(id) => write!(f, "feature {id}"),
        }
    }
}

/// Why a document's references cannot form a DAG.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DagError {
    #[error(
        "feature {feature}'s `{field}` reads feature {target}, which is not before it in its part"
    )]
    Order {
        feature: FeatureId,
        field: String,
        target: FeatureId,
    },
    #[error("a cycle: {}", .0.iter().map(Node::to_string).collect::<Vec<_>>().join(" → "))]
    Cycle(Vec<Node>),
}

impl DagError {
    pub fn diagnostic(&self) -> Diagnostic {
        let (code, refs) = match self {
            DagError::Order {
                feature, target, ..
            } => (
                "dag.order",
                vec![Ref::Feature(*feature), Ref::Feature(*target)],
            ),
            DagError::Cycle(nodes) => {
                ("dag.cycle", nodes.iter().filter_map(Node::as_ref).collect())
            }
        };
        Diagnostic::new(Severity::Error, code, self.to_string())
            .expect("a well-formed code")
            .with_refs(refs)
    }
}

/// The DAG of one document, in a deterministic topological order:
/// parameters by id, then parts, then features by part and history
/// position, each as soon as what it reads is placed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dag {
    order: Vec<Node>,
    reads: BTreeMap<Node, BTreeSet<Node>>,
    readers: BTreeMap<Node, BTreeSet<Node>>,
}

/// The features a persistent name was made or changed by.
fn name_features(name: &PersistentName, out: &mut BTreeSet<FeatureId>) {
    out.insert(match name.root {
        NameRoot::Sweep { feature, .. }
        | NameRoot::Plugin { feature, .. }
        | NameRoot::Frozen { feature, .. } => feature,
    });
    for step in &name.chain {
        match step {
            NameStep::Modified { feature, .. } => {
                out.insert(*feature);
            }
            NameStep::Generated { feature, from } => {
                out.insert(*feature);
                from.iter().for_each(|n| name_features(n, out));
            }
        }
    }
}

/// The features a reference reads.
pub(crate) fn ref_features(r: &Ref) -> BTreeSet<FeatureId> {
    let mut out = BTreeSet::new();
    match r {
        Ref::Feature(f) | Ref::Slot { feature: f, .. } | Ref::Sketch { feature: f, .. } => {
            out.insert(*f);
        }
        Ref::Topo(name) => name_features(name, &mut out),
        Ref::Param(_) | Ref::Plugin { .. } => {}
    }
    out
}

/// What one feature reads, each with the field it is read by. A target
/// the document lacks is no edge: it is the evaluator's lost reference.
fn feature_reads(doc: &Document, record: &FeatureRecord) -> Vec<(String, Node)> {
    let mut out = Vec::new();
    for (field, expr) in &record.params {
        for name in expr.names() {
            if let Some((id, _)) = doc.param_by_name(name) {
                out.push((format!("params.{field}"), Node::Param(id)));
            }
        }
    }
    for (field, r) in &record.inputs {
        let field = format!("inputs.{field}");
        if let Ref::Param(p) = r
            && doc.params.contains_key(p)
        {
            out.push((field.clone(), Node::Param(*p)));
        }
        for f in ref_features(r) {
            if doc.feature(f).is_some() {
                out.push((field.clone(), Node::Feature(f)));
            }
        }
    }
    out
}

impl Dag {
    pub fn build(doc: &Document) -> Result<Dag, DagError> {
        let mut reads: BTreeMap<Node, BTreeSet<Node>> = BTreeMap::new();
        let mut rank: BTreeMap<Node, (u8, u64, usize)> = BTreeMap::new();
        for (id, p) in &doc.params {
            let node = Node::Param(*id);
            rank.insert(node, (0, id.0.0, 0));
            let names = p.expr.names();
            let edges = names.iter().filter_map(|n| doc.param_by_name(n));
            reads.insert(node, edges.map(|(id, _)| Node::Param(id)).collect());
        }
        for (pid, part) in &doc.parts {
            rank.insert(Node::Part(*pid), (1, pid.0.0, 0));
            reads.insert(Node::Part(*pid), BTreeSet::new());
            for (at, fid) in part.history.iter().enumerate() {
                let node = Node::Feature(*fid);
                rank.insert(node, (2, pid.0.0, at));
                let mut set = BTreeSet::from([Node::Part(*pid)]);
                for (field, target) in feature_reads(doc, &part.features[fid]) {
                    if let Node::Feature(t) = target
                        && let Some((tpart, tat, _)) = doc.feature(t)
                        && tpart.id == *pid
                        && tat >= at
                    {
                        return Err(DagError::Order {
                            feature: *fid,
                            field,
                            target: t,
                        });
                    }
                    set.insert(target);
                }
                reads.insert(node, set);
            }
        }
        let mut readers: BTreeMap<Node, BTreeSet<Node>> =
            reads.keys().map(|n| (*n, BTreeSet::new())).collect();
        for (n, rs) in &reads {
            for r in rs {
                readers.get_mut(r).expect("every read is a node").insert(*n);
            }
        }
        let order = topological(&reads, &readers, &rank)?;
        Ok(Dag {
            order,
            reads,
            readers,
        })
    }

    /// Every node, each after everything it reads.
    pub fn order(&self) -> &[Node] {
        &self.order
    }

    /// What `node` reads directly.
    pub fn reads(&self, node: Node) -> impl Iterator<Item = Node> + '_ {
        self.reads.get(&node).into_iter().flatten().copied()
    }

    /// What reads `node` directly.
    pub fn readers(&self, node: Node) -> impl Iterator<Item = Node> + '_ {
        self.readers.get(&node).into_iter().flatten().copied()
    }

    /// The touched nodes this DAG holds and everything downstream of them,
    /// in topological order: what the evaluator revisits.
    pub fn dirty(&self, touched: &BTreeSet<Node>) -> Vec<Node> {
        let mut seen = BTreeSet::new();
        let mut stack: Vec<Node> = touched
            .iter()
            .filter(|n| self.reads.contains_key(n))
            .copied()
            .collect();
        while let Some(n) = stack.pop() {
            if seen.insert(n) {
                stack.extend(self.readers(n));
            }
        }
        self.order
            .iter()
            .filter(|n| seen.contains(n))
            .copied()
            .collect()
    }
}

/// Kahn's algorithm, the ready node of least rank first; what is left over
/// holds a cycle, which is returned.
fn topological(
    reads: &BTreeMap<Node, BTreeSet<Node>>,
    readers: &BTreeMap<Node, BTreeSet<Node>>,
    rank: &BTreeMap<Node, (u8, u64, usize)>,
) -> Result<Vec<Node>, DagError> {
    let mut waiting: BTreeMap<Node, usize> = reads.iter().map(|(n, r)| (*n, r.len())).collect();
    let mut ready: BTreeSet<_> = waiting
        .iter()
        .filter(|(_, w)| **w == 0)
        .map(|(n, _)| (rank[n], *n))
        .collect();
    let mut order = Vec::with_capacity(reads.len());
    while let Some((_, n)) = ready.pop_first() {
        order.push(n);
        waiting.remove(&n);
        for r in &readers[&n] {
            let w = waiting.get_mut(r).expect("a reader is still waiting");
            *w -= 1;
            if *w == 0 {
                ready.insert((rank[r], *r));
            }
        }
    }
    if waiting.is_empty() {
        return Ok(order);
    }
    Err(DagError::Cycle(find_cycle(reads, &waiting)))
}

/// A cycle among the nodes left waiting: each of them reads another one,
/// so walking reads from the least must come back. Returned from its least
/// node, each node reading the next.
fn find_cycle(reads: &BTreeMap<Node, BTreeSet<Node>>, left: &BTreeMap<Node, usize>) -> Vec<Node> {
    let mut path = Vec::new();
    let mut at = *left.keys().next().expect("something is left");
    loop {
        if let Some(i) = path.iter().position(|n| *n == at) {
            let mut cycle: Vec<Node> = path.split_off(i);
            let least = (0..cycle.len())
                .min_by_key(|i| cycle[*i])
                .expect("non-empty");
            cycle.rotate_left(least);
            return cycle;
        }
        path.push(at);
        at = *reads[&at]
            .iter()
            .find(|r| left.contains_key(r))
            .expect("a waiting node reads a waiting node");
    }
}
