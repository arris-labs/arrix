//! The Assembly tab's commands (plans/c2-m2 step 7, ADR-0030 §6): the import
//! is one gesture and one command, undone as one, and a cancelled dialog
//! does nothing; a new assembly is entered. Logic only, through the palette
//! as a user would.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use arrix_app::{ArrixApp, OpenDialog};
use egui::Key;
use egui_kittest::Harness;

struct Stub(Rc<RefCell<Option<PathBuf>>>);

impl OpenDialog for Stub {
    fn choose(&mut self) -> Option<PathBuf> {
        self.0.borrow().clone()
    }
}

fn palette(h: &mut Harness<'_, ArrixApp>, query: &str) {
    h.key_press_modifiers(egui::Modifiers::CTRL, Key::K);
    h.run_steps(2);
    h.event(egui::Event::Text(query.to_owned()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

/// The commands each gesture applied, in order, those of none left out.
fn applied(h: &Harness<'_, ArrixApp>) -> Vec<usize> {
    let log = h.state().debug_state().gestures;
    log.iter().map(|g| g.commands).filter(|n| *n > 0).collect()
}

fn parts(h: &Harness<'_, ArrixApp>) -> Vec<String> {
    let tree = h.state().debug_state().shell.expect("a frame ran").tree;
    tree.parts.into_iter().map(|p| p.name).collect()
}

#[test]
fn importing_and_making_an_assembly() {
    let mut h = Harness::builder()
        .with_size(egui::vec2(1440.0, 900.0))
        .build_eframe(|cc| ArrixApp::new(cc));
    h.run();
    let answer = Rc::new(RefCell::new(None));
    h.state_mut()
        .set_open_dialog(Box::new(Stub(answer.clone())));
    h.key_press_modifiers(egui::Modifiers::CTRL, Key::N);
    h.run_steps(3);
    assert!(parts(&h).is_empty());

    // A cancelled dialog does nothing.
    palette(&mut h, "Import STEP");
    assert!(parts(&h).is_empty() && applied(&h).is_empty());

    // The import is one command, and the tab looks at the new part (`context` shows only with an
    // assembly, so the entry is the assembly's below).
    *answer.borrow_mut() = Some(PathBuf::from("../../tests/fixtures/step/servo.step"));
    palette(&mut h, "Import STEP");
    // The session reads the file's tree on its worker, and the one command
    // follows when it comes back.
    let started = web_time::Instant::now();
    while parts(&h).is_empty() {
        h.step();
        assert!(started.elapsed().as_secs() < 30, "the import's tree");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(3);
    assert_eq!(parts(&h), ["Servo"]);
    assert_eq!(applied(&h), [1]);

    // A new assembly is entered, one command.
    palette(&mut h, "New assembly");
    assert_eq!(applied(&h), [1, 1]);
    let context = h.state().debug_state().context.expect("a context");
    assert_eq!(
        (context.kind.as_str(), context.name.as_str()),
        ("assembly", "Assembly 1")
    );

    // Undo takes the assembly, then the whole import: part and file.
    for _ in 0..2 {
        h.key_press_modifiers(egui::Modifiers::CTRL, Key::Z);
        h.run_steps(3);
    }
    assert!(parts(&h).is_empty(), "{:?}", parts(&h));
    let tree = h.state().debug_state().shell.expect("a frame ran").tree;
    assert!(tree.assemblies.is_empty());
    assert_eq!(applied(&h), [1, 1, 1, 1]);
}
