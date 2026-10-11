//! The shell's export (plans/c1-m4 step 7): `Export…` through the palette
//! with the save dialog answered by the scenario, the file read back, STEP
//! refused until Arris's writer can declare metres, and no document command
//! in the gesture log.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use arrix_app::SaveDialog;
use egui::Key;

use crate::harness;
use crate::viewport::click;

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

/// A dialog that answers what the scenario put in `answer`, and counts the
/// times it was asked.
struct Stub {
    answer: Rc<RefCell<Option<PathBuf>>>,
    asked: Rc<RefCell<usize>>,
}

fn stub(h: &mut Harness<'_>) -> (Rc<RefCell<Option<PathBuf>>>, Rc<RefCell<usize>>) {
    let answer = Rc::new(RefCell::new(None));
    let asked = Rc::new(RefCell::new(0));
    h.state_mut().set_save_dialog(Box::new(Stub {
        answer: answer.clone(),
        asked: asked.clone(),
    }));
    (answer, asked)
}

impl SaveDialog for Stub {
    fn choose(&mut self, _name: &str) -> Option<PathBuf> {
        *self.asked.borrow_mut() += 1;
        self.answer.borrow().clone()
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = harness::target_dir().join("export-scenario");
    std::fs::create_dir_all(&dir).expect("create the scratch directory");
    let path = dir.join(name);
    let _ = std::fs::remove_file(&path);
    path
}

/// Runs `Export…` through the palette, as a user would.
fn run_export(h: &mut Harness<'_>) {
    h.key_press_modifiers(egui::Modifiers::CTRL, Key::K);
    h.run_steps(2);
    h.event(egui::Event::Text("Export".to_owned()));
    h.run_steps(2);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

/// Steps until `path` exists (the file arrives behind the request).
fn wait_for(h: &mut Harness<'_>, path: &std::path::Path) {
    let started = web_time::Instant::now();
    while !path.exists() {
        h.step();
        assert!(
            started.elapsed() < std::time::Duration::from_secs(30),
            "{} was never written",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(2);
}

#[test]
fn export_writes_the_bodies_and_applies_no_command() {
    harness::with_app("export", |h| {
        harness::open_settled(h, "tests/docs/slice");
        h.set_render_every_step(true);
        let (answer, asked) = stub(h);
        let saved = h.state().saved().unwrap();

        // Nothing selected: every body, as OBJ.
        let all = scratch("all.obj");
        *answer.borrow_mut() = Some(all.clone());
        run_export(h);
        wait_for(h, &all);
        let text = std::fs::read_to_string(&all).unwrap();
        assert!(text.lines().any(|l| l.starts_with("f ")), "faces written");
        assert_eq!(h.state().debug_state().gesture_commands, Some(0));
        assert_eq!(
            h.state().saved().unwrap().0,
            saved.0,
            "the document is unchanged"
        );

        // A face of the plate selected: the body it is in, the same file.
        let face = h.state().screen_point([-0.016, -0.011, 0.005]).unwrap();
        click(h, face);
        let selected = scratch("selected.obj");
        *answer.borrow_mut() = Some(selected.clone());
        run_export(h);
        wait_for(h, &selected);
        assert_eq!(std::fs::read(&selected).unwrap(), text.as_bytes());

        // STEP is refused until Arris's writer can declare metres: nothing
        // written, and the status line says why.
        let step = scratch("body.step");
        *answer.borrow_mut() = Some(step.clone());
        run_export(h);
        let started = web_time::Instant::now();
        let hint = loop {
            h.step();
            let hint = h
                .state()
                .debug_state()
                .shell
                .expect("a frame ran")
                .status
                .hint;
            if hint.contains("kernel.export.unit") {
                break hint;
            }
            assert!(
                started.elapsed() < std::time::Duration::from_secs(30),
                "never refused: {hint}"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(hint.starts_with("export failed"), "{hint}");
        assert!(!step.exists());

        // The format is the extension's: a closed STL.
        let stl = scratch("body.stl");
        *answer.borrow_mut() = Some(stl.clone());
        run_export(h);
        wait_for(h, &stl);
        let bytes = std::fs::read(&stl).unwrap();
        let count = u32::from_le_bytes(bytes[80..84].try_into().unwrap()) as usize;
        assert_eq!(bytes.len(), 84 + 50 * count);

        // Cancelled: asked, and nothing written. An extension it does not
        // know writes nothing and says so.
        let asked_before = *asked.borrow();
        *answer.borrow_mut() = None;
        run_export(h);
        let unknown = scratch("body.txt");
        *answer.borrow_mut() = Some(unknown.clone());
        run_export(h);
        h.run_steps(30);
        assert_eq!(*asked.borrow(), asked_before + 2);
        assert!(!unknown.exists());
        assert_eq!(h.state().saved().unwrap().0, saved.0);
        assert_eq!(h.state().debug_state().gesture_commands, Some(0));
    });
}
