//! The wire form of the worker's messages (plans/c4 step 6, ADR-0042):
//! a session whose authority and evaluator exchange only serialised
//! messages gives the events, bodies and saved bytes of `LocalSession`,
//! the session suite's scenarios being run again through it.

use std::collections::BTreeMap;
use std::sync::Arc;

use super::tests::{build, envelope, set_h};
use super::wire::{self, Heard, in_loopback};
use super::*;
use crate::authority::Authority;
use crate::eval::{EvalLine, Evaluation};
use crate::frozen::{BlobRef, Frozen};
use crate::registry::Registry;

/// A session driven through `build` and an edit, each generation heard
/// out before the next is asked (a race would make two runs differ): every
/// event, and the files it saves.
fn run() -> (Vec<SessionEvent>, BTreeMap<String, Vec<u8>>) {
    let s = LocalSession::open(Document::default(), Registry::with_core_types()).unwrap();
    let events = s.subscribe();
    let mut replica = Replica::default();
    let mut seen = Vec::new();
    let commands = build().into_iter().chain([set_h("7 mm")]);
    for (base, command) in commands.enumerate() {
        envelope(&s, AuthorId(Id(1)), base as u64, command);
        seen.extend(super::tests::until_finished(
            &events,
            &mut replica,
            base as u64 + 1,
        ));
    }
    (seen, s.save().0)
}

#[test]
fn a_loopback_session_gives_the_events_and_saved_bytes_of_a_local_one() {
    let (plain_events, plain_files) = run();
    let (wired_events, wired_files) = in_loopback(run);
    assert_eq!(wired_events, plain_events);
    assert_eq!(wired_files, plain_files);
}

macro_rules! in_loopback_too {
    ($($name:ident => $path:path),* $(,)?) => {$(
        #[test]
        fn $name() {
            in_loopback($path);
        }
    )*};
}

in_loopback_too! {
    loopback_replica_follows_the_stream => super::tests::a_replica_follows_the_applied_stream_and_the_evaluation,
    loopback_stale_command_is_rejected => super::tests::a_stale_command_is_rejected_through_the_session,
    loopback_evaluation_catches_up => super::tests::the_evaluation_catches_up_with_the_newest_generation,
    loopback_late_subscriber_hears_again => super::tests::a_late_subscriber_hears_the_latest_outcomes_again,
    loopback_extrude_preview_is_its_apply => super::preview_tests::an_extrude_previewed_is_what_it_gives_once_applied,
    loopback_unchanged_body_sends_no_mesh => super::preview_tests::a_preview_of_an_unchanged_body_sends_no_mesh,
    loopback_superseded_preview_is_silent => super::preview_tests::a_superseded_or_cleared_preview_is_heard_no_more,
    loopback_stale_preview_is_rejected => super::preview_tests::a_stale_or_refused_preview_is_rejected_as_its_submit_would_be,
    loopback_export_is_of_the_edit => super::export_tests::an_export_after_an_edit_is_of_the_edited_generation,
    loopback_preview_does_not_reach_the_file => super::export_tests::a_preview_in_between_does_not_reach_the_file,
    loopback_export_changes_nothing => super::export_tests::an_export_changes_nothing_in_the_document,
    loopback_export_without_bodies_says_so => super::export_tests::a_selection_that_is_no_body_and_a_document_with_none_say_so,
    loopback_export_names_its_bodies => super::export_tests::an_export_names_its_bodies_and_the_headless_path_agrees,
    loopback_old_base_expansion_is_stale => super::expand_tests::a_base_older_than_the_newest_generation_is_stale_at_once,
    loopback_step_tree_comes_back => super::step_tree_tests::a_files_tree_comes_back_under_its_tag_and_the_document_stays,
    loopback_non_step_says_why => super::step_tree_tests::a_file_that_is_no_step_says_why,
}

#[test]
fn a_registry_change_has_no_wire_form() {
    let job = preview::Job::Registry(Box::new(Registry::with_core_types()), Default::default());
    assert!(matches!(
        wire::encode_job(&job),
        Err(wire::WireError::Registry)
    ));
}

#[test]
fn a_snapshot_crosses_as_the_document_it_was() {
    let mut local = Authority::default();
    for (base, command) in build().into_iter().enumerate() {
        let envelope = CommandEnvelope {
            author: AuthorId(Id(1)),
            base: Generation(base as u64),
            command,
        };
        local.submit(&envelope).unwrap();
    }
    let job = preview::Job::Commit((local.generation(), Arc::new(local.document().clone())));
    match wire::decode_job(&wire::encode_job(&job).unwrap()).unwrap() {
        preview::Job::Commit((generation, doc)) => {
            assert_eq!(generation, local.generation());
            assert_eq!(*doc, *local.document());
        }
        _ => panic!("a commit decodes as a commit"),
    }
}

#[test]
fn a_frozen_result_crosses_with_its_blob_bytes() {
    let bytes = b"arris body bytes".to_vec();
    let blob = BlobRef::of(&bytes, "arrisbody");
    let names = BlobRef::of(b"{}", "json");
    let result = serde_json::from_value(serde_json::json!({
        "input_hash": "ab".repeat(32),
        "plugin_version": "1.2.3",
        "slots": { "body": blob },
        "names": names,
    }))
    .unwrap();
    let frozen = Arc::new(Frozen {
        result,
        blobs: BTreeMap::from([(blob, bytes), (names, b"{}".to_vec())]),
    });
    let feature = ids_feature();
    let results = BTreeMap::from([(feature, frozen)]);
    match wire::decode_heard(&wire::encode_freeze(&results)).unwrap() {
        Heard::Freeze(back) => assert_eq!(back, results),
        Heard::Publish(_) => panic!("a freeze decodes as a freeze"),
    }
}

fn ids_feature() -> FeatureId {
    arrix_core::IdMinter::new(3).mint()
}

/// The eval line a client reads from a generation's events, in a session
/// with a body: plain, and through the wire form as a browser hears it.
fn heard_line() -> (EvalLine, EvalLine) {
    use super::preview_tests::{envelope, extrude, open, until};
    let (s, events, key) = open();
    s.submit(envelope(1, extrude(key, "4 mm"))).unwrap();
    let seen = until(
        &events,
        |e| matches!(e, SessionEvent::Finished { generation, .. } if *generation == Generation(2)),
    );
    let saved = s.save();
    let doc = crate::open::open(&saved).unwrap();
    let heard = Evaluation::heard(Generation(2), &seen).expect("generation 2 finished");
    let printed = crate::eval::eval("plate", &saved, &Registry::with_core_types()).unwrap();
    (heard.line("plate", &doc), printed)
}

#[test]
fn the_eval_line_read_from_the_events_is_the_one_arrix_eval_prints() {
    for (heard, printed) in [heard_line(), in_loopback(heard_line)] {
        assert!(
            !heard.bodies.is_empty() && heard.mass.is_some(),
            "{heard:?}"
        );
        assert_eq!(
            serde_json::to_string(&heard).unwrap(),
            serde_json::to_string(&printed).unwrap()
        );
    }
}
