//! Body slots' versions (ADR-0007): a body input resolves to the version
//! current at the reading feature, a modifier's output is the target's next
//! version, a consumer ends the slot, and the DAG holds the edges that
//! implies. The modifiers here are test types over the plugin API.

use std::sync::Arc;

use arrix_core::{
    CurveKey, DVec2, DVec3, Diagnostic, Id, PartId, ProfileLoop, ProfileSegment, QuantityKind,
    Severity, SweepPartName,
};
use arrix_plugin_api::{
    Body, FeatureOutput, FeatureTypeSpec, InputKind, InputSpec, InputValue, Kernel, OutputValue,
    ParamSpec, SlotKind, SlotOutput, SlotSpec,
};
use proptest::prelude::*;

use super::tests::{Block, PART, cached, doc, face, failure, fid, record, slot};
use super::*;
use crate::command::{Applied, Command, apply};
use crate::document::Part;
use crate::registry::TypeOutput;

const MM: f64 = 1e-3;

fn body_ref(feature: u64) -> Ref {
    Ref::Slot {
        feature: fid(feature),
        slot: slot("body"),
    }
}

fn input(name: &str, kind: InputKind) -> InputSpec {
    InputSpec {
        name: name.into(),
        title: name.into(),
        kind,
    }
}

fn length(name: &str, default: &str) -> ParamSpec {
    ParamSpec {
        name: name.into(),
        title: name.into(),
        kind: QuantityKind::Length,
        default: default.into(),
    }
}

fn spec(id: &str, params: Vec<ParamSpec>, inputs: Vec<InputSpec>, outputs: Vec<SlotSpec>) -> Spec {
    Spec(FeatureTypeSpec {
        id: id.into(),
        version: 1,
        title: id.into(),
        params,
        inputs,
        outputs,
    })
}

struct Spec(FeatureTypeSpec);

fn plane_out(z: f64) -> TypeOutput {
    let f = Frame::WORLD_XY.offset(z);
    FeatureOutput {
        slots: vec![SlotOutput {
            name: slot("plane"),
            value: OutputValue::Plane(f),
        }],
    }
    .into()
}

fn target(args: &FeatureArgs, name: &str) -> Result<Body, Diagnostic> {
    match args.input(name).map(|i| &i.value) {
        Some(InputValue::Body(b)) => Ok(Body::from_handle(b.handle())),
        _ => Err(Diagnostic::new(Severity::Error, "test.no-body", "no body").unwrap()),
    }
}

/// `test.mod`: fuses or cuts (the `op` choice) a `size` post at `x`, which
/// runs from 1 mm below the plane through 6 mm above it, into its `target`.
/// Its `body` output is the target's next version.
struct Modifier(FeatureTypeSpec);

impl Modifier {
    fn new() -> Self {
        let s = spec(
            "test.mod",
            vec![length("x", "5 mm"), length("size", "2 mm")],
            vec![input("target", InputKind::Body)],
            vec![SlotSpec {
                name: slot("body"),
                kind: SlotKind::Body,
                modifies: Some("target".into()),
            }],
        );
        Modifier(s.0)
    }
}

impl FeatureType for Modifier {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.0
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let base = target(args, "target")?;
        let (x, size) = (args.param("x").unwrap(), args.param("size").unwrap());
        let line = |k: u64, to: DVec2| ProfileSegment::Line {
            key: CurveKey(Id(k)),
            to,
        };
        let outer = ProfileLoop::Path {
            start: DVec2::new(x, x),
            segments: vec![
                line(1, DVec2::new(x + size, x)),
                line(2, DVec2::new(x + size, x + size)),
                line(3, DVec2::new(x, x + size)),
                line(4, DVec2::new(x, x)),
            ],
        };
        let plane = Frame::WORLD_XY.offset(-MM);
        let profile = arrix_core::Profile::new(plane, outer, vec![]).unwrap();
        let tool = kernel.extrude(&profile, 7.0 * MM)?;
        let body = match args.choices.get("op").map(String::as_str) {
            None | Some("fuse") => kernel.fuse(&base, &tool)?,
            Some("cut") => kernel.cut(&base, &tool)?,
            Some(other) => {
                return Err(Diagnostic::new(
                    Severity::Error,
                    "test.op",
                    format!("no operation {other}"),
                )
                .unwrap());
            }
        };
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: slot("body"),
                value: OutputValue::Body(body),
            }],
        }
        .into())
    }
}

/// `test.use`: reads its `target` as a tool and modifies nothing.
struct Consumer(FeatureTypeSpec);

impl FeatureType for Consumer {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.0
    }

    fn evaluate(&self, _: &mut dyn Kernel, args: &FeatureArgs) -> Result<TypeOutput, Diagnostic> {
        target(args, "target")?;
        Ok(plane_out(0.0))
    }
}

/// `test.read`: a `plane` at the volume, in metres, of the version of its
/// `body` it was given, which tells which version that was. It passes the
/// body on unchanged as its next version, since a body input that nothing
/// modifies is a tool and ends its slot.
struct Reader(FeatureTypeSpec);

impl FeatureType for Reader {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.0
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        let body = target(args, "body")?;
        let mut out = plane_out(kernel.measure(&body)?.volume);
        out.output.slots.push(SlotOutput {
            name: slot("body"),
            value: OutputValue::Body(body),
        });
        Ok(out)
    }
}

fn registry() -> Registry {
    let plane = || {
        vec![SlotSpec {
            name: slot("plane"),
            kind: SlotKind::Plane,
            modifies: None,
        }]
    };
    let mut passes = plane();
    passes.push(SlotSpec {
        name: slot("body"),
        kind: SlotKind::Body,
        modifies: Some("body".into()),
    });
    let mut r = Registry::with_core_types();
    r.register(Arc::new(Block::new())).unwrap();
    r.register(Arc::new(Modifier::new())).unwrap();
    let consumer = spec(
        "test.use",
        vec![],
        vec![input("target", InputKind::Body)],
        plane(),
    );
    r.register(Arc::new(Consumer(consumer.0))).unwrap();
    let reader = spec(
        "test.read",
        vec![],
        vec![input("body", InputKind::Body)],
        passes,
    );
    r.register(Arc::new(Reader(reader.0))).unwrap();
    r
}

/// The block (feature 1), 40 mm square and 5 mm high, then `features`.
fn part(features: Vec<FeatureRecord>) -> Document {
    let block = record(1, "test.block", &[("width", "40 mm")], &[]);
    doc(&[], std::iter::once(block).chain(features).collect())
}

fn modifier(id: u64, op: &str, x: &str) -> FeatureRecord {
    let mut r = record(id, "test.mod", &[("x", x)], &[("target", body_ref(1))]);
    r.choices.insert("op".into(), op.into());
    r
}

fn consumer(id: u64) -> FeatureRecord {
    record(id, "test.use", &[], &[("target", body_ref(1))])
}

fn reader(id: u64) -> FeatureRecord {
    record(id, "test.read", &[], &[("body", body_ref(1))])
}

fn eval(d: &Document) -> Evaluation {
    Evaluator::new(registry()).evaluate(d)
}

/// The volume of a feature's `body` slot, cubic millimetres.
fn volume(e: &Evaluation, feature: u64) -> f64 {
    match e.slot(fid(feature), "body") {
        Some(SlotView::Body(m)) => m.volume * 1e9,
        other => panic!("feature {feature}: {other:?}"),
    }
}

/// What a `test.read` feature was given, cubic millimetres.
fn read(e: &Evaluation, feature: u64) -> f64 {
    match e.slot(fid(feature), "plane") {
        Some(SlotView::Plane(f)) => f.origin().z * 1e9,
        other => panic!("feature {feature}: {other:?}"),
    }
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

fn insert(d: &Document, at: usize, record: FeatureRecord) -> Applied {
    let add = Command::AddFeature {
        part: PART,
        at,
        record,
    };
    apply(d, &add).unwrap()
}

#[test]
fn a_modifiers_output_is_the_targets_next_version() {
    let e = eval(&part(vec![
        modifier(2, "cut", "5 mm"),
        modifier(3, "fuse", "12 mm"),
    ]));
    assert!(near(volume(&e, 1), 8000.0));
    // A 2 × 2 mm post through 5 mm is cut out; a fused post sticks out 1 mm
    // below and above.
    assert!(near(volume(&e, 2), 7980.0), "{}", volume(&e, 2));
    assert!(
        near(volume(&e, 3), 7980.0 + 8.0),
        "the second modifier worked on the first's body: {}",
        volume(&e, 3)
    );
}

#[test]
fn inserting_a_modifier_moves_every_later_reader_and_undo_moves_them_back() {
    let base = part(vec![reader(3)]);
    let before = eval(&base);
    assert!(near(read(&before, 3), 8000.0));

    let cut = insert(&base, 1, modifier(2, "cut", "5 mm"));
    let after = eval(&cut.document);
    assert_eq!(
        cut.document.feature(fid(3)).unwrap().2,
        base.feature(fid(3)).unwrap().2,
        "the reader's record is untouched"
    );
    assert!(near(read(&after, 3), 7980.0), "{}", read(&after, 3));

    let undone = apply(&cut.document, &cut.inverse).unwrap();
    assert_eq!(undone.document, base);
    assert!(near(read(&eval(&undone.document), 3), 8000.0));
}

#[test]
fn a_reader_takes_the_last_modifier_before_it_and_only_that() {
    let d = part(vec![
        modifier(2, "fuse", "5 mm"),
        reader(3),
        modifier(4, "cut", "12 mm"),
        reader(5),
    ]);
    let e = eval(&d);
    assert!(near(read(&e, 3), volume(&e, 2)));
    assert!(near(read(&e, 5), volume(&e, 4)));
    assert!(!near(read(&e, 3), read(&e, 5)));

    // A modifier after a reader changes nothing the reader read.
    let mut ev = Evaluator::new(registry());
    ev.evaluate(&part(vec![reader(3)]));
    let later =
        ev.evaluate(&insert(&part(vec![reader(3)]), 2, modifier(2, "cut", "5 mm")).document);
    assert!(cached(&later, 3));
    assert!(near(read(&later, 3), 8000.0));
}

#[test]
fn a_suppressed_modifier_is_not_a_version() {
    let mut off = modifier(2, "cut", "5 mm");
    off.suppressed = true;
    let e = eval(&part(vec![off, reader(3)]));
    assert!(near(read(&e, 3), 8000.0));
}

#[test]
fn a_failed_modifier_leaves_its_readers_unavailable_not_on_the_earlier_version() {
    let e = eval(&part(vec![modifier(2, "twist", "5 mm"), reader(3)]));
    assert_eq!(failure(&e, 2).code.as_str(), "test.op");
    let d = failure(&e, 3);
    assert_eq!(d.code.as_str(), "input.unavailable");
    assert!(d.refs.contains(&Ref::Feature(fid(2))), "{d:?}");
}

#[test]
fn a_tool_body_ends_its_slot() {
    let e = eval(&part(vec![reader(2), consumer(3), reader(4), consumer(5)]));
    assert!(near(read(&e, 2), 8000.0), "before the consumer it is alive");
    for (feature, by) in [(4, 3), (5, 3)] {
        let d = failure(&e, feature);
        assert_eq!(d.code.as_str(), "slot.consumed", "feature {feature}");
        assert!(d.refs.contains(&Ref::Feature(fid(by))), "{d:?}");
    }

    let mut off = consumer(3);
    off.suppressed = true;
    let e = eval(&part(vec![off, reader(4)]));
    assert!(
        near(read(&e, 4), 8000.0),
        "a suppressed consumer consumes nothing"
    );
}

#[test]
fn a_name_into_a_body_follows_its_version() {
    let side = face(1, SweepPartName::Side(CurveKey(Id(1))));
    let on_side = || {
        record(
            4,
            "core.datum-plane",
            &[],
            &[("plane", Ref::Topo(side.clone()))],
        )
    };
    let e = eval(&part(vec![modifier(2, "cut", "5 mm"), on_side()]));
    match e.slot(fid(4), "plane") {
        Some(SlotView::Plane(f)) => assert_eq!(f.z_axis(), -DVec3::Y),
        other => panic!("{other:?}"),
    }
    // The name reads the body as of its place: after a consumer it is gone.
    let e = eval(&part(vec![consumer(2), on_side()]));
    assert_eq!(failure(&e, 4).code.as_str(), "slot.consumed");
}

#[test]
fn a_modifiers_own_slot_names_nothing_and_another_part_is_out_of_reach() {
    let own = record(3, "test.read", &[], &[("body", body_ref(2))]);
    let e = eval(&part(vec![modifier(2, "cut", "5 mm"), own]));
    assert_eq!(failure(&e, 3).code.as_str(), "ref.lost");

    let mut d = part(vec![]);
    let elsewhere = PartId(Id(2));
    d.parts.insert(
        elsewhere,
        Part {
            id: elsewhere,
            name: "Other".into(),
            history: vec![fid(3)],
            features: [(fid(3), reader(3))].into(),
            rollback: None,
        },
    );
    let e = eval(&d);
    assert_eq!(failure(&e, 3).code.as_str(), "input.cross-part");
}

#[test]
fn a_reader_reads_the_modifiers_and_consumers_before_it() {
    let d = part(vec![
        modifier(2, "fuse", "5 mm"),
        reader(3),
        consumer(4),
        modifier(5, "cut", "12 mm"),
    ]);
    let dag = Dag::build_with(&d, &registry()).unwrap();
    let reads = |f: u64| -> Vec<FeatureId> {
        dag.reads(Node::Feature(fid(f)))
            .filter_map(|n| match n {
                Node::Feature(f) => Some(f),
                _ => None,
            })
            .collect()
    };
    assert_eq!(reads(3), [fid(1), fid(2)]);
    assert_eq!(reads(4), [fid(1), fid(2), fid(3)]);
    assert_eq!(reads(5), [fid(1), fid(2), fid(3), fid(4)]);
    // Without the registry the DAG has the explicit edges alone.
    let plain = Dag::build(&d).unwrap();
    assert!(
        !plain
            .reads(Node::Feature(fid(3)))
            .any(|n| n == Node::Feature(fid(2)))
    );
}

#[test]
fn a_modifier_cannot_move_before_the_feature_that_made_its_slot() {
    let d = part(vec![modifier(2, "cut", "5 mm"), reader(3)]);
    let to_front = Command::ReorderFeature {
        feature: fid(2),
        to: 0,
    };
    assert!(apply(&d, &to_front).is_err());
    // Past a reader it is allowed: the reader then sees the earlier version.
    let past = Command::ReorderFeature {
        feature: fid(2),
        to: 2,
    };
    let moved = apply(&d, &past).unwrap();
    assert!(near(read(&eval(&moved.document), 3), 8000.0));
    let back = apply(&moved.document, &moved.inverse).unwrap();
    assert_eq!(back.document, d);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    /// Whatever the order of modifiers and readers: the DAG is acyclic, and
    /// each reader is given the last modifier's body before it.
    #[test]
    fn each_reader_is_given_the_last_modifier_before_it(
        steps in proptest::collection::vec(any::<bool>(), 1..9),
    ) {
        let mut features = Vec::new();
        let mut last = 1u64;
        let mut expect = Vec::new();
        for (i, modifies) in steps.iter().enumerate() {
            let id = i as u64 + 2;
            if *modifies {
                // Posts 3 mm apart across the 40 mm block, so no two
                // modifiers give the same volume.
                let x = format!("{} mm", 1 + 3 * i);
                features.push(modifier(id, "fuse", &x));
                last = id;
            } else {
                features.push(reader(id));
                expect.push((id, last));
            }
        }
        let d = part(features);
        prop_assert!(Dag::build_with(&d, &registry()).is_ok());
        let e = eval(&d);
        for (reader, modifier) in expect {
            let want = if modifier == 1 { volume(&e, 1) } else { volume(&e, modifier) };
            prop_assert!(near(read(&e, reader), want), "reader {} of {}", reader, modifier);
        }
    }
}
