//! Cancellation inside a component (plans/c3-m1 step 9, ADR-0035 §7): the
//! evaluator's interrupt stops a Tier 1 feature that loops, through the
//! engine's epoch, within the cancellation budget
//! (docs/CONCURRENCY-WASM.md §Budgets), and as a stop, not as its fuel
//! running out. The session-level twin of `arrix-doc`'s
//! `session/interrupt_tests.rs`, whose stall is a kernel call.

#[path = "support/tier1.rs"]
mod tier1_build;

use std::sync::mpsc::channel;
use std::time::Duration;

use arrix_doc::{
    AuthorId, Command, CommandEnvelope, Evaluator, FeatureEdit, FeatureOutcome, Generation,
    LocalSession, MemorySource, Registry, Replica, Session, SessionEvent, open,
};
use arrix_plugin_api::{FeatureId, Id};
use arrix_plugin_host::register_tier1;
use web_time::Instant;

/// The most a manifest may grant: seconds of a loop, against the
/// milliseconds a stop takes.
const FUEL: u64 = 1 << 36;

/// docs/CONCURRENCY-WASM.md §Budgets.
const BUDGET: Duration = Duration::from_millis(50);

const FINE: &str = "1000000000001";
const LOOP: &str = "1000000000002";

/// A one-part document: a feature that extrudes, then one that loops.
fn document() -> arrix_doc::Document {
    let feature = |id: &str, ty: &str| {
        serde_json::json!({
            "id": id, "inputs": {}, "name": ty, "params": {},
            "suppressed": false, "type_id": ty, "type_version": 1,
        })
    };
    let part = serde_json::json!({
        "features": {
            FINE: feature(FINE, "misbehave.fine"),
            LOOP: feature(LOOP, "misbehave.loop"),
        },
        "history": [FINE, LOOP], "id": "2000000000001",
        "name": "Loop", "rollback": null,
    });
    let mut files = MemorySource::default();
    for (path, json) in [
        ("document.json", serde_json::json!({ "schema": 5 })),
        ("params.json", serde_json::json!({})),
        ("parts/2000000000001.json", part),
    ] {
        files
            .0
            .insert(path.to_owned(), serde_json::to_vec_pretty(&json).unwrap());
    }
    open(&files).unwrap()
}

fn id(s: &str) -> FeatureId {
    FeatureId(s.parse::<Id>().unwrap())
}

fn registry() -> Registry {
    let dir = tier1_build::misbehave("misbehave-cancel", FUEL, true);
    let mut registry = Registry::with_core_types();
    register_tier1(&mut registry, &dir).unwrap();
    registry
}

#[test]
fn the_interrupt_stops_a_looping_component_within_the_budget() {
    let mut evaluator = Evaluator::new(registry());
    let interrupt = evaluator.interrupt();
    let (raised, at) = channel();
    let raiser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(200));
        let _ = raised.send(Instant::now());
        interrupt.raise();
    });
    // The stop is the caller's: the evaluation ends with nothing for the
    // feature it stopped in, where a fuel failure would have been its
    // outcome and the evaluation would have finished.
    let stopped = evaluator.evaluate_while(&document(), |_| true);
    let ended = Instant::now();
    raiser.join().unwrap();
    assert!(stopped.is_none(), "{stopped:?}");
    let latency = ended - at.recv().unwrap();
    assert!(latency <= BUDGET, "stopped {latency:?} after the raise");
}

#[test]
fn a_newer_edit_stops_the_looping_component_and_publishes_no_failure() {
    let s = LocalSession::open(document(), registry()).unwrap();
    let events = s.subscribe();
    let mut replica = Replica::default();
    let next = || {
        events
            .recv_timeout(Duration::from_secs(30))
            .expect("the session publishes within the timeout")
    };
    let mut seen = Vec::new();
    // Generation 0 evaluates `fine`, then runs into the loop.
    loop {
        let e = next();
        replica.receive(&e).unwrap();
        let fine_done =
            matches!(&e, SessionEvent::Evaluated { event, .. } if event.feature == id(FINE));
        seen.push(e);
        if fine_done {
            break;
        }
    }
    std::thread::sleep(Duration::from_millis(100));
    let suppress = Command::EditFeature {
        feature: id(LOOP),
        edit: FeatureEdit {
            suppressed: Some(true),
            ..FeatureEdit::default()
        },
    };
    s.submit(CommandEnvelope {
        author: AuthorId(Id(1)),
        base: Generation(0),
        command: suppress,
    })
    .unwrap();
    while replica.finished().is_none_or(|(g, _)| g < Generation(1)) {
        let e = next();
        replica.receive(&e).unwrap();
        seen.push(e);
    }

    let failed: Vec<_> = seen
        .iter()
        .filter(|e| {
            matches!(e, SessionEvent::Evaluated { event, .. }
                if matches!(event.outcome, FeatureOutcome::Failed { .. }))
        })
        .collect();
    assert!(failed.is_empty(), "a stop is not a failure: {failed:?}");
    assert!(
        !seen.iter().any(
            |e| matches!(e, SessionEvent::Evaluated { generation, event }
            if *generation == Generation(0) && event.feature == id(LOOP))
        ),
        "the feature that was stopped publishes nothing"
    );
    assert!(matches!(
        replica.outcome(id(FINE)),
        Some((Generation(1), FeatureOutcome::Ok { cached: true, .. }))
    ));
}
