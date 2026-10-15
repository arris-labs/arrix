//! Expansion through `LocalSession` (plans/c3-m3 step 5, ADR-0038 §8):
//! the request and its answers cross as JSON, a command the registry lacks
//! is answered rather than dropped, and a base older than the newest
//! generation is refused at once. Expanding with a plugin's command is
//! tested in `arrix-plugin-host`, which has plugins to register.

use arrix_core::{Diagnostic, Id, Severity};

use super::preview_tests::{PART, envelope, open, until};
use super::*;
use crate::command::{Command, FeatureEdit};
use crate::eval::EdgePick;

fn request(base: u64) -> ExpandRequest {
    ExpandRequest {
        command: "nobody.nothing".into(),
        context: ExpandContext::Part(PART),
        params: [("size".to_owned(), "5 mm".to_owned())].into(),
        choices: [("kind".to_owned(), "a".to_owned())].into(),
        inputs: [(
            "edges".to_owned(),
            vec![EdgePick {
                path: vec!["4GM777SG0Q2K8".parse().unwrap()],
                edge: "edge:revolve.0000000000001.side.0000000000002"
                    .parse()
                    .unwrap(),
            }],
        )]
        .into(),
        ids: vec![Id(1), Id(2)],
        base: Generation(base),
    }
}

#[test]
fn a_request_and_its_answers_cross_as_json() {
    crate::session::crosses(&Request::Expand(request(3)), "request");
    // A request that gives nothing optional is the bare minimum.
    let bare = ExpandRequest {
        command: "a.b".into(),
        context: ExpandContext::Assembly("6NDZZ2CNWB0X9".parse().unwrap()),
        params: Default::default(),
        choices: Default::default(),
        inputs: Default::default(),
        ids: Vec::new(),
        base: Generation(0),
    };
    crate::session::crosses(&Request::Expand(bare), "request");
    crate::session::crosses(&Outcome::Expanding, "outcome");
    let answers = [
        Ok(Command::Group {
            label: "Place".into(),
            commands: Vec::new(),
        }),
        Err(Diagnostic::new(Severity::Error, "command.invalid", "bad").unwrap()),
    ];
    for group in answers {
        let event = SessionEvent::Expanded {
            generation: Generation(4),
            group,
        };
        crate::session::crosses(&event, "event");
    }
}

#[test]
fn a_command_the_registry_lacks_is_answered_not_dropped() {
    let (s, events, _) = open();
    assert_eq!(s.expand(request(1)), Ok(Outcome::Expanding));
    let seen = until(&events, |e| matches!(e, SessionEvent::Expanded { .. }));
    match seen.into_iter().last() {
        Some(SessionEvent::Expanded {
            generation,
            group: Err(d),
        }) => {
            assert_eq!(generation, Generation(1));
            assert_eq!(d.code.as_str(), "command.unknown");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
pub(super) fn a_base_older_than_the_newest_generation_is_stale_at_once() {
    let (s, _events, _) = open();
    let rename = Command::EditFeature {
        feature: super::preview_tests::SKETCH,
        edit: FeatureEdit {
            name: Some("Renamed".into()),
            ..FeatureEdit::default()
        },
    };
    assert!(matches!(
        s.submit(envelope(1, rename)),
        Ok(Outcome::Applied(_))
    ));
    let stale = s.expand(request(1));
    assert!(matches!(stale, Err(Rejected::Stale { base, .. }) if base == Generation(1)));
}
