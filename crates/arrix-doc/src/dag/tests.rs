use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{FeatureId, Id, ParamId, PartId, QuantityKind, Ref};
use arrix_sketch::{Constraint, Draft};
use proptest::prelude::*;

use super::*;
use crate::document::{Document, FeatureRecord, FeatureTypeId, Invalid, Param, Part};
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
            .map(|(i, r)| (format!("in{i}"), r.into()))
            .collect(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

fn push_feature(doc: &mut Document, part: PartId, rec: FeatureRecord) {
    let p = doc.parts.entry(part).or_insert_with(|| Part {
        id: part,
        name: format!("part{}", part.0.0),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
        material: None,
        body_materials: Default::default(),
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
fn a_name_reads_the_features_of_its_root_and_every_step() {
    let name = format!(
        "face:revolve.{}.side.{}/inst.{}.2/mod.{}.1/gen.{}[edge:sweep.{}.end-cap]",
        Id(1),
        Id(9),
        Id(2),
        Id(3),
        Id(4),
        Id(5)
    );
    let r = Ref::Topo(name.parse().unwrap());
    let want: BTreeSet<_> = (1..=5).map(fid).collect();
    assert_eq!(ref_features(&r), want);
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
    lower.inputs.insert("plane".into(), topo(22).into());
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
    lower
        .inputs
        .insert("plane".into(), Ref::Feature(fid(20)).into());
    assert!(matches!(Dag::build(&doc), Err(DagError::Order { target, .. }) if target == fid(20)));
}

#[test]
fn a_list_input_is_one_edge_per_element() {
    let (mut doc, ..) = scenario();
    let part = PartId(Id(10));
    let lower = doc
        .parts
        .get_mut(&part)
        .unwrap()
        .features
        .get_mut(&fid(20))
        .unwrap();
    // Forward in history through the second element: refused, naming it.
    lower.inputs.insert(
        "edges".into(),
        vec![Ref::Param(ParamId(Id(1))), topo(22)].into(),
    );
    let err = Dag::build(&doc).unwrap_err();
    assert_eq!(
        err,
        DagError::Order {
            feature: fid(20),
            field: "inputs.edges".into(),
            target: fid(22)
        }
    );
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
            for r in rec.inputs.values().flat_map(|i| i.refs()) {
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

mod assemblies {
    use arrix_core::{AssemblyId, ComponentId, JointId, SlotName};

    use super::*;
    use crate::assembly::{Anchor, Assembly, Component, ComponentOf, Joint, JointKind, Pose};
    use crate::registry::Registry;

    fn e(text: &str) -> Expr {
        Expr::parse(text).unwrap()
    }

    fn typed(
        id: u64,
        type_id: &str,
        params: &[(&str, &str)],
        inputs: Vec<(&str, Ref)>,
    ) -> FeatureRecord {
        FeatureRecord {
            type_id: FeatureTypeId::new(type_id).unwrap(),
            params: params.iter().map(|(k, v)| (k.to_string(), e(v))).collect(),
            inputs: inputs
                .into_iter()
                .map(|(k, r)| (k.to_string(), r.into()))
                .collect(),
            ..record(fid(id), vec![], &[])
        }
    }

    fn body_of(feature: u64) -> Ref {
        Ref::Slot {
            feature: fid(feature),
            slot: SlotName::new("body").unwrap(),
        }
    }

    fn component(id: u64, of: ComponentOf, pose: Option<&str>) -> Component {
        Component {
            id: ComponentId(Id(id)),
            name: format!("c{id}"),
            of,
            pose: pose.map(|z| Pose {
                position: [e("0 mm"), e("0 mm"), e(z)],
                rpy: [e("0 deg"), e("0 deg"), e("0 deg")],
            }),
        }
    }

    fn anchor(path: &[u64], axis: Ref) -> Anchor {
        let Ref::Topo(axis) = axis else { panic!() };
        Anchor {
            path: path.iter().map(|c| ComponentId(Id(*c))).collect(),
            axis,
            origin: None,
        }
    }

    fn assembly(id: u64, components: Vec<Component>, joints: Vec<Joint>) -> Assembly {
        Assembly {
            id: AssemblyId(Id(id)),
            name: format!("a{id}"),
            components: components.into_iter().map(|c| (c.id, c)).collect(),
            joints: joints.into_iter().map(|j| (j.id, j)).collect(),
        }
    }

    const BRACKET: PartId = PartId(Id(10));
    const OTHER: PartId = PartId(Id(11));
    const SERVO: PartId = PartId(Id(12));
    const ASM: u64 = 50;
    const C_BRACKET: u64 = 51;
    const C_SERVO: u64 = 52;
    const C_OTHER: u64 = 53;
    const JOINT: u64 = 60;

    /// A bracket (a plate, two fillets of it by `r` and `s`, then a datum
    /// plane by `d`), an unrelated part with a sketch-like feature, a servo
    /// part, and an assembly whose joint goes from a face of the plate to a
    /// face of the servo, its value `angle`.
    fn scene() -> Document {
        let mut doc = Document::default();
        for (id, name, expr) in [
            (1, "w", "20 mm"),
            (2, "r", "1 mm"),
            (3, "s", "2 mm"),
            (4, "d", "5 mm"),
            (5, "angle", "30 deg"),
            (6, "z", "3 mm"),
            (7, "zo", "4 mm"),
        ] {
            param(&mut doc, id, name, expr);
        }
        push_feature(
            &mut doc,
            BRACKET,
            typed(20, "core.extrude", &[("distance", "w")], vec![]),
        );
        push_feature(
            &mut doc,
            BRACKET,
            typed(
                21,
                "core.fillet",
                &[("radius", "r")],
                vec![("target", body_of(20))],
            ),
        );
        push_feature(
            &mut doc,
            BRACKET,
            typed(
                22,
                "core.fillet",
                &[("radius", "s")],
                vec![("target", body_of(20))],
            ),
        );
        push_feature(
            &mut doc,
            BRACKET,
            typed(23, "core.datum-plane", &[("offset", "d")], vec![]),
        );
        push_feature(
            &mut doc,
            OTHER,
            typed(30, "core.extrude", &[("distance", "20 mm")], vec![]),
        );
        push_feature(&mut doc, SERVO, typed(40, "core.import-step", &[], vec![]));
        let joint = Joint {
            id: JointId(Id(JOINT)),
            name: "hinge".into(),
            kind: JointKind::Revolute,
            parent: anchor(&[C_BRACKET], topo(20)),
            child: anchor(&[C_SERVO], topo(40)),
            value: e("angle"),
            offset: e("0 deg"),
            limits: None,
            flip: false,
        };
        let a = assembly(
            ASM,
            vec![
                component(C_BRACKET, ComponentOf::Part(BRACKET), Some("z")),
                component(C_SERVO, ComponentOf::Part(SERVO), None),
                component(C_OTHER, ComponentOf::Part(OTHER), Some("zo")),
            ],
            vec![joint],
        );
        doc.assemblies.insert(a.id, a);
        doc
    }

    fn dag(doc: &Document) -> Dag {
        doc.validate().unwrap();
        Dag::build_with(doc, &Registry::with_core_types()).unwrap()
    }

    fn dirty(dag: &Dag, touched: &[Node]) -> Vec<Node> {
        dag.dirty(&touched.iter().copied().collect())
    }

    const JOINT_NODE: Node = Node::Joint(JointId(Id(JOINT)));
    const ASSEMBLY_NODE: Node = Node::Assembly(AssemblyId(Id(ASM)));

    #[test]
    fn a_joint_reads_its_anchors_and_its_parameters() {
        let d = dag(&scene());
        let reads: BTreeSet<Node> = d.reads(JOINT_NODE).collect();
        let want = [
            Node::Param(ParamId(Id(5))),
            Node::Component(ComponentId(Id(C_BRACKET))),
            Node::Component(ComponentId(Id(C_SERVO))),
            Node::Part(BRACKET),
            Node::Part(SERVO),
            Node::Feature(fid(40)),
            // The plate, and every feature that modifies its body: not the
            // datum plane after them.
            Node::Feature(fid(20)),
            Node::Feature(fid(21)),
            Node::Feature(fid(22)),
        ];
        assert_eq!(reads, want.into_iter().collect::<BTreeSet<_>>());
        let assembly: BTreeSet<Node> = d.reads(ASSEMBLY_NODE).collect();
        assert!(assembly.contains(&JOINT_NODE));
        assert!(assembly.contains(&Node::Component(ComponentId(Id(C_OTHER)))));
        // The ordering puts the joint after what it reads and before its
        // assembly.
        let at = |n: Node| d.order().iter().position(|m| *m == n).unwrap();
        assert!(at(Node::Feature(fid(22))) < at(JOINT_NODE));
        assert!(at(JOINT_NODE) < at(ASSEMBLY_NODE));
    }

    #[test]
    fn a_width_edit_dirties_the_joint_and_an_unanchored_edit_does_not() {
        let d = dag(&scene());
        let w = dirty(&d, &[Node::Param(ParamId(Id(1)))]);
        assert!(
            w.contains(&JOINT_NODE) && w.contains(&ASSEMBLY_NODE),
            "{w:?}"
        );
        // A modifier of the anchored body, late in the history.
        let s = dirty(&d, &[Node::Param(ParamId(Id(3)))]);
        assert!(s.contains(&JOINT_NODE), "{s:?}");
        // The joint's own value, and the pose of the bracket's component.
        assert!(dirty(&d, &[Node::Param(ParamId(Id(5)))]).contains(&JOINT_NODE));
        // The pose of the bracket's component places the joint's parent.
        let z = dirty(&d, &[Node::Param(ParamId(Id(6)))]);
        assert!(z.contains(&JOINT_NODE), "{z:?}");
        // The pose of a component no joint goes through does not.
        let zo = dirty(&d, &[Node::Param(ParamId(Id(7)))]);
        assert!(!zo.contains(&JOINT_NODE), "{zo:?}");
        assert!(zo.contains(&ASSEMBLY_NODE));
        // A feature after the anchored body's last, one in an unanchored
        // part, and a part nothing anchors to leave the joint alone.
        for untouched in [
            Node::Param(ParamId(Id(4))),
            Node::Feature(fid(23)),
            Node::Feature(fid(30)),
            Node::Part(OTHER),
        ] {
            let dirt = dirty(&d, &[untouched]);
            assert!(!dirt.contains(&JOINT_NODE), "{untouched:?}: {dirt:?}");
        }
        // Rolling the anchored part back, or adding to it, can change what
        // the names resolve against.
        assert!(dirty(&d, &[Node::Part(BRACKET)]).contains(&JOINT_NODE));
        // Editing the component the joint places does too.
        assert!(dirty(&d, &[Node::Component(ComponentId(Id(C_SERVO)))]).contains(&JOINT_NODE));
    }

    #[test]
    fn an_assembly_in_itself_is_a_cycle_at_any_depth() {
        let mut doc = scene();
        let (outer, inner) = (AssemblyId(Id(70)), AssemblyId(Id(71)));
        let nest = |id: u64, of: AssemblyId| {
            assembly(
                id,
                vec![component(id + 10, ComponentOf::Assembly(of), None)],
                vec![],
            )
        };
        doc.assemblies.insert(outer, nest(70, inner));
        doc.assemblies.insert(inner, nest(71, outer));
        let err = doc.validate().unwrap_err();
        let Invalid::Dag(DagError::Cycle(nodes)) = &err else {
            panic!("{err}")
        };
        assert!(nodes.contains(&Node::Assembly(outer)) && nodes.contains(&Node::Assembly(inner)));
        assert_eq!(err.diagnostic().code.as_str(), "dag.cycle");
        // Itself, directly.
        let mut doc = scene();
        doc.assemblies.insert(outer, nest(70, outer));
        assert!(matches!(
            doc.validate().unwrap_err(),
            Invalid::Dag(DagError::Cycle(_))
        ));
        // Nesting without a loop is fine, and reads the nested assembly.
        let mut doc = scene();
        doc.assemblies.insert(outer, nest(70, AssemblyId(Id(ASM))));
        let d = doc.validate().unwrap();
        assert!(
            d.reads(Node::Component(ComponentId(Id(80))))
                .any(|n| n == ASSEMBLY_NODE)
        );
    }

    #[test]
    fn a_lost_path_is_no_edge_but_its_first_component_is_read() {
        let mut doc = scene();
        let a = doc.assemblies.get_mut(&AssemblyId(Id(ASM))).unwrap();
        a.joints.get_mut(&JointId(Id(JOINT))).unwrap().child.path =
            vec![ComponentId(Id(C_SERVO)), ComponentId(Id(99))];
        let d = dag(&doc);
        let reads: BTreeSet<Node> = d.reads(JOINT_NODE).collect();
        assert!(reads.contains(&Node::Component(ComponentId(Id(C_SERVO)))));
        assert!(!reads.contains(&Node::Part(SERVO)) && !reads.contains(&Node::Feature(fid(40))));
    }
}
