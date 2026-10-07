//! The UI's first document commands: undo, redo and a new document, and
//! the count of commands each gesture submitted (SEED.md §8.2).
//! Logic only.

use arrix_app::ArrixApp;
use arrix_core::{Id, ParamId, QuantityKind};
use arrix_doc::expr::Expr;
use arrix_doc::{Command, MemorySource, Param};
use egui::{Key, Modifiers};
use egui_kittest::Harness;

fn app() -> Harness<'static, ArrixApp> {
    let mut harness = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(|cc| ArrixApp::new(cc));
    harness.run();
    harness
}

fn empty_document() -> MemorySource {
    MemorySource(
        [("document.json".to_owned(), br#"{"schema": 3}"#.to_vec())]
            .into_iter()
            .collect(),
    )
}

fn set_width(text: &str) -> Command {
    Command::AddParam {
        param: ParamId(Id(7)),
        record: Param {
            name: "width".into(),
            kind: QuantityKind::Length,
            expr: Expr::parse(text).unwrap(),
        },
    }
}

fn gesture(h: &Harness<'_, ArrixApp>) -> Option<usize> {
    h.state().debug_state().gesture_commands
}

fn hint(h: &Harness<'_, ArrixApp>) -> String {
    h.state().debug_state().shell.unwrap().status.hint
}

fn press(h: &mut Harness<'_, ArrixApp>, modifiers: Modifiers, key: Key) {
    h.key_press_modifiers(modifiers, key);
    h.run_steps(3);
}

#[test]
fn undo_and_redo_return_the_saved_files_byte_for_byte() {
    let mut h = app();
    h.state_mut().open("empty", &empty_document()).unwrap();
    h.run_steps(3);
    let before = h.state().saved().unwrap();
    assert_eq!(h.state_mut().submit(set_width("60 mm")), Ok(true));
    assert_eq!(gesture(&h), Some(1));
    let after = h.state().saved().unwrap();
    assert_ne!(before.0, after.0);

    press(&mut h, Modifiers::CTRL, Key::Z);
    assert_eq!(h.state().saved().unwrap().0, before.0);
    assert_eq!(gesture(&h), Some(1));
    press(&mut h, Modifiers::CTRL | Modifiers::SHIFT, Key::Z);
    assert_eq!(h.state().saved().unwrap().0, after.0);
    assert_eq!(gesture(&h), Some(1));
    // Ctrl+Y redoes too.
    press(&mut h, Modifiers::CTRL, Key::Z);
    press(&mut h, Modifiers::CTRL, Key::Y);
    assert_eq!(h.state().saved().unwrap().0, after.0);
    assert_eq!(gesture(&h), Some(1));
}

#[test]
fn a_gesture_that_changes_nothing_counts_zero_and_says_why() {
    let mut h = app();
    h.state_mut().open("empty", &empty_document()).unwrap();
    h.run_steps(3);
    assert_eq!(gesture(&h), None, "no gesture yet");

    press(&mut h, Modifiers::CTRL, Key::Z);
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(hint(&h), "nothing to undo");
    press(&mut h, Modifiers::CTRL | Modifiers::SHIFT, Key::Z);
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(hint(&h), "nothing to redo");

    assert_eq!(h.state_mut().submit(set_width("60 mm")), Ok(true));
    h.run_steps(3);
    assert!(!hint(&h).starts_with("nothing to"), "{}", hint(&h));
    let saved = h.state().saved().unwrap().0;
    // The same value again changes nothing: no command, no undo entry.
    let same = Command::SetParam {
        param: ParamId(Id(7)),
        expr: Expr::parse("60 mm").unwrap(),
    };
    assert_eq!(h.state_mut().submit(same), Ok(false));
    assert_eq!(gesture(&h), Some(0));
    // A refused command says why and changes nothing.
    assert!(h.state_mut().submit(set_width("60 mm")).is_err());
    assert_eq!(gesture(&h), Some(0));
    assert_eq!(h.state().saved().unwrap().0, saved);
}

#[test]
fn a_new_document_is_an_empty_tab() {
    let mut h = app();
    press(&mut h, Modifiers::CTRL, Key::N);
    let shell = h.state().debug_state().shell.unwrap();
    assert_eq!(shell.documents.tabs.len(), 1);
    assert!(shell.documents.tabs[0].active);
    assert!(h.state().saved().unwrap().0.contains_key("document.json"));
    assert_eq!(gesture(&h), Some(0));
    press(&mut h, Modifiers::CTRL, Key::N);
    let tabs = h.state().debug_state().shell.unwrap().documents.tabs;
    assert_eq!(tabs.len(), 2);
    assert!(tabs[1].active);
}
