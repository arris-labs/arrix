//! Export through `LocalSession` (plans/c1-m4 step 5): the file is of the
//! newest commit however a preview or an edit came before it, the document
//! never moves, and a request and its answer cross as JSON.

use arrix_core::{ExportFormat, FeatureId, Severity};

use super::preview_tests::{PLATE, envelope, extrude, open, until};
use super::*;
use crate::command::{Command, FeatureEdit};
use crate::expr::Expr;
use crate::registry::Registry;

fn distance(mm: &str) -> Command {
    Command::EditFeature {
        feature: PLATE,
        edit: FeatureEdit {
            params: [("distance".into(), Some(Expr::parse(mm).unwrap()))].into(),
            ..FeatureEdit::default()
        },
    }
}

/// The next `Exported` event: its generation and file.
fn exported(events: &EventStream) -> (Generation, Result<Vec<u8>, Diagnostic>) {
    let seen = until(events, |e| matches!(e, SessionEvent::Exported { .. }));
    match seen.into_iter().last() {
        Some(SessionEvent::Exported {
            generation,
            format: ExportFormat::Obj,
            bytes,
        }) => (generation, bytes),
        other => panic!("{other:?}"),
    }
}

/// The tallest point of an OBJ file, in metres.
fn height(obj: &[u8]) -> f64 {
    String::from_utf8(obj.to_vec())
        .unwrap()
        .lines()
        .filter_map(|l| l.strip_prefix("v "))
        .map(|l| l.split_whitespace().nth(2).unwrap().parse::<f64>().unwrap())
        .fold(f64::MIN, f64::max)
}

fn export(s: &LocalSession) -> Result<Outcome, Rejected> {
    s.export(vec![], ExportFormat::Obj)
}

#[test]
fn an_export_after_an_edit_is_of_the_edited_generation() {
    let (s, events, key) = open();
    s.submit(envelope(1, extrude(key, "12 mm"))).unwrap();
    assert_eq!(export(&s), Ok(Outcome::Exporting));
    let (generation, bytes) = exported(&events);
    assert_eq!(generation, Generation(2));
    assert!((height(&bytes.unwrap()) - 0.012).abs() < 1e-12);

    // Asked at once, with nothing awaited between: the edit comes first.
    s.submit(envelope(2, distance("20 mm"))).unwrap();
    export(&s).unwrap();
    let (generation, bytes) = exported(&events);
    assert_eq!(generation, Generation(3));
    assert!((height(&bytes.unwrap()) - 0.020).abs() < 1e-12);
}

#[test]
fn a_preview_in_between_does_not_reach_the_file() {
    let (s, events, key) = open();
    s.submit(envelope(1, extrude(key, "12 mm"))).unwrap();
    let preview = Request::Preview {
        preview: PreviewId(1),
        envelope: envelope(2, distance("30 mm")),
    };
    assert_eq!(s.request(preview), Ok(Outcome::Previewing));
    // The preview is evaluated on the worker before the export is asked.
    until(&events, |e| matches!(e, SessionEvent::Previewed { .. }));
    export(&s).unwrap();
    let (generation, bytes) = exported(&events);
    assert_eq!(generation, Generation(2));
    assert!((height(&bytes.unwrap()) - 0.012).abs() < 1e-12);
}

#[test]
fn an_export_changes_nothing_in_the_document() {
    let (s, events, key) = open();
    s.submit(envelope(1, extrude(key, "12 mm"))).unwrap();
    let before = s.save();
    for format in ExportFormat::ALL {
        s.export(vec![], format).unwrap();
        until(&events, |e| matches!(e, SessionEvent::Exported { .. }));
    }
    assert_eq!(s.save().0, before.0);
    // No generation was made: the next command is still based on 2.
    s.submit(envelope(2, distance("5 mm"))).unwrap();
}

#[test]
fn a_selection_that_is_no_body_and_a_document_with_none_say_so() {
    let (s, events, _) = open();
    // Generation 1 holds a sketch and no body.
    export(&s).unwrap();
    let (_, bytes) = exported(&events);
    assert_eq!(bytes.unwrap_err().code.as_str(), "export.no-bodies");

    let nothing = BodySlot {
        feature: FeatureId(arrix_core::Id(999)),
        slot: arrix_core::SlotName::new("body").unwrap(),
    };
    s.export(vec![nothing], ExportFormat::Obj).unwrap();
    let (_, bytes) = exported(&events);
    assert_eq!(bytes.unwrap_err().code.as_str(), "export.unknown-body");
}

#[test]
fn an_export_names_its_bodies_and_the_headless_path_agrees() {
    let (s, events, key) = open();
    s.submit(envelope(1, extrude(key, "12 mm"))).unwrap();
    let only = BodySlot {
        feature: PLATE,
        slot: arrix_core::SlotName::new("body").unwrap(),
    };
    s.export(vec![only.clone()], ExportFormat::Obj).unwrap();
    let (_, named) = exported(&events);
    export(&s).unwrap();
    let (_, all) = exported(&events);
    assert_eq!(named, all, "the one body is every body");

    let file = crate::export(
        "doc",
        &s.save(),
        &Registry::with_core_types(),
        &[only],
        ExportFormat::Obj,
    )
    .unwrap();
    assert_eq!(
        Ok(file.bytes),
        all,
        "headless and session write the same file"
    );
    assert_eq!(file.line.status, crate::EvalStatus::Ok);
}

#[test]
fn a_request_and_its_answer_cross_as_json() {
    let request = Request::Export {
        bodies: vec![BodySlot {
            feature: PLATE,
            slot: arrix_core::SlotName::new("body").unwrap(),
        }],
        format: ExportFormat::Step,
    };
    crate::session::crosses(&request, "request");

    let answers = [
        Ok(b"ISO-10303-21;".to_vec()),
        Err(Diagnostic::new(Severity::Error, "export.no-bodies", "none").unwrap()),
    ];
    for bytes in answers {
        let event = SessionEvent::Exported {
            generation: Generation(4),
            format: ExportFormat::Stl,
            bytes,
        };
        crate::session::crosses(&event, "event");
    }
}
