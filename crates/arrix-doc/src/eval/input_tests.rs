//! The input kinds of plugin API 0.4 through the evaluator (ADR-0012,
//! ADR-0013): a list of edges, a point and an axis from a sketch and from
//! the body, the tool a joining feature keeps, and the failures that name
//! what they could not meet. The types here are test types over the API.

use std::sync::{Arc, Mutex};

use arrix_core::{
    CurveKey, DVec2, DVec3, Id, Input, NameRoot, PersistentName, Profile, ProfileLoop,
    ProfileSegment, Ref, SweepPartName, TopoKind,
};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    SlotKind, SlotOutput, SlotSpec, Tool, ToolOp,
};
use arrix_sketch::{Draft, Entity, Point};

use super::tests::{Block, cached, doc, failure, fid, plane_slot, record, slot};
use super::*;
use crate::registry::TypeOutput;

const MM: f64 = 1e-3;
const BLOCK: u64 = 2;
const KEEPER: u64 = 3;
const PROBE: u64 = 4;
const SKETCH: u64 = 5;

fn spec_input(name: &str, kind: InputKind, list: bool) -> InputSpec {
    InputSpec {
        name: name.into(),
        title: name.into(),
        kind,
        list,
        shown_when: None,
    }
}

/// `test.keeper`: fuses a 2 mm post into its `target` and keeps it as its
/// tool. Its `body` is the target's next version.
struct Keeper(FeatureTypeSpec);

impl Keeper {
    fn new() -> Self {
        Keeper(FeatureTypeSpec {
            id: "test.keeper".into(),
            version: 1,
            title: "Keeper".into(),
            params: vec![],
            inputs: vec![spec_input("target", InputKind::Body, false)],
            outputs: vec![SlotSpec {
                name: slot("body"),
                kind: SlotKind::Body,
                modifies: Some("target".into()),
            }],
            choices: vec![],
        })
    }
}

impl FeatureType for Keeper {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.0
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let Some(InputValue::Body(target)) = args.input("target").map(|i| &i.value) else {
            unreachable!("the test sets a target")
        };
        let line = |k: u64, x: f64, y: f64| ProfileSegment::Line {
            key: CurveKey(Id(k)),
            to: DVec2::new(x, y),
        };
        let outer = ProfileLoop::Path {
            start: DVec2::new(1.0 * MM, 1.0 * MM),
            segments: vec![
                line(1, 3.0 * MM, 1.0 * MM),
                line(2, 3.0 * MM, 3.0 * MM),
                line(3, 1.0 * MM, 3.0 * MM),
                line(4, 1.0 * MM, 1.0 * MM),
            ],
        };
        let plane = Frame::WORLD_XY.offset(9.0 * MM);
        let tool = kernel.extrude(&Profile::new(plane, outer, vec![]).unwrap(), 4.0 * MM)?;
        let body = kernel.fuse(target, &tool)?;
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: slot("body"),
                value: OutputValue::Body(body),
            }],
            tool: Some(Tool {
                body: tool,
                op: ToolOp::Fuse,
            }),
        }
        .into())
    }
}

/// What `test.probe` was given, as text, once per evaluation.
type Seen = Arc<Mutex<Vec<String>>>;

/// `test.probe`: reads one of each new kind, records what it got, and
/// measures the tool it was given, so the handle is shown to be live.
struct Probe(FeatureTypeSpec, Seen);

impl Probe {
    fn new(seen: Seen) -> Self {
        Probe(
            FeatureTypeSpec {
                id: "test.probe".into(),
                version: 1,
                title: "Probe".into(),
                params: vec![],
                inputs: vec![
                    spec_input("edges", InputKind::Edge, true),
                    spec_input("corner", InputKind::Point, false),
                    spec_input("rail", InputKind::Axis, false),
                    spec_input("tool", InputKind::Tool, false),
                    spec_input("marks", InputKind::Point, true),
                ],
                outputs: vec![SlotSpec {
                    name: slot("plane"),
                    kind: SlotKind::Plane,
                    modifies: None,
                }],
                choices: vec![],
            },
            seen,
        )
    }
}

impl FeatureType for Probe {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.0
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let mut seen = Vec::new();
        for input in &args.inputs {
            seen.push(match &input.value {
                InputValue::Edge(n) => format!("{}: {n}", input.name),
                InputValue::Point(p) => format!("{}: {:?} {:?}", input.name, p.at, p.normal),
                InputValue::Axis(a) => format!("{}: {:?} {:?}", input.name, a.origin, a.direction),
                InputValue::Tool(t) => {
                    let volume = kernel.measure(&t.body)?.volume;
                    format!("{}: {:?} {volume:.3e}", input.name, t.op)
                }
                other => format!("{}: {other:?}", input.name),
            });
        }
        *self.1.lock().unwrap() = seen;
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: slot("plane"),
                value: OutputValue::Plane(Frame::WORLD_XY),
            }],
            tool: None,
        }
        .into())
    }
}

fn registry(seen: &Seen) -> Registry {
    let mut r = Registry::with_core_types();
    r.register(Arc::new(Block::new())).unwrap();
    r.register(Arc::new(Keeper::new())).unwrap();
    r.register(Arc::new(Probe::new(seen.clone()))).unwrap();
    r
}

fn name(kind: TopoKind, part: SweepPartName) -> Ref {
    Ref::Topo(PersistentName {
        kind,
        root: NameRoot::Sweep {
            feature: fid(BLOCK),
            index: 0,
            part,
        },
        chain: vec![],
    })
}

/// A sketch with two points and a line between them, on the datum plane
/// (z = 10 mm), and its ids.
fn sketch() -> (FeatureRecord, [Ref; 3]) {
    let mut d = Draft::seeded(3);
    let a = d.add_point(Point::new(2.0 * MM, 3.0 * MM));
    let b = d.add_point(Point::new(6.0 * MM, 3.0 * MM));
    let line = d.add_entity(Entity::Line { start: a, end: b });
    let mut r = record(SKETCH, "core.sketch", &[], &[("plane", plane_slot(1))]);
    r.params.clear();
    r.sketch = Some(Box::new(d.sketch));
    let at = |entity| Ref::Sketch {
        feature: fid(SKETCH),
        entity,
    };
    (r, [at(a), at(b), at(line)])
}

/// A plane 10 mm up, a 20 mm block on it 8 mm high, a keeper joining a post
/// to it, the sketch, and the probe reading them as `probe` says.
fn scenario(probe: impl FnOnce(&mut FeatureRecord, &[Ref; 3])) -> Document {
    let (sketch, marks) = sketch();
    let mut p = record(PROBE, "test.probe", &[], &[]);
    probe(&mut p, &marks);
    doc(
        &[],
        vec![
            record(1, "core.datum-plane", &[("offset", "10 mm")], &[]),
            record(
                BLOCK,
                "test.block",
                &[("width", "20 mm"), ("height", "8 mm")],
                &[("plane", plane_slot(1))],
            ),
            record(
                KEEPER,
                "test.keeper",
                &[],
                &[(
                    "target",
                    Ref::Slot {
                        feature: fid(BLOCK),
                        slot: slot("body"),
                    },
                )],
            ),
            sketch,
            p,
        ],
    )
}

fn everything(p: &mut FeatureRecord, marks: &[Ref; 3]) {
    let edges = vec![
        name(TopoKind::Edge, SweepPartName::EndEdge(CurveKey(Id(2)))),
        name(TopoKind::Edge, SweepPartName::EndEdge(CurveKey(Id(1)))),
    ];
    p.inputs.insert("edges".into(), Input::Many(edges));
    p.inputs.insert(
        "corner".into(),
        name(TopoKind::Vertex, SweepPartName::EndVertex(CurveKey(Id(1)))).into(),
    );
    p.inputs.insert(
        "rail".into(),
        name(TopoKind::Edge, SweepPartName::StartEdge(CurveKey(Id(1)))).into(),
    );
    p.inputs
        .insert("tool".into(), Ref::Feature(fid(KEEPER)).into());
    p.inputs.insert(
        "marks".into(),
        Input::Many(vec![marks[1].clone(), marks[0].clone()]),
    );
}

fn run(doc: &Document) -> (Evaluation, Vec<String>) {
    let seen = Seen::default();
    let e = Evaluator::new(registry(&seen)).evaluate(doc);
    let seen = seen.lock().unwrap().clone();
    (e, seen)
}

#[test]
fn every_new_kind_resolves_and_a_list_arrives_in_order() {
    let (e, seen) = run(&scenario(everything));
    assert!(
        matches!(e.outcome(fid(PROBE)), Some(FeatureOutcome::Ok { .. })),
        "{:?}",
        e.outcome(fid(PROBE))
    );
    let at = |x: f64, y: f64, z: f64| format!("{:?} ", DVec3::new(x * MM, y * MM, z * MM));
    let edge = |k: u64| name(TopoKind::Edge, SweepPartName::EndEdge(CurveKey(Id(k))));
    let text = |r: Ref| match r {
        Ref::Topo(n) => n.to_string(),
        _ => unreachable!(),
    };
    // The edges, in the record's order, one input each.
    assert_eq!(seen[0], format!("edges: {}", text(edge(2))));
    assert_eq!(seen[1], format!("edges: {}", text(edge(1))));
    // The vertex above the block's origin corner: 10 mm + 8 mm up, no
    // normal.
    assert_eq!(seen[2], format!("corner: {}None", at(0.0, 0.0, 18.0)));
    // Side 1's bottom edge along x, at the block's base.
    assert!(seen[3].starts_with("rail: "), "{}", seen[3]);
    assert!(seen[3].contains("0.0, 0.0"), "{}", seen[3]);
    // The tool: the keeper's post, joined, a 2 × 2 × 4 mm box.
    assert_eq!(seen[4], "tool: Fuse 1.600e-8");
    // The sketch's points, in the order given, on the plane 10 mm up with
    // its normal.
    let normal = format!("Some({:?})", DVec3::Z);
    assert_eq!(seen[5], format!("marks: {}{normal}", at(6.0, 3.0, 10.0)));
    assert_eq!(seen[6], format!("marks: {}{normal}", at(2.0, 3.0, 10.0)));
}

#[test]
fn a_sketch_line_is_an_axis_from_its_start_and_an_unchanged_probe_is_cached() {
    let doc = scenario(|p, marks| {
        p.inputs.insert("rail".into(), marks[2].clone().into());
    });
    let (e, seen) = run(&doc);
    assert_eq!(
        seen,
        [format!(
            "rail: {:?} {:?}",
            DVec3::new(2.0 * MM, 3.0 * MM, 10.0 * MM),
            DVec3::X
        )]
    );
    // The same document through the same evaluator reuses it.
    let seen = Seen::default();
    let mut ev = Evaluator::new(registry(&seen));
    ev.evaluate(&doc);
    assert!(cached(&ev.evaluate(&doc), PROBE));
    assert!(matches!(
        e.outcome(fid(PROBE)),
        Some(FeatureOutcome::Ok { .. })
    ));
}

#[test]
fn a_moved_sketch_point_or_a_changed_tool_changes_the_probes_hash() {
    let hash = |e: &Evaluation| match e.outcome(fid(PROBE)) {
        Some(FeatureOutcome::Ok { hash, .. }) => *hash,
        other => panic!("{other:?}"),
    };
    let (base, _) = run(&scenario(everything));
    let mut moved = scenario(everything);
    let sketch = moved
        .parts
        .get_mut(&super::tests::PART)
        .unwrap()
        .features
        .get_mut(&fid(SKETCH))
        .unwrap();
    let sk = sketch.sketch.as_mut().unwrap();
    let id = *sk.points().keys().next().unwrap();
    sk.point_mut(id).unwrap().set_pos(2.5 * MM, 3.0 * MM);
    let (after, _) = run(&moved);
    assert_ne!(hash(&base), hash(&after), "a point of the marks moved");

    // The tool is the keeper's: the block's width changes what it is a
    // tool of, and the probe reads that.
    let mut wider = scenario(everything);
    wider
        .parts
        .get_mut(&super::tests::PART)
        .unwrap()
        .features
        .get_mut(&fid(BLOCK))
        .unwrap()
        .params
        .insert("width".into(), Expr::parse("22 mm").unwrap());
    let (after, _) = run(&wider);
    assert_ne!(hash(&base), hash(&after));
}

#[test]
fn a_list_on_a_single_input_and_one_reference_on_a_list_input_are_refused() {
    let edge = name(TopoKind::Edge, SweepPartName::EndEdge(CurveKey(Id(1))));
    let (e, _) = run(&scenario(|p, _| {
        p.inputs
            .insert("corner".into(), Input::Many(vec![edge.clone()]));
    }));
    assert_eq!(failure(&e, PROBE).code.as_str(), "input.list");
    let (e, _) = run(&scenario(|p, _| {
        p.inputs.insert("edges".into(), edge.clone().into());
    }));
    assert_eq!(failure(&e, PROBE).code.as_str(), "input.list");
    // An empty list is the input unset.
    let (e, _) = run(&scenario(|p, _| {
        p.inputs.insert("edges".into(), Input::Many(vec![]));
    }));
    assert!(matches!(
        e.outcome(fid(PROBE)),
        Some(FeatureOutcome::Ok { .. })
    ));
}

#[test]
fn a_lost_edge_names_the_element_with_candidates_and_a_wrong_kind_is_refused() {
    let gone = name(TopoKind::Edge, SweepPartName::EndEdge(CurveKey(Id(99))));
    let (e, _) = run(&scenario(|p, _| {
        let ok = name(TopoKind::Edge, SweepPartName::EndEdge(CurveKey(Id(1))));
        p.inputs
            .insert("edges".into(), Input::Many(vec![ok, gone.clone()]));
    }));
    let d = failure(&e, PROBE);
    assert_eq!(d.code.as_str(), "ref.lost");
    assert_eq!(d.refs, [gone]);
    assert!(!d.candidates.is_empty() && d.candidates.len() <= 5);

    for (input, what, code) in [
        (
            "edges",
            name(TopoKind::Face, SweepPartName::EndCap),
            "input.wrong-kind",
        ),
        (
            "corner",
            name(TopoKind::Edge, SweepPartName::EndEdge(CurveKey(Id(1)))),
            "input.wrong-kind",
        ),
        (
            "rail",
            name(TopoKind::Vertex, SweepPartName::EndVertex(CurveKey(Id(1)))),
            "input.wrong-kind",
        ),
        // A feature's plane slot is no tool.
        ("tool", plane_slot(1), "input.wrong-kind"),
    ] {
        let list = input == "edges";
        let (e, _) = run(&scenario(|p, _| {
            let value: Input = if list {
                Input::Many(vec![what.clone()])
            } else {
                what.clone().into()
            };
            p.inputs.insert(input.into(), value);
        }));
        assert_eq!(failure(&e, PROBE).code.as_str(), code, "{input}");
    }
}

#[test]
fn a_feature_that_keeps_no_tool_is_input_no_tool_and_only_the_keeper_has_one() {
    let (e, _) = run(&scenario(|p, _| {
        p.inputs
            .insert("tool".into(), Ref::Feature(fid(BLOCK)).into());
    }));
    let d = failure(&e, PROBE);
    assert_eq!(d.code.as_str(), "input.no-tool");
    assert_eq!(d.refs, [Ref::Feature(fid(BLOCK))]);
    // The tool is no slot of the keeper: nothing reads it as a body.
    let (e, _) = run(&scenario(|p, _| {
        p.inputs.insert(
            "tool".into(),
            Ref::Slot {
                feature: fid(KEEPER),
                slot: slot("body"),
            }
            .into(),
        );
    }));
    assert_eq!(failure(&e, PROBE).code.as_str(), "input.wrong-kind");
}

#[test]
fn the_tool_is_a_dag_edge_and_survives_the_cache_between_evaluations() {
    let doc = scenario(everything);
    let seen = Seen::default();
    let mut ev = Evaluator::new(registry(&seen));
    ev.evaluate(&doc);
    // Collecting between evaluations keeps the keeper's tool body alive:
    // the probe measures it again when it is evaluated afresh after an
    // unrelated edit leaves the keeper cached.
    let mut edited = doc.clone();
    edited
        .parts
        .get_mut(&super::tests::PART)
        .unwrap()
        .features
        .get_mut(&fid(SKETCH))
        .unwrap()
        .sketch
        .as_mut()
        .unwrap()
        .points_mut()
        .next()
        .unwrap()
        .set_pos(2.5 * MM, 3.0 * MM);
    let second = ev.evaluate(&edited);
    assert!(cached(&second, KEEPER));
    assert!(!cached(&second, PROBE));
    assert_eq!(seen.lock().unwrap()[4], "tool: Fuse 1.600e-8");
}
