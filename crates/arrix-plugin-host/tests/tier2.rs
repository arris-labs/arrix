//! Tier 2 hosting, the happy path (plans/boundary-m1 step 5): a plugin in
//! a child process, written on the bare wire in Python
//! (`tests/fixtures/block`), builds through the host's kernel by callback,
//! and its feature is the same history entry as a Tier 0 double's making
//! the same calls: the same call records, the same named bodies.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use arrix_core::PartId;
use arrix_doc::{
    Command, Document, Evaluator, FeatureOutcome, FeatureRecord, FeatureTypeId, Part, Registry,
    SlotView, apply, expr::Expr,
};
use arrix_plugin_api::{
    ChoiceValue, CurveKey, DVec2, Diagnostic, Feature, FeatureId, FeatureOutput, FeatureTypeSpec,
    Id, InputKind, InputSpec, InputValue, Kernel, OutputValue, ParamSpec, ParamValue, Profile,
    ProfileLoop, ProfileSegment, QuantityKind, Ref, ResolvedInput, SlotKind, SlotName, SlotOutput,
    SlotSpec,
};
use arrix_plugin_host::{register_tier0, register_tier2};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/block")
}

/// `block.block` as `block.py` builds it, compiled in.
struct Double;

impl Feature for Double {
    fn describe(&self) -> Vec<FeatureTypeSpec> {
        vec![FeatureTypeSpec {
            id: "block.block".into(),
            version: 1,
            title: "Block".into(),
            params: vec![ParamSpec {
                name: "size".into(),
                title: "Size".into(),
                kind: QuantityKind::Length,
                default: "10 mm".into(),
                shown_when: None,
            }],
            choices: vec![],
            inputs: vec![InputSpec {
                name: "plane".into(),
                title: "Plane".into(),
                kind: InputKind::Plane,
                list: false,
                shown_when: None,
            }],
            outputs: vec![SlotSpec {
                name: SlotName::new("body").unwrap(),
                kind: SlotKind::Body,
                modifies: None,
            }],
        }]
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        _type_id: &str,
        params: &[ParamValue],
        _choices: &[ChoiceValue],
        inputs: &[ResolvedInput],
    ) -> Result<FeatureOutput, Diagnostic> {
        let InputValue::Plane(plane) = inputs[0].value else {
            unreachable!("the block's one input is a plane");
        };
        let size = params[0].value.si;
        let h = size / 2.0;
        let corners = [(h, -h), (h, h), (-h, h), (-h, -h)];
        let segments = corners
            .iter()
            .enumerate()
            .map(|(i, &(x, y))| ProfileSegment::Line {
                key: CurveKey(Id(i as u64 + 1)),
                to: DVec2::new(x, y),
            })
            .collect();
        let outer = ProfileLoop::Path {
            start: DVec2::new(-h, -h),
            segments,
        };
        let profile = Profile::new(plane, outer, vec![]).unwrap();
        let body = kernel.extrude(&profile, size / 2.0)?;
        let names = kernel.names(&body)?;
        let edge = names
            .into_iter()
            .find(|n| n.to_string().starts_with("edge:"))
            .unwrap();
        let body = kernel.fillet(&body, &[edge], size / 10.0)?;
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: SlotName::new("body").unwrap(),
                value: OutputValue::Body(body),
            }],
            tool: None,
        })
    }
}

fn tier2() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier2(&mut r, &fixture()).unwrap();
    r
}

fn tier0() -> Registry {
    let manifest = std::fs::read_to_string(fixture().join("arrix-plugin.toml"))
        .unwrap()
        .replace("tier = 2", "tier = 0")
        .replace(
            "[tier2]\ncommand = [\"python3\", \"-m\", \"arrix.plugin\", \"block\"]\n",
            "",
        );
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, &manifest, Arc::new(Double)).unwrap();
    r
}

fn record(id: u64, type_id: &str) -> FeatureRecord {
    FeatureRecord {
        id: FeatureId(Id(id)),
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: format!("f{id}"),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

/// A datum plane 5 mm up and a block on it of side `size`.
fn doc(size: &str) -> Document {
    let mut plane = record(1, "core.datum-plane");
    plane
        .params
        .insert("offset".into(), Expr::parse("5 mm").unwrap());
    let mut block = record(2, "block.block");
    block
        .params
        .insert("size".into(), Expr::parse(size).unwrap());
    let on = Ref::Slot {
        feature: FeatureId(Id(1)),
        slot: SlotName::new("plane").unwrap(),
    };
    block.inputs.insert("plane".into(), on.into());
    let features = [plane, block];
    let part = Part {
        id: PartId(Id(100)),
        name: "Part".into(),
        history: features.iter().map(|f| f.id).collect(),
        features: features.into_iter().map(|f| (f.id, f)).collect(),
        rollback: None,
    };
    apply(&Document::default(), &Command::AddPart { part })
        .unwrap()
        .document
}

fn block() -> FeatureId {
    FeatureId(Id(2))
}

#[test]
fn a_tier2_plugin_registers_the_types_it_describes() {
    let r = tier2();
    let id = FeatureTypeId::new("block.block").unwrap();
    let spec = r.get(&id).expect("registered").spec();
    assert_eq!(
        spec,
        &Double.describe()[0],
        "the spec crossed the wire intact"
    );
    assert_eq!(r.get(&id).unwrap().plugin_version(), Some("0.1.0"));
}

#[test]
fn a_tier2_feature_builds_by_callback_as_a_tier0_one_does() {
    let d = doc("10 mm");
    let mut live = Evaluator::new(tier2());
    let mut double = Evaluator::new(tier0());
    let (a, b) = (live.evaluate(&d), double.evaluate(&d));
    let Some(SlotView::Body(m)) = a.slot(block(), "body") else {
        panic!("{:?}", a.events)
    };
    // A 10 mm square 5 mm tall, one vertical or top edge rounded at 1 mm.
    let full = 1e-2 * 1e-2 * 5e-3;
    assert!(m.volume < full && m.volume > 0.99 * full, "{m:?}");
    assert_eq!(m.faces, 7, "{m:?}");
    assert_eq!(a.outcome(block()), b.outcome(block()));
    // Recorded and named as the compiled-in double's: the same calls, and
    // the same bodies with the same names in the frozen result.
    assert_eq!(live.kernel_calls(), double.kernel_calls());
    let ops: Vec<_> = live
        .kernel_calls()
        .iter()
        .map(|c| serde_json::to_value(&c.op).unwrap()["op"].clone())
        .collect();
    // The feature's extrude and fillet, then the evaluator's measure of
    // the slot and the bytes of its frozen result.
    assert_eq!(ops, ["extrude", "fillet", "mass_properties", "to_bytes"]);
    let blobs = |e: &Evaluator| e.frozen()[&block()].blobs.clone();
    assert_eq!(blobs(&live), blobs(&double));
}

#[test]
fn the_process_serves_every_evaluation_of_the_session() {
    let mut live = Evaluator::new(tier2());
    let mut double = Evaluator::new(tier0());
    for size in ["10 mm", "12 mm", "10 mm", "8 mm"] {
        let d = doc(size);
        let (a, b) = (live.evaluate(&d), double.evaluate(&d));
        assert!(
            matches!(a.outcome(block()), Some(FeatureOutcome::Ok { .. })),
            "{size}: {:?}",
            a.events
        );
        assert_eq!(a.outcome(block()), b.outcome(block()), "{size}");
    }
    assert_eq!(live.kernel_calls(), double.kernel_calls());
}

/// `block.py` run as `faulty.py <args>` on the SDK in a directory of its own, its
/// manifest's `[tier2]` and `[capabilities]` as given.
fn faulty(name: &str, args: &[&str], tier2: &str, kernel: bool) -> Registry {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("tier2-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for file in ["block.py", "faulty.py"] {
        std::fs::copy(fixture().join(file), dir.join(file)).unwrap();
    }
    let command: Vec<_> = ["python3", "-m", "arrix.plugin", "faulty"]
        .iter()
        .chain(args)
        .collect();
    let manifest = std::fs::read_to_string(fixture().join("arrix-plugin.toml"))
        .unwrap()
        .replace(
            "command = [\"python3\", \"-m\", \"arrix.plugin\", \"block\"]",
            &format!(
                "command = {}\n{tier2}",
                serde_json::to_string(&command).unwrap()
            ),
        )
        .replace("kernel = true", &format!("kernel = {kernel}"));
    std::fs::write(dir.join("arrix-plugin.toml"), manifest).unwrap();
    let mut r = Registry::with_core_types();
    register_tier2(&mut r, &dir).unwrap();
    r
}

fn failure(e: &arrix_doc::Evaluation) -> (String, String) {
    match e.outcome(block()) {
        Some(FeatureOutcome::Failed { diagnostic, .. }) => {
            (diagnostic.code.to_string(), diagnostic.message.clone())
        }
        other => panic!("{other:?}"),
    }
}

fn fails_as(registry: Registry, code: &str) -> String {
    let mut ev = Evaluator::new(registry);
    let e = ev.evaluate(&doc("10 mm"));
    let (got, message) = failure(&e);
    assert_eq!(got, code, "{message}");
    // The evaluation went on: the plane before it is live.
    assert!(matches!(
        e.outcome(FeatureId(Id(1))),
        Some(FeatureOutcome::Ok { .. })
    ));
    message
}

#[test]
fn a_crash_is_the_features_diagnostic() {
    let m = fails_as(faulty("crash", &["crash"], "", true), "plugin.crash");
    assert!(
        m.contains("block") && m.contains("evaluating block.block"),
        "{m}"
    );
}

#[test]
fn a_silent_plugin_times_out_instead_of_hanging() {
    let r = faulty("hang", &["hang"], "timeout-seconds = 0.5", true);
    fails_as(r, "plugin.timeout");
}

#[test]
fn a_line_that_is_no_message_is_a_protocol_error() {
    fails_as(faulty("garbage", &["garbage"], "", true), "plugin.protocol");
    fails_as(faulty("bad", &["bad-answer"], "", true), "plugin.protocol");
}

#[test]
fn a_kernel_call_without_the_capability_is_refused() {
    let m = fails_as(
        faulty("nokernel", &["none"], "", false),
        "plugin.capability",
    );
    assert!(m.contains("kernel.extrude"), "{m}");
}

#[test]
fn a_handle_no_evaluation_gave_is_refused_by_the_kernel() {
    let m = fails_as(faulty("stale", &["stale"], "", true), "kernel.unknown-body");
    assert!(m.contains("77"), "{m}");
}

#[test]
fn the_process_is_restarted_on_next_use() {
    let marker = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tier2-restart.marker");
    let _ = std::fs::remove_file(&marker);
    let r = faulty(
        "restart",
        &["crash-once", marker.to_str().unwrap()],
        "",
        true,
    );
    let mut ev = Evaluator::new(r);
    let first = ev.evaluate(&doc("10 mm"));
    assert_eq!(failure(&first).0, "plugin.crash");
    let second = ev.evaluate(&doc("12 mm"));
    assert!(
        matches!(second.outcome(block()), Some(FeatureOutcome::Ok { .. })),
        "{:?}",
        second.events
    );
}

#[test]
fn the_interrupt_stops_the_plugin_at_its_next_kernel_call() {
    let mut ev = Evaluator::new(tier2());
    let interrupt = ev.interrupt();
    interrupt.raise();
    // Stopped inside the feature: no event, nothing cached, no failure.
    assert!(ev.evaluate_while(&doc("10 mm"), |_| true).is_none());
    interrupt.lower();
    let e = ev.evaluate(&doc("10 mm"));
    assert!(
        matches!(e.outcome(block()), Some(FeatureOutcome::Ok { .. })),
        "the restarted process builds: {:?}",
        e.events
    );
}
