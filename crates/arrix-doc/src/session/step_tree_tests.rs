//! `Request::ReadStepTree` through `LocalSession` (plans/c2-m3 step 8,
//! ADR-0032): a file's tree is read on the worker and comes back under the
//! request's tag; a file that is no STEP says why; the document does not
//! move; the request and its answer cross as JSON.

use super::preview_tests::{open, until};
use super::*;

fn arm() -> Vec<u8> {
    include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/fixtures/step/arm.step"
    ))
    .to_vec()
}

fn tree(events: &EventStream, tag: u64) -> Result<StepTree, Diagnostic> {
    let seen = until(
        events,
        |e| matches!(e, SessionEvent::StepTree { tag: t, .. } if *t == tag),
    );
    match seen.into_iter().last() {
        Some(SessionEvent::StepTree { tree, .. }) => tree,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_files_tree_comes_back_under_its_tag_and_the_document_stays() {
    let (s, events, _) = open();
    let before = s.save();
    let request = Request::ReadStepTree {
        tag: 7,
        bytes: arm(),
    };
    let json = crate::session::crosses(&request, "request");
    assert_eq!(
        s.request(serde_json::from_str(&json).unwrap()),
        Ok(Outcome::Reading)
    );
    let tree = tree(&events, 7).unwrap();
    let [arm] = &tree.roots[..] else {
        panic!("{tree:?}")
    };
    assert_eq!(arm.name, "arm");
    assert_eq!(tree.solids.len(), 4);
    assert!(tree.solids.iter().all(Option::is_none));
    assert_eq!(s.save().0, before.0, "reading a file changes nothing");
    crate::session::crosses(
        &SessionEvent::StepTree {
            tag: 7,
            tree: Ok(tree),
        },
        "event",
    );
}

#[test]
fn a_file_that_is_no_step_says_why() {
    let (s, events, _) = open();
    s.read_step_tree(3, b"nothing".to_vec()).unwrap();
    let err = tree(&events, 3).unwrap_err();
    assert_eq!(err.code.as_str(), "kernel.import.parse");
    crate::session::crosses(
        &SessionEvent::StepTree {
            tag: 3,
            tree: Err(err),
        },
        "event",
    );
}
