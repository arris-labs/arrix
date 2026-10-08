//! A superseded evaluation stops inside its kernel call (docs/CONCURRENCY-
//! WASM.md §The evaluator): the feature it stopped in publishes nothing,
//! and the evaluation that replaces it finds the features the edit kept
//! in the cache. `test.stall` makes kernel calls until one is stopped.

use std::collections::BTreeMap;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arrix_core::{
    CurveKey, DVec2, Diagnostic, FeatureId, Frame, Id, PartId, Profile, ProfileLoop,
    ProfileSegment, QuantityKind,
};
use arrix_plugin_api::{FeatureTypeSpec, Kernel, ParamSpec};

use super::*;
use crate::command::{Command, FeatureEdit};
use crate::document::{FeatureRecord, FeatureTypeId, Part};
use crate::expr::Expr;
use crate::registry::{FeatureArgs, FeatureType, Registry, TypeOutput};

const ME: AuthorId = AuthorId(Id(1));
const PART: PartId = PartId(Id(10));
const PLANE: FeatureId = FeatureId(Id(11));
const STALL: FeatureId = FeatureId(Id(12));

/// With `stall` above zero it extrudes until the kernel stops it, and
/// says so on `started` once it has begun; at zero it does nothing.
struct Stall {
    spec: FeatureTypeSpec,
    started: Mutex<Sender<()>>,
}

impl FeatureType for Stall {
    fn spec(&self) -> &FeatureTypeSpec {
        &self.spec
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        if args.param("stall").unwrap() == 0.0 {
            return Ok(TypeOutput::default());
        }
        let line = |k: u64, x: f64, y: f64| ProfileSegment::Line {
            key: CurveKey(Id(k)),
            to: DVec2::new(x, y),
        };
        let outer = ProfileLoop::Path {
            start: DVec2::ZERO,
            segments: vec![
                line(1, 1.0, 0.0),
                line(2, 1.0, 1.0),
                line(3, 0.0, 1.0),
                line(4, 0.0, 0.0),
            ],
        };
        let profile = Profile::new(Frame::WORLD_XY, outer, vec![]).unwrap();
        let _ = self.started.lock().unwrap().send(());
        loop {
            kernel.extrude(&profile, 1.0)?;
        }
    }
}

fn registry(started: Sender<()>) -> Registry {
    let mut r = Registry::with_core_types();
    let spec = FeatureTypeSpec {
        id: "test.stall".into(),
        version: 1,
        title: "Stall".into(),
        params: vec![ParamSpec {
            name: "stall".into(),
            title: "Stall".into(),
            kind: QuantityKind::Length,
            default: "0 mm".into(),
            shown_when: None,
        }],
        inputs: vec![],
        outputs: vec![],
        choices: vec![],
    };
    let stall = Stall {
        spec,
        started: Mutex::new(started),
    };
    r.register(Arc::new(stall)).unwrap();
    r
}

fn record(id: FeatureId, type_id: &str, params: &[(&str, &str)]) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: type_id.into(),
        params: params
            .iter()
            .map(|(k, v)| ((*k).into(), Expr::parse(v).unwrap()))
            .collect(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
    }
}

fn stall_to(mm: &str) -> Command {
    Command::EditFeature {
        feature: STALL,
        edit: FeatureEdit {
            params: [("stall".into(), Some(Expr::parse(mm).unwrap()))].into(),
            ..FeatureEdit::default()
        },
    }
}

fn submit(s: &LocalSession, base: u64, command: Command) {
    let envelope = CommandEnvelope {
        author: ME,
        base: Generation(base),
        command,
    };
    s.submit(envelope).unwrap();
}

#[test]
fn a_newer_edit_stops_the_feature_running_and_hears_only_its_own_events() {
    let (started, hear) = channel();
    let s = LocalSession::open(Document::default(), registry(started)).unwrap();
    let events = s.subscribe();
    let mut replica = Replica::default();
    let part = Part {
        id: PART,
        name: "Part".into(),
        history: vec![PLANE, STALL],
        features: [
            (
                PLANE,
                record(PLANE, "core.datum-plane", &[("offset", "2 mm")]),
            ),
            (STALL, record(STALL, "test.stall", &[])),
        ]
        .into(),
        rollback: None,
    };
    submit(&s, 0, Command::AddPart { part });
    super::tests::until_finished(&events, &mut replica, 1);

    // The stall runs until the kernel stops it, and nothing else would.
    submit(&s, 1, stall_to("1 mm"));
    hear.recv_timeout(Duration::from_secs(30))
        .expect("the stalling feature began");
    submit(&s, 2, stall_to("0 mm"));
    let seen = super::tests::until_finished(&events, &mut replica, 3);

    let evaluated: Vec<_> = seen
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Evaluated { generation, event } => Some((*generation, event)),
            _ => None,
        })
        .collect();
    assert!(
        !evaluated
            .iter()
            .any(|(g, e)| *g == Generation(2) && e.feature == STALL),
        "the feature that was stopped publishes nothing: {evaluated:?}"
    );
    assert!(
        !seen.iter().any(|e| matches!(
            e,
            SessionEvent::Evaluated {
                event: EvalEvent {
                    outcome: FeatureOutcome::Failed { .. },
                    ..
                },
                ..
            }
        )),
        "a stop is not a failure"
    );
    let plane = evaluated
        .iter()
        .find(|(g, e)| *g == Generation(3) && e.feature == PLANE)
        .expect("the newer generation evaluates the plane");
    assert!(
        matches!(plane.1.outcome, FeatureOutcome::Ok { cached: true, .. }),
        "the feature the edit kept comes from the cache"
    );
    assert!(matches!(
        replica.outcome(STALL),
        Some((Generation(3), FeatureOutcome::Ok { .. }))
    ));
}
