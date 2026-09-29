use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{FeatureId, Id, ParamId, PartId, QuantityKind, Ref};
use arrix_sketch::{Constraint, Draft};
use proptest::prelude::*;

use super::*;
use crate::document::{Document, FeatureRecord, FeatureTypeId, Param, Part};
use crate::expr::Expr;

fn fid(n: u64) -> FeatureId {
    FeatureId(Id(n))
}

fn record(id: FeatureId, inputs: Vec<Ref>, exprs: &[&str]) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new("core.datum-plane").unwrap(),
        type_version: 1,
        name: format!("f{}", id.0.0),
        params: exprs
            .iter()
            .enumerate()
            .map(|(i, e)| (format!("p{i}"), Expr::parse(e).unwrap()))
            .collect(),
        choices: BTreeMap::new(),
        inputs: inputs
            .into_iter()
            .enumerate()
            .map(|(i, r)| (format!("in{i}"), r))
            .collect(),
        suppressed: false,
        sketch: None,
        frozen: None,
    }
}

fn push_feature(doc: &mut Document, part: PartId, rec: FeatureRecord) {
    let p = doc.parts.entry(part).or_insert_with(|| Part {
        id: part,
        name: format!("part{}", part.0.0),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
    });
    p.history.push(rec.id);
    p.features.insert(rec.id, rec);
}

fn param(doc: &mut Document, id: u64, name: &str, expr: &str) -> ParamId {
    let pid = ParamId(Id(id));
    doc.params.insert(
        pid,
        Param {
            name: name.into(),
            kind: QuantityKind::Length,
            expr: Expr::parse(expr).unwrap(),
        },
    );
    pid
}

fn topo(feature: u64) -> Ref {
    Ref::Topo(
        format!("face:sweep.{}.end-cap", Id(feature))
            .parse()
            .unwrap(),
    )
}

/// The acceptance scenario's shape: `w` and `m`, a lower plane, the gear on
/// it reading both, an upper plane on the gear's top face.
fn scenario() -> (Document, [Node; 7]) {
    let mut doc = Document::default();
    let m = param(&mut doc, 1, "m", "1 mm");
    let w = param(&mut doc, 2, "w", "8 * m");
    let part = PartId(Id(10));
    push_feature(&mut doc, part, record(fid(20), vec![], &["10 mm"]));
    push_feature(
        &mut doc,
        part,
        record(fid(21), vec![Ref::Feature(fid(20))], &["w", "m"]),
    );
    push_feature(&mut doc, part, record(fid(22), vec![topo(21)], &["2 mm"]));
    let nodes = [
        Node::Param(m),
        Node::Param(w),
        Node::Part(part),
        Node::Feature(fid(20)),
        Node::Feature(fid(21)),
        Node::Feature(fid(22)),
        Node::Param(ParamId(Id(99))),
    ];
    (doc, nodes)
}

#[test]
fn orders_the_scenario_and_finds_what_is_dirty() {
    let (doc, [m, w, part, lower, gear, upper, missing]) = scenario();
    let dag = Dag::build(&doc).unwrap();
    assert_eq!(dag.order(), [m, w, part, lower, gear, upper]);
    assert_eq!(dag.reads(gear).collect::<Vec<_>>(), [m, w, part, lower]);
    assert_eq!(dag.readers(gear).collect::<Vec<_>>(), [upper]);
    assert_eq!(dag.dirty(&BTreeSet::from([w])), [w, gear, upper]);
    assert_eq!(dag.dirty(&BTreeSet::from([m])), [m, w, gear, upper]);
    assert_eq!(
        dag.dirty(&BTreeSet::from([part])),
        [part, lower, gear, upper]
    );
    assert_eq!(dag.dirty(&BTreeSet::from([missing])), []);
    assert!(doc.validate().is_ok());
}

#[test]
fn a_sketch_dimension_reads_the_parameters_its_expression_names() {
    let mut doc = Document::default();
    let w = param(&mut doc, 1, "w", "40 mm");
    let h = param(&mut doc, 2, "h", "w / 2");
    param(&mut doc, 3, "unread", "1 mm");
    let part = PartId(Id(10));
    let mut d = Draft::seeded(1);
    let (a, b, _) = d.add_line(0.0, 0.0, 0.02, 0.0);
    let dim = d.add_constraint(Constraint::Distance { a, b, value: 0.02 });
    d.set_constraint_expr(dim, Some("h + 1 mm".into()));
    let mut rec = record(fid(20), vec![], &[]);
    rec.sketch = Some(Box::new(d.sketch));
    push_feature(&mut doc, part, rec);

    let dag = doc.validate().unwrap();
    let sketch = Node::Feature(fid(20));
    assert_eq!(
        dag.reads(sketch).collect::<Vec<_>>(),
        [Node::Param(h), Node::Part(part)]
    );
    assert_eq!(
        dag.dirty(&BTreeSet::from([Node::Param(w)])),
        [Node::Param(w), Node::Param(h), sketch]
    );
}

#[test]
fn refuses_a_reference_forward_in_history() {
    let (mut doc, ..) = scenario();
    let part = PartId(Id(10));
    let lower = doc
        .parts
        .get_mut(&part)
        .unwrap()
        .features
        .get_mut(&fid(20))
        .unwrap();
    lower.inputs.insert("plane".into(), topo(22));
    let err = Dag::build(&doc).unwrap_err();
    assert_eq!(
        err,
        DagError::Order {
            feature: fid(20),
            field: "inputs.plane".into(),
            target: fid(22)
        }
    );
    let d = err.diagnostic();
    assert_eq!(d.code.as_str(), "dag.order");
    assert_eq!(d.refs, [Ref::Feature(fid(20)), Ref::Feature(fid(22))]);
    let lower = doc
        .parts
        .get_mut(&part)
        .unwrap()
        .features
        .get_mut(&fid(20))
        .unwrap();
    lower.inputs.insert("plane".into(), Ref::Feature(fid(20)));
    assert!(matches!(Dag::build(&doc), Err(DagError::Order { target, .. }) if target == fid(20)));
}

#[test]
fn refuses_a_cycle_naming_its_nodes() {
    let mut doc = Document::default();
    let a = param(&mut doc, 1, "a", "b + 1 mm");
    let b = param(&mut doc, 2, "b", "c");
    let c = param(&mut doc, 3, "c", "a * 2");
    param(&mut doc, 4, "d", "a");
    let err = Dag::build(&doc).unwrap_err();
    let cycle = [Node::Param(a), Node::Param(b), Node::Param(c)];
    assert_eq!(err, DagError::Cycle(cycle.to_vec()));
    let d = err.diagnostic();
    assert_eq!(d.code.as_str(), "dag.cycle");
    assert_eq!(d.refs.len(), 3);

    // Across parts: each part's feature reads the other's.
    let mut doc = Document::default();
    push_feature(&mut doc, PartId(Id(1)), record(fid(5), vec![topo(6)], &[]));
    push_feature(
        &mut doc,
        PartId(Id(2)),
        record(fid(6), vec![Ref::Feature(fid(5))], &[]),
    );
    let err = Dag::build(&doc).unwrap_err();
    assert_eq!(
        err,
        DagError::Cycle(vec![Node::Feature(fid(5)), Node::Feature(fid(6))])
    );
}

#[test]
fn validates_the_rest_of_the_invariants() {
    let (doc, ..) = scenario();
    let mut dup = doc.clone();
    param(&mut dup, 3, "w", "1 mm");
    assert!(matches!(dup.validate(), Err(crate::document::Invalid::DuplicateParam(n)) if n == "w"));
    let mut bad = doc.clone();
    param(&mut bad, 3, "2w", "1 mm");
    assert!(bad.validate().is_err());
    let mut twin = doc.clone();
    let part = twin.parts.get_mut(&PartId(Id(10))).unwrap();
    part.features.get_mut(&fid(22)).unwrap().name = "f21".into();
    assert!(twin.validate().is_err());
    let mut roll = doc.clone();
    roll.parts.get_mut(&PartId(Id(10))).unwrap().rollback = Some(4);
    assert!(roll.validate().is_err());
    let mut hist = doc;
    hist.parts.get_mut(&PartId(Id(10))).unwrap().history.pop();
    assert!(hist.validate().is_err());
}

/// One random edit: a parameter reading others by name, or a feature in
/// one of two parts reading earlier or later features and parameters.
#[derive(Clone, Debug)]
enum Edit {
    Param {
        reads: Vec<u8>,
    },
    Feature {
        part: u8,
        feature_reads: Vec<(u8, bool)>,
        param_reads: Vec<u8>,
    },
    Delete(u8),
}

fn arb_edit() -> impl Strategy<Value = Edit> {
    let small = || proptest::collection::vec(0u8..12, 0..3);
    prop_oneof![
        small().prop_map(|reads| Edit::Param { reads }),
        (
            0u8..2,
            proptest::collection::vec((0u8..12, any::<bool>()), 0..3),
            small()
        )
            .prop_map(|(part, feature_reads, param_reads)| Edit::Feature {
                part,
                feature_reads,
                param_reads
            }),
        (0u8..12).prop_map(Edit::Delete),
    ]
}

fn apply(doc: &mut Document, n: u64, edit: &Edit) {
    match edit {
        Edit::Param { reads } => {
            let expr = std::iter::once("1 mm".to_owned())
                .chain(reads.iter().map(|r| format!("p{r}")))
                .collect::<Vec<_>>()
                .join(" + ");
            param(doc, n, &format!("p{}", doc.params.len()), &expr);
        }
        Edit::Feature {
            part,
            feature_reads,
            param_reads,
        } => {
            let inputs = feature_reads
                .iter()
                .map(|(t, by_name)| {
                    let t = 1000 + u64::from(*t);
                    if *by_name {
                        topo(t)
                    } else {
                        Ref::Feature(fid(t))
                    }
                })
                .collect();
            let exprs: Vec<String> = param_reads.iter().map(|r| format!("p{r}")).collect();
            let exprs: Vec<&str> = exprs.iter().map(String::as_str).collect();
            let id = fid(1000 + n);
            push_feature(
                doc,
                PartId(Id(u64::from(*part) + 1)),
                record(id, inputs, &exprs),
            );
        }
        Edit::Delete(t) => {
            for p in doc.parts.values_mut() {
                let id = fid(1000 + u64::from(*t));
                p.history.retain(|f| *f != id);
                p.features.remove(&id);
            }
        }
    }
}

/// An oracle: edges read straight off the generated records.
fn oracle_reads(doc: &Document) -> BTreeMap<Node, BTreeSet<Node>> {
    let by_name = |n: &str| doc.param_by_name(n).map(|(id, _)| Node::Param(id));
    let exists = |f: FeatureId| doc.feature(f).map(|_| Node::Feature(f));
    let mut out = BTreeMap::new();
    for (id, p) in &doc.params {
        out.insert(
            Node::Param(*id),
            p.expr.names().into_iter().filter_map(by_name).collect(),
        );
    }
    for (pid, part) in &doc.parts {
        out.insert(Node::Part(*pid), BTreeSet::new());
        for rec in part.features.values() {
            let mut s = BTreeSet::from([Node::Part(*pid)]);
            s.extend(
                rec.params
                    .values()
                    .flat_map(|e| e.names())
                    .filter_map(by_name),
            );
            for r in rec.inputs.values() {
                let t = match r {
                    Ref::Feature(f) => *f,
                    Ref::Topo(n) => match n.root {
                        arrix_core::NameRoot::Sweep { feature, .. } => feature,
                        _ => unreachable!(),
                    },
                    _ => unreachable!(),
                };
                s.extend(exists(t));
            }
            out.insert(Node::Feature(rec.id), s);
        }
    }
    out
}

fn reachable(reads: &BTreeMap<Node, BTreeSet<Node>>, from: Node, to: Node) -> bool {
    let mut stack = vec![from];
    let mut seen = BTreeSet::new();
    while let Some(n) = stack.pop() {
        for r in &reads[&n] {
            if *r == to {
                return true;
            }
            if seen.insert(*r) {
                stack.push(*r);
            }
        }
    }
    false
}

fn check(doc: &Document, touched: &[u8]) -> Result<(), TestCaseError> {
    let reads = oracle_reads(doc);
    let forward = doc.parts.values().any(|p| {
        p.history.iter().enumerate().any(|(at, f)| {
            reads[&Node::Feature(*f)].iter().any(|r| {
                matches!(r, Node::Feature(t) if p.history.iter().position(|x| x == t) >= Some(at))
            })
        })
    });
    let cyclic = reads.keys().any(|n| reachable(&reads, *n, *n));
    match Dag::build(doc) {
        Ok(dag) => {
            prop_assert!(!forward && !cyclic);
            prop_assert_eq!(dag.order().len(), reads.len());
            let pos: BTreeMap<Node, usize> = dag
                .order()
                .iter()
                .enumerate()
                .map(|(i, n)| (*n, i))
                .collect();
            for (n, rs) in &reads {
                prop_assert_eq!(&dag.reads(*n).collect::<BTreeSet<_>>(), rs);
                for r in rs {
                    prop_assert!(pos[r] < pos[n], "{} before {}", r, n);
                }
            }
            let touched: BTreeSet<Node> = touched
                .iter()
                .filter_map(|i| dag.order().get(*i as usize))
                .copied()
                .collect();
            let dirty = dag.dirty(&touched);
            let expect: Vec<Node> = dag
                .order()
                .iter()
                .filter(|n| {
                    touched
                        .iter()
                        .any(|t| *n == t || reachable(&reads, **n, *t))
                })
                .copied()
                .collect();
            prop_assert_eq!(dirty, expect);
        }
        Err(DagError::Order {
            feature, target, ..
        }) => {
            prop_assert!(forward);
            let (fp, fat, _) = doc.feature(feature).unwrap();
            let (tp, tat, _) = doc.feature(target).unwrap();
            prop_assert!(fp.id == tp.id && tat >= fat);
        }
        Err(DagError::Cycle(nodes)) => {
            prop_assert!(cyclic && !forward);
            for (i, n) in nodes.iter().enumerate() {
                let next = nodes[(i + 1) % nodes.len()];
                prop_assert!(reads[n].contains(&next), "{} reads {}", n, next);
            }
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn random_edits_keep_the_dag_honest(
        edits in proptest::collection::vec(arb_edit(), 1..24),
        touched in proptest::collection::vec(0u8..30, 0..4),
    ) {
        let mut doc = Document::default();
        for (n, edit) in edits.iter().enumerate() {
            apply(&mut doc, n as u64 + 1, edit);
            check(&doc, &touched)?;
        }
    }
}
