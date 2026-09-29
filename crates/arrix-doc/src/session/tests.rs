use std::collections::BTreeMap;
use std::time::Duration;

use arrix_core::{Id, IdMinter, PartId, QuantityKind, Ref, SlotName};

use super::*;
use crate::authority::{Authority, Change};
use crate::command::Command;
use crate::document::{FeatureRecord, FeatureTypeId, Param, Part};
use crate::eval::{FeatureOutcome, SlotView};
use crate::expr::Expr;
use crate::registry::Registry;

const ME: AuthorId = AuthorId(Id(1));
const THEM: AuthorId = AuthorId(Id(2));

/// The ids a test client mints: a fixed seed, as every test passes one.
struct Ids {
    part: PartId,
    h: ParamId,
    low: FeatureId,
    high: FeatureId,
}

fn ids() -> Ids {
    let mut m = IdMinter::new(7);
    Ids {
        part: m.mint(),
        h: m.mint(),
        low: m.mint(),
        high: m.mint(),
    }
}

fn plane(id: FeatureId, offset: &str, on: Option<FeatureId>) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new("core.datum-plane").unwrap(),
        type_version: 1,
        name: format!("plane {id}"),
        params: BTreeMap::from([("offset".into(), Expr::parse(offset).unwrap())]),
        choices: BTreeMap::new(),
        inputs: on
            .map(|f| {
                let slot = SlotName::new("plane").unwrap();
                ("plane".into(), Ref::Slot { feature: f, slot })
            })
            .into_iter()
            .collect(),
        suppressed: false,
        sketch: None,
        frozen: None,
    }
}

fn set_h(expr: &str) -> Command {
    Command::SetParam {
        param: ids().h,
        expr: Expr::parse(expr).unwrap(),
    }
}

/// `h = 5 mm`, a part, a plane `h` above world XY and a plane 2 mm above
/// that.
fn build() -> Vec<Command> {
    let i = ids();
    let record = Param {
        name: "h".into(),
        kind: QuantityKind::Length,
        expr: Expr::parse("5 mm").unwrap(),
    };
    let part = Part {
        id: i.part,
        name: "Part".into(),
        history: vec![],
        features: BTreeMap::new(),
        rollback: None,
    };
    vec![
        Command::AddParam { param: i.h, record },
        Command::AddPart { part },
        Command::AddFeature {
            part: i.part,
            at: 0,
            record: plane(i.low, "h", None),
        },
        Command::AddFeature {
            part: i.part,
            at: 1,
            record: plane(i.high, "2 mm", Some(i.low)),
        },
    ]
}

fn envelope(s: &LocalSession, author: AuthorId, base: u64, command: Command) -> Outcome {
    let e = CommandEnvelope {
        author,
        base: Generation(base),
        command,
    };
    s.submit(e).unwrap()
}

fn next(events: &EventStream) -> SessionEvent {
    events
        .recv_timeout(Duration::from_secs(30))
        .expect("the session publishes within the timeout")
}

/// Follows `events` until the evaluation of `generation` finished, and
/// returns every event on the way.
fn until_finished(
    events: &EventStream,
    replica: &mut Replica,
    generation: u64,
) -> Vec<SessionEvent> {
    let mut seen = Vec::new();
    while replica
        .finished()
        .is_none_or(|(g, _)| g < Generation(generation))
    {
        let e = next(events);
        replica.receive(&e).unwrap();
        seen.push(e);
    }
    seen
}

fn z(replica: &Replica, feature: FeatureId) -> f64 {
    match replica.outcome(feature) {
        Some((_, FeatureOutcome::Ok { slots, .. })) => match slots.values().next() {
            Some(SlotView::Plane(f)) => f.origin().z,
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_replica_follows_the_applied_stream_and_the_evaluation() {
    let s = LocalSession::open(Document::default(), Registry::with_core_types()).unwrap();
    let events = s.subscribe();
    let mut replica = Replica::default();
    let mut local = Authority::default();
    for (g, c) in build().into_iter().chain([set_h("7 mm")]).enumerate() {
        let submitted = CommandEnvelope {
            author: ME,
            base: Generation(g as u64),
            command: c,
        };
        assert_eq!(s.submit(submitted.clone()), local.submit(&submitted));
    }
    assert_eq!(s.undo(ME), local.undo(ME));
    assert_eq!(s.redo(ME), local.redo(ME));
    until_finished(&events, &mut replica, 7);
    assert_eq!(replica.document(), local.document());
    assert_eq!(replica.generation(), Generation(7));
    assert!((z(&replica, ids().high) - 0.009).abs() < 1e-15);

    // A late subscriber starts from the document as it is now.
    let mut late = Replica::default();
    late.receive(&next(&s.subscribe())).unwrap();
    assert_eq!(late.document(), local.document());
    assert_eq!(late.generation(), Generation(7));
}

#[test]
fn a_stale_command_is_rejected_through_the_session() {
    let s = LocalSession::open(Document::default(), Registry::with_core_types()).unwrap();
    for (g, c) in build().into_iter().enumerate() {
        envelope(&s, ME, g as u64, c);
    }
    envelope(&s, ME, 4, set_h("6 mm"));
    let e = CommandEnvelope {
        author: THEM,
        base: Generation(4),
        command: set_h("9 mm"),
    };
    assert!(matches!(s.submit(e), Err(Rejected::Stale { .. })));
    assert!(matches!(s.undo(THEM), Err(Rejected::NothingToUndo)));
}

#[test]
fn every_request_and_event_crosses_as_json() {
    let s = LocalSession::open(Document::default(), Registry::with_core_types()).unwrap();
    let events = s.subscribe();
    let mut requests = Vec::new();
    let mut commands = build();
    // A failed feature, so a failure crosses too.
    let mut bad = plane(IdMinter::new(99).mint(), "1 mm", None);
    bad.choices.insert("world".into(), "up".into());
    commands.push(Command::AddFeature {
        part: ids().part,
        at: 2,
        record: bad,
    });
    for (g, command) in commands.into_iter().enumerate() {
        requests.push(Request::Submit(CommandEnvelope {
            author: ME,
            base: Generation(g as u64),
            command,
        }));
    }
    requests.push(Request::Undo { author: ME });
    requests.push(Request::Redo { author: ME });
    for r in &requests {
        let json = serde_json::to_string(r).unwrap();
        assert_eq!(
            &serde_json::from_str::<Request>(&json).unwrap(),
            r,
            "{json}"
        );
        s.request(serde_json::from_str(&json).unwrap()).unwrap();
    }
    let mut replica = Replica::default();
    let seen = until_finished(&events, &mut replica, 7);
    let kinds = |k: &str| {
        seen.iter()
            .filter(|e| serde_json::to_value(e).unwrap()["kind"] == k)
            .count()
    };
    assert_eq!(kinds("opened"), 1);
    assert_eq!(kinds("applied"), 7);
    assert!(kinds("evaluated") >= 3 && kinds("finished") >= 1);
    assert!(seen.iter().any(|e| matches!(
        e,
        SessionEvent::Evaluated {
            event: EvalEvent {
                outcome: FeatureOutcome::Failed { .. },
                ..
            },
            ..
        }
    )));
    for e in &seen {
        let json = serde_json::to_string(e).unwrap();
        assert_eq!(
            &serde_json::from_str::<SessionEvent>(&json).unwrap(),
            e,
            "{json}"
        );
    }
}

#[test]
fn the_evaluation_catches_up_with_the_newest_generation() {
    let s = LocalSession::open(Document::default(), Registry::with_core_types()).unwrap();
    let events = s.subscribe();
    for (g, c) in build().into_iter().enumerate() {
        envelope(&s, ME, g as u64, c);
    }
    for mm in 1..=40 {
        envelope(&s, ME, 3 + mm, set_h(&format!("{mm} mm")));
    }
    let mut replica = Replica::default();
    let seen = until_finished(&events, &mut replica, 44);
    assert!((z(&replica, ids().high) - 0.042).abs() < 1e-15);
    let generations: Vec<Generation> = seen
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Evaluated { generation, .. } => Some(*generation),
            _ => None,
        })
        .collect();
    assert!(
        generations.windows(2).all(|w| w[0] <= w[1]),
        "the evaluator only moves forward"
    );
}

#[test]
fn a_replica_keeps_the_newest_outcome_and_refuses_a_gap() {
    let mut r = Replica::default();
    let event = |g: u64, outcome: FeatureOutcome| SessionEvent::Evaluated {
        generation: Generation(g),
        event: EvalEvent {
            feature: ids().low,
            outcome,
        },
    };
    r.receive(&event(3, FeatureOutcome::Suppressed)).unwrap();
    r.receive(&event(2, FeatureOutcome::RolledBack)).unwrap();
    assert_eq!(
        r.outcome(ids().low),
        Some((Generation(3), &FeatureOutcome::Suppressed))
    );
    let change = Change {
        generation: Generation(2),
        author: ME,
        command: set_h("1 mm"),
        touched: Default::default(),
    };
    let gap = r.receive(&SessionEvent::Applied { change, hash: None });
    assert_eq!(
        gap,
        Err(ReplicaError::Gap {
            expected: Generation(1),
            got: Generation(2)
        })
    );
}

#[test]
fn a_replica_that_differs_from_the_authority_says_so() {
    let mut r = Replica::default();
    let command = build().remove(0);
    let mut other = Authority::default();
    other
        .submit(&CommandEnvelope {
            author: ME,
            base: Generation(0),
            command: command.clone(),
        })
        .unwrap();
    let change = Change {
        generation: Generation(1),
        author: ME,
        command,
        touched: Default::default(),
    };
    let wrong = DocHash::of(&Document::default());
    let right = DocHash::of(other.document());
    let mut first = r.clone();
    assert_eq!(
        first.receive(&SessionEvent::Applied {
            change: change.clone(),
            hash: Some(wrong)
        }),
        if cfg!(debug_assertions) {
            Err(ReplicaError::Diverged(Generation(1)))
        } else {
            Ok(())
        }
    );
    r.receive(&SessionEvent::Applied {
        change,
        hash: Some(right),
    })
    .unwrap();
    assert_eq!(r.document(), other.document());
}
