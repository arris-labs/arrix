//! Previews through `LocalSession` (ADR-0019): what an extrude previews is
//! what it gives once applied, the document never moves, and a superseded
//! or cleared preview's events stop.

use std::collections::BTreeMap;
use std::time::Duration;

use arrix_core::{FeatureId, Id, MeshDetail, PartId, Ref, RegionKey, RenderMesh};
use arrix_sketch::{Draft, Entity, Point};

use super::*;
use crate::authority::Rejected;
use crate::command::{Command, FeatureEdit};
use crate::document::{FeatureRecord, FeatureTypeId, Part};
use crate::eval::SlotView;
use crate::expr::Expr;
use crate::registry::Registry;

pub(super) const ME: AuthorId = AuthorId(Id(1));
pub(super) const PART: PartId = PartId(Id(10));
pub(super) const SKETCH: FeatureId = FeatureId(Id(11));
pub(super) const PLATE: FeatureId = FeatureId(Id(12));
const MM: f64 = 1e-3;

fn record(id: FeatureId, type_id: &str) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: type_id.into(),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
    }
}

/// A part holding a 40 × 30 mm rectangle sketched on world XY.
fn sketched() -> Command {
    let mut d = Draft::seeded(7);
    let p = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)]
        .map(|(x, y)| d.add_point(Point::new(x * MM, y * MM)));
    for i in 0..4 {
        d.add_entity(Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
    }
    let mut sketch = record(SKETCH, "core.sketch");
    sketch.sketch = Some(Box::new(d.sketch));
    Command::AddPart {
        part: Part {
            id: PART,
            name: "Part".into(),
            history: vec![SKETCH],
            features: [(SKETCH, sketch)].into(),
            rollback: None,
        },
    }
}

pub(super) fn extrude(key: RegionKey, distance: &str) -> Command {
    let mut r = record(PLATE, "core.extrude");
    r.params
        .insert("distance".into(), Expr::parse(distance).unwrap());
    let region = Ref::Region {
        feature: SKETCH,
        key,
    };
    r.inputs.insert("region".into(), region.into());
    Command::AddFeature {
        part: PART,
        at: 1,
        record: r,
    }
}

pub(super) fn envelope(base: u64, command: Command) -> CommandEnvelope {
    CommandEnvelope {
        author: ME,
        base: Generation(base),
        command,
    }
}

fn preview(s: &LocalSession, n: u64, base: u64, command: Command) -> Result<Outcome, Rejected> {
    s.request(Request::Preview {
        preview: PreviewId(n),
        envelope: envelope(base, command),
    })
}

pub(super) fn next(events: &EventStream) -> SessionEvent {
    events
        .recv_timeout(Duration::from_secs(30))
        .expect("the session publishes within the timeout")
}

/// Events until `done` holds of one, which is the last returned.
pub(super) fn until(
    events: &EventStream,
    mut done: impl FnMut(&SessionEvent) -> bool,
) -> Vec<SessionEvent> {
    let mut seen = Vec::new();
    loop {
        let e = next(events);
        let stop = done(&e);
        seen.push(e);
        if stop {
            return seen;
        }
    }
}

fn refined_of(e: &SessionEvent) -> Option<&RenderMesh> {
    match e {
        SessionEvent::Meshed {
            detail: MeshDetail::Refined,
            mesh: Ok(mesh),
            ..
        }
        | SessionEvent::PreviewMeshed {
            detail: MeshDetail::Refined,
            mesh: Ok(mesh),
            ..
        } => Some(mesh),
        _ => None,
    }
}

fn tag_of(e: &SessionEvent) -> Option<PreviewId> {
    match e {
        SessionEvent::Previewed { tag, .. } | SessionEvent::PreviewMeshed { tag, .. } => {
            Some(tag.preview)
        }
        _ => None,
    }
}

/// A session at generation 1 holding the sketch, evaluated, and the key
/// of its region.
pub(super) fn open() -> (LocalSession, EventStream, RegionKey) {
    let s = LocalSession::open(Document::default(), Registry::with_core_types()).unwrap();
    let events = s.subscribe();
    s.submit(envelope(0, sketched())).unwrap();
    let mut replica = Replica::default();
    while replica.finished().is_none_or(|(g, _)| g < Generation(1)) {
        replica.receive(&next(&events)).unwrap();
    }
    let key = match replica.outcome(SKETCH) {
        Some((_, FeatureOutcome::Ok { slots, .. })) => match slots.values().next() {
            Some(SlotView::Sketch(v)) if v.regions.len() == 1 => v.regions[0].key.clone(),
            other => panic!("{other:?}"),
        },
        other => panic!("{other:?}"),
    };
    (s, events, key)
}

/// The plate's outcome with `cached` cleared: an applied command finds
/// the preview's result in the cache.
fn plate(events: &[EvalEvent]) -> FeatureOutcome {
    let e = events.iter().find(|e| e.feature == PLATE).unwrap();
    match e.outcome.clone() {
        FeatureOutcome::Ok {
            hash,
            slots,
            frozen,
            diagnostics,
            ..
        } => FeatureOutcome::Ok {
            hash,
            cached: false,
            slots,
            frozen,
            diagnostics,
        },
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_extrude_previewed_is_what_it_gives_once_applied() {
    let (s, events, key) = open();
    let before = s.save();
    let command = extrude(key, "12 mm");
    assert_eq!(preview(&s, 1, 1, command.clone()), Ok(Outcome::Previewing));
    let seen = until(&events, |e| {
        matches!(
            e,
            SessionEvent::PreviewMeshed {
                detail: MeshDetail::Refined,
                ..
            }
        )
    });
    let Some(SessionEvent::Previewed {
        tag,
        generation,
        events: previewed,
        bodies,
        ..
    }) = seen
        .iter()
        .find(|e| matches!(e, SessionEvent::Previewed { .. }))
    else {
        panic!("{seen:?}");
    };
    assert_eq!(
        *tag,
        PreviewTag {
            author: ME,
            preview: PreviewId(1)
        }
    );
    assert_eq!(*generation, Generation(1));
    assert_eq!(bodies.len(), 1);
    assert!(bodies[0].changed && bodies[0].slot.feature == PLATE);
    let previewed_mesh = seen.iter().find_map(refined_of).unwrap().clone();
    // Nothing moved: no change, and the document saves to the same bytes.
    assert!(
        !seen
            .iter()
            .any(|e| matches!(e, SessionEvent::Applied { .. }))
    );
    assert_eq!(s.save().0, before.0);

    s.submit(envelope(1, command)).unwrap();
    let mut applied = Vec::new();
    let seen = until(&events, |e| {
        if let SessionEvent::Evaluated { event, .. } = e {
            applied.push(event.clone());
        }
        matches!(
            e,
            SessionEvent::Meshed {
                detail: MeshDetail::Refined,
                ..
            }
        )
    });
    assert_eq!(plate(&applied), plate(previewed));
    assert_eq!(seen.iter().find_map(refined_of), Some(&previewed_mesh));
}

#[test]
fn a_preview_of_an_unchanged_body_sends_no_mesh() {
    let (s, events, key) = open();
    s.submit(envelope(1, extrude(key, "12 mm"))).unwrap();
    until(&events, |e| {
        matches!(
            e,
            SessionEvent::Meshed {
                detail: MeshDetail::Refined,
                ..
            }
        )
    });
    // Renaming the plate changes no body.
    let rename = Command::EditFeature {
        feature: PLATE,
        edit: FeatureEdit {
            name: Some("Plate".into()),
            ..FeatureEdit::default()
        },
    };
    preview(&s, 1, 2, rename).unwrap();
    let seen = until(&events, |e| matches!(e, SessionEvent::Previewed { .. }));
    let Some(SessionEvent::Previewed { bodies, .. }) = seen.last() else {
        unreachable!()
    };
    assert_eq!(bodies.len(), 1);
    assert!(!bodies[0].changed);
    // A later commit is the next thing heard: no preview mesh came.
    s.submit(envelope(
        2,
        Command::SetRollback {
            part: PART,
            at: Some(1),
        },
    ))
    .unwrap();
    let after = until(&events, |e| matches!(e, SessionEvent::Applied { .. }));
    assert!(after.iter().all(|e| tag_of(e).is_none()), "{after:?}");
}

#[test]
fn a_superseded_or_cleared_preview_is_heard_no_more() {
    let (s, events, key) = open();
    preview(&s, 1, 1, extrude(key.clone(), "12 mm")).unwrap();
    preview(&s, 2, 1, extrude(key.clone(), "20 mm")).unwrap();
    let seen = until(&events, |e| {
        tag_of(e) == Some(PreviewId(2))
            && matches!(
                e,
                SessionEvent::PreviewMeshed {
                    detail: MeshDetail::Refined,
                    ..
                }
            )
    });
    let first_of_2 = seen
        .iter()
        .position(|e| tag_of(e) == Some(PreviewId(2)))
        .unwrap();
    assert!(
        seen[first_of_2..]
            .iter()
            .all(|e| tag_of(e) != Some(PreviewId(1))),
        "{seen:?}"
    );

    // A clear, then a commit as a marker: nothing of a preview after it.
    preview(&s, 3, 1, extrude(key.clone(), "30 mm")).unwrap();
    assert_eq!(
        s.request(Request::ClearPreview { author: ME }),
        Ok(Outcome::NoOp)
    );
    s.submit(envelope(1, extrude(key, "5 mm"))).unwrap();
    let seen = until(&events, |e| {
        matches!(
            e,
            SessionEvent::Meshed {
                detail: MeshDetail::Refined,
                ..
            }
        )
    });
    let applied = seen
        .iter()
        .position(|e| matches!(e, SessionEvent::Applied { .. }))
        .unwrap();
    assert!(
        seen[applied..].iter().all(|e| tag_of(e).is_none()),
        "{seen:?}"
    );
}

#[test]
fn a_stale_or_refused_preview_is_rejected_as_its_submit_would_be() {
    let (s, _events, key) = open();
    s.submit(envelope(1, extrude(key.clone(), "12 mm")))
        .unwrap();
    let edit = |d: &str| Command::EditFeature {
        feature: PLATE,
        edit: FeatureEdit {
            params: [("distance".into(), Some(Expr::parse(d).unwrap()))].into(),
            ..FeatureEdit::default()
        },
    };
    s.submit(envelope(2, edit("13 mm"))).unwrap();
    assert!(matches!(
        preview(&s, 1, 2, edit("14 mm")),
        Err(Rejected::Stale { .. })
    ));
    assert!(matches!(
        preview(&s, 2, 3, extrude(key, "1 mm")),
        Err(Rejected::Refused(_))
    ));
}

#[test]
fn preview_requests_and_events_cross_as_json() {
    let (s, events, key) = open();
    let requests = [
        Request::Preview {
            preview: PreviewId(1),
            envelope: envelope(1, extrude(key, "12 mm")),
        },
        Request::ClearPreview { author: ME },
    ];
    let mut seen = Vec::new();
    for r in &requests {
        let json = serde_json::to_string(r).unwrap();
        assert_eq!(
            &serde_json::from_str::<Request>(&json).unwrap(),
            r,
            "{json}"
        );
        s.request(serde_json::from_str(&json).unwrap()).unwrap();
        if seen.is_empty() {
            seen = until(&events, |e| {
                matches!(
                    e,
                    SessionEvent::PreviewMeshed {
                        detail: MeshDetail::Refined,
                        ..
                    }
                )
            });
        }
    }
    assert!(
        seen.iter()
            .any(|e| matches!(e, SessionEvent::Previewed { .. }))
    );
    for e in seen.iter().filter(|e| tag_of(e).is_some()) {
        let json = serde_json::to_string(e).unwrap();
        assert_eq!(
            &serde_json::from_str::<SessionEvent>(&json).unwrap(),
            e,
            "{json}"
        );
    }
}
