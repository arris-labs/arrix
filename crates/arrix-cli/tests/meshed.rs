//! `SessionEvent::Meshed` on the M1 slice (plans/viewport-port step 4):
//! the evaluator meshes a part's final bodies, coarse for all and then
//! refined, reuses the meshes of a cached version, and never meshes an
//! intermediate or consumed one. Here because the slice needs `gears`.

mod common;

use std::collections::BTreeSet;
use std::time::Duration;
// `arrix-cli` is never built for wasm, so the std clock is fine here
// (docs/CONCURRENCY-WASM.md §The browser build).
#[allow(clippy::disallowed_types, reason = "native-only measurement clock")]
use std::time::Instant;

use arrix_core::{MeshDetail, ParamId};
use arrix_doc::{
    AuthorId, Command, CommandEnvelope, Document, Evaluator, FeatureOutcome, Generation,
    LocalSession, MemorySource, Replica, Session, SessionEvent, SlotView, expr::Expr, open,
};
use arrix_kernel::KernelOp;
use common::{read_tree, registry, workspace};

fn slice() -> (MemorySource, Document) {
    let mut files = std::collections::BTreeMap::new();
    read_tree(&workspace().join("tests/docs/slice"), "", &mut files);
    let source = MemorySource(files);
    let doc = open(&source).unwrap();
    (source, doc)
}

fn tessellations(e: &Evaluator) -> usize {
    e.kernel_calls()
        .iter()
        .filter(|c| matches!(c.op, KernelOp::Tessellate { .. }))
        .count()
}

/// Bodies the slice's evaluation holds in any slot, and the ones it ends
/// with: the plate is modified by the join and the pocket, the gear
/// consumed by the join.
#[test]
fn only_the_final_bodies_are_meshed_and_a_cached_version_keeps_its_mesh() {
    let (_, doc) = slice();
    let mut e = Evaluator::new(registry());
    let evaluation = e.evaluate(&doc);
    let body_slots = evaluation
        .events
        .iter()
        .map(|ev| match &ev.outcome {
            FeatureOutcome::Ok { slots, .. } => slots
                .values()
                .filter(|s| matches!(s, SlotView::Body(_)))
                .count(),
            _ => 0,
        })
        .sum::<usize>();
    let finals = e.final_bodies().to_vec();
    println!("{body_slots} body slots, {} final", finals.len());
    assert!(
        body_slots > finals.len(),
        "intermediate and consumed versions exist"
    );
    assert_eq!(
        finals.len(),
        1,
        "the pocketed plate is all the slice ends with"
    );

    for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
        for f in &finals {
            let mesh = e.mesh(f, detail).unwrap();
            assert!(!mesh.triangles.is_empty() && !mesh.faces.is_empty());
        }
    }
    assert_eq!(tessellations(&e), 2 * finals.len(), "only the final bodies");

    // The same document again: every feature cached, so every mesh is.
    e.evaluate(&doc);
    assert_eq!(e.final_bodies(), finals.as_slice());
    for detail in [MeshDetail::Coarse, MeshDetail::Refined] {
        for f in &finals {
            e.mesh(f, detail).unwrap();
        }
    }
    assert_eq!(tessellations(&e), 2 * finals.len(), "meshes are reused");
}

/// The evaluation of the part, as far as the session's stream shows it.
fn hear(
    events: &arrix_doc::EventStream,
    replica: &mut Replica,
    generation: Generation,
    mut on: impl FnMut(&SessionEvent),
) -> Vec<SessionEvent> {
    let mut seen = Vec::new();
    let mut refined = BTreeSet::new();
    let mut coarse = BTreeSet::new();
    loop {
        let finished = replica.finished().is_some_and(|(g, _)| g >= generation);
        if finished && !coarse.is_empty() && coarse == refined {
            return seen;
        }
        let e = events
            .recv_timeout(Duration::from_secs(60))
            .expect("the session publishes within the timeout");
        replica.receive(&e).unwrap();
        on(&e);
        if let SessionEvent::Meshed {
            generation: g,
            slot,
            detail,
            ..
        } = &e
            && *g == generation
        {
            match detail {
                MeshDetail::Coarse => coarse.insert(slot.clone()),
                MeshDetail::Refined => refined.insert(slot.clone()),
            };
        }
        seen.push(e);
    }
}

#[test]
fn the_session_streams_coarse_then_refined_and_a_width_edit_meshes_again() {
    let (_, doc) = slice();
    let w: ParamId = doc.param_by_name("w").unwrap().0;
    let session = LocalSession::open(doc, registry()).unwrap();
    let events = session.subscribe();
    let mut replica = Replica::default();
    let seen = hear(&events, &mut replica, Generation(0), |_| {});
    let meshed: Vec<_> = seen
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Meshed { detail, mesh, .. } => Some((*detail, mesh)),
            _ => None,
        })
        .collect();
    assert_eq!(meshed.len(), 2);
    assert_eq!(meshed[0].0, MeshDetail::Coarse);
    assert_eq!(meshed[1].0, MeshDetail::Refined);
    let after_finished = seen
        .iter()
        .skip_while(|e| !matches!(e, SessionEvent::Finished { .. }))
        .count();
    assert!(after_finished >= 3, "meshes follow the evaluation");
    for (_, mesh) in &meshed {
        let mesh = mesh.as_ref().unwrap();
        assert!(mesh.faces.len() > 6, "the plate, joined with the gear");
    }

    // A width edit: the first coarse mesh of the new generation is timed.
    let start = Instant::now();
    session
        .submit(CommandEnvelope {
            author: AuthorId(arrix_core::Id(1)),
            base: replica.generation(),
            command: Command::SetParam {
                param: w,
                expr: Expr::parse("50 mm").unwrap(),
            },
        })
        .unwrap();
    let mut first_coarse = None;
    let seen = hear(&events, &mut replica, Generation(1), |e| {
        if first_coarse.is_none()
            && matches!(
                e,
                SessionEvent::Meshed {
                    generation: Generation(1),
                    detail: MeshDetail::Coarse,
                    ..
                }
            )
        {
            first_coarse = Some(start.elapsed());
        }
    });
    println!(
        "first coarse mesh after a width edit: {:.1} ms (budget 150 ms)",
        first_coarse.unwrap().as_secs_f64() * 1e3
    );
    let refined: Vec<_> = seen
        .iter()
        .filter(|e| {
            matches!(
                e,
                SessionEvent::Meshed {
                    generation: Generation(1),
                    detail: MeshDetail::Refined,
                    ..
                }
            )
        })
        .collect();
    assert_eq!(refined.len(), 1);

    for e in &seen {
        let json = serde_json::to_string(e).unwrap();
        assert_eq!(
            &serde_json::from_str::<SessionEvent>(&json).unwrap(),
            e,
            "{json}"
        );
    }
}
