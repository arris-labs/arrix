//! C2 M2's accept (plans/c2-m2, docs/ROADMAP.md §C2): the app builds the
//! bracket-and-servo assembly of `tests/docs/assembly` through its own
//! commands, moves it by a drag, walks every gesture back by undo, and
//! renders the original headless.
//!
//! The scenario starts from `tests/docs/assembly` with its assembly
//! deleted before the first frame (the app deletes nothing yet, and
//! rebuilding the bracket feature by feature would test nothing of M2's).
//! It answers the import's file dialog, and picks by clicking where the
//! faces are drawn. One edit it does by command and not by the form, noted
//! where it is: a double click cannot be made in the harness's frames
//! (0.25 s apart, a double click's gap is shorter).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use arrix_app::OpenDialog;
use arrix_core::{AssemblyId, ComponentId, Id};
use arrix_doc::{Command, Component};
use egui::accesskit::Role;
use egui_kittest::kittest::Queryable as _;

use crate::component_picks::click_on;
use crate::harness;
use crate::joint_drag::{button, hint, pose, press_and_walk, turned};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

/// What the original `tests/docs/assembly` evaluates to
/// (`crates/arrix-cli/tests/golden/eval_assembly_live.jsonl`): the servo
/// on the bore, upside down.
const HINGED: [[f64; 3]; 3] = [[0.004, 0.01, 0.041], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0]];

struct Answer(Rc<RefCell<Option<PathBuf>>>);

impl OpenDialog for Answer {
    fn choose(&mut self) -> Option<PathBuf> {
        self.0.borrow().clone()
    }
}

fn run_command(h: &mut Harness<'_>, title: &str) {
    h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::K);
    h.run_steps(2);
    h.event(egui::Event::Text(title.to_owned()));
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

/// Steps until `done`, half a minute at most: the session answers on its
/// own threads.
fn wait_for(h: &mut Harness<'_>, what: &str, done: impl Fn(&Harness<'_>) -> bool) {
    let started = web_time::Instant::now();
    while !done(h) {
        h.step();
        assert!(started.elapsed().as_secs() < 30, "{what}: timed out");
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// Steps until the document has evaluated the `n`th command it was given
/// and everything it says is heard, then a few frames more (a camera fit
/// may keep asking for repaints, which `Harness::run` refuses).
fn settled_at(h: &mut Harness<'_>, n: u64) {
    wait_for(h, "the command's evaluation", |h| {
        h.state()
            .debug_state()
            .evaluation
            .is_some_and(|e| e.settled && e.generation >= n)
    });
    h.run_steps(3);
}

/// Opens the form's nth choice and takes `option`.
fn choose(h: &mut Harness<'_>, combo: usize, option: &str) {
    h.get_all_by_role(Role::ComboBox)
        .nth(combo)
        .unwrap()
        .click();
    h.run_steps(2);
    h.get_by_label(option).click();
    h.run_steps(3);
}

/// Replaces the text of the nth text input and commits it.
fn type_into(h: &mut Harness<'_>, n: usize, text: &str) {
    h.get_all_by_role(Role::TextInput).nth(n).unwrap().focus();
    h.run_steps(2);
    h.key_press_modifiers(
        egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
        egui::Key::A,
    );
    h.run_steps(2);
    h.get_all_by_role(Role::TextInput)
        .nth(n)
        .unwrap()
        .type_text(text);
    h.run_steps(2);
    h.key_press(egui::Key::Enter);
    h.run_steps(3);
}

fn ok(h: &mut Harness<'_>) {
    h.get_by_label("OK").click();
    h.run_steps(4);
}

/// The wheel, `notches` turns toward the point `at`.
fn zoom(h: &mut Harness<'_>, at: egui::Pos2, notches: usize) {
    h.hover_at(at);
    h.step();
    for _ in 0..notches {
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 1.0),
            modifiers: egui::Modifiers::NONE,
            phase: egui::TouchPhase::Move,
        });
        h.step();
    }
    h.run_steps(3);
}

/// The first point of the square of half-width `r` about `centre` where a
/// click picks an entity `wanted` accepts: a bore's wall is a few pixels
/// wide, so where it is drawn is searched for, not assumed.
fn find(
    h: &mut Harness<'_>,
    centre: egui::Pos2,
    r: f32,
    wanted: impl Fn(&arrix_viewport::Hit) -> bool,
) -> egui::Pos2 {
    let mut y = -r;
    while y <= r {
        let mut x = -r;
        while x <= r {
            let at = centre + egui::vec2(x, y);
            crate::viewport::click(h, at);
            if h.state().last_click().is_some_and(|hit| wanted(&hit)) {
                return at;
            }
            x += 1.0;
        }
        y += 1.0;
    }
    panic!("no click near {centre:?} picked what was wanted");
}

/// The component named `name` in the tree's one assembly.
fn component(h: &Harness<'_>, name: &str) -> ComponentId {
    let tree = h.state().debug_state().shell.expect("a frame ran").tree;
    let row = tree.assemblies[0]
        .components
        .iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no component {name}"));
    ComponentId(row.path[0].parse::<Id>().expect("an id"))
}

/// The assembly the tab looks at.
fn assembly_in_view(h: &Harness<'_>) -> AssemblyId {
    h.state()
        .debug_state()
        .context
        .expect("an assembly")
        .id
        .parse()
        .expect("an assembly id")
}

/// The saved record of the one assembly.
fn assembly_record(h: &Harness<'_>) -> serde_json::Value {
    let saved = h.state().saved().expect("a document is open").0;
    let (_, bytes) = saved
        .iter()
        .find(|(p, _)| p.starts_with("assemblies/"))
        .expect("an assembly is saved");
    serde_json::from_slice(bytes).expect("the record parses")
}

fn near(got: [[f64; 3]; 3], want: [[f64; 3]; 3], tolerance: f64) -> bool {
    got.iter()
        .flatten()
        .zip(want.iter().flatten())
        .all(|(g, w)| (g - w).abs() <= tolerance)
}

/// The gestures that applied a command, as their counts.
fn applied(h: &Harness<'_>) -> Vec<usize> {
    let log = h.state().debug_state().gestures;
    log.iter().map(|g| g.commands).filter(|n| *n > 0).collect()
}

#[test]
fn c2_m2_accept() {
    harness::with_app("c2_m2_accept", |h| {
        h.state_mut().seed_ids(11);
        harness::open_settled(h, "tests/docs/assembly");
        let opened = h.state().saved().expect("a document is open").0;
        let original = assembly_in_view(h);
        h.state_mut()
            .submit(Command::DeleteAssembly { assembly: original })
            .expect("the scenario's deletion");
        harness::wait_settled(h);
        h.run_steps(3);

        // The servo's file becomes a part: one gesture, one command.
        let answer = Rc::new(RefCell::new(Some(PathBuf::from(
            "../../tests/fixtures/step/servo.step",
        ))));
        h.state_mut().set_open_dialog(Box::new(Answer(answer)));
        run_command(h, "Import STEP");
        assert_eq!(applied(h), [1, 1], "the deletion, then the whole import");
        run_command(h, "New assembly");
        assert_eq!(applied(h), [1, 1, 1]);
        settled_at(h, 3);

        // The bracket (grounded at the origin), then the servo twice: the
        // first 80 mm along -X for now, so its faces can be picked, the
        // second 80 mm along X.
        run_command(h, "Insert component");
        choose(h, 0, "Part: Bracket");
        ok(h);
        run_command(h, "Insert component");
        type_into(h, 1, "-80 mm");
        ok(h);
        run_command(h, "Insert component");
        type_into(h, 1, "80 mm");
        ok(h);
        assert_eq!(applied(h), [1, 1, 1, 1, 1, 1]);
        wait_for(h, "three instances", |h| {
            h.state().debug_state().instances.len() == 3
        });
        settled_at(h, 6);
        let servo = component(h, "servo");
        let servo_2 = component(h, "servo 2");
        let state = h.state().debug_state();
        let servo_part = &state
            .instances
            .iter()
            .find(|i| i.path == [servo.0.to_string()])
            .expect("the first servo")
            .part;
        assert_eq!(
            state
                .bodies
                .iter()
                .filter(|b| &b.part == servo_part)
                .count(),
            1,
            "the servo's mesh is uploaded once for its two instances"
        );
        h.set_render_every_step(true);

        // Looking at the whole assembly, then closer at the bracket's bore.
        h.state_mut().set_context("Assembly 1").unwrap();
        h.run_steps(5);
        h.state_mut().skip_animations();
        let bore = h.state().screen_point([0.014, 0.0, 0.005]).unwrap();
        zoom(h, bore, 12);
        let bore = h.state().screen_point([0.014, 0.0, 0.005]).unwrap();
        let top = h.state().screen_point([-0.06, 0.01, 0.045]).unwrap();
        let named = |part: &'static str| {
            move |hit: &arrix_viewport::Hit| hit.name.to_string().contains(part)
        };
        let wall = find(
            h,
            bore + egui::vec2(0.0, -1.0),
            3.0,
            named("side.0000000000002"),
        );
        let plate = find(h, bore + egui::vec2(11.0, -34.0), 3.0, |hit| {
            hit.name.to_string() == "face:sweep.EBWK5C332393W.end-cap"
        });
        let shaft = find(h, top + egui::vec2(-32.0, 10.0), 3.0, named(".410"));
        let shaft_top = find(h, top + egui::vec2(-5.0, 16.0), 3.0, named(".165"));

        // A revolute joint: the bore's wall (the parent) and the shaft's
        // face (with the first servo's path, the child), each with the
        // face it is measured from along it.
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        h.get_by_label("Assembly").click();
        h.run_steps(3);
        h.get_by_label("Revolute joint").click();
        h.run_steps(4);
        crate::viewport::click(h, wall);
        h.run_steps(3);
        crate::viewport::click(h, shaft);
        h.run_steps(3);
        h.get_by_label("Parent origin").click();
        h.run_steps(3);
        crate::viewport::click(h, plate);
        h.run_steps(3);
        h.get_by_label("Child origin").click();
        h.run_steps(3);
        crate::viewport::click(h, shaft_top);
        h.run_steps(3);

        // A joint places a component that has no pose of its own. The
        // create form has that choice, but the edit form's needs a double
        // click: that one edit is a command from the scenario, made while
        // the joint's draft stays open.
        let record = assembly_record(h);
        let mut edited: Component =
            serde_json::from_value(record["components"][servo.0.to_string()].clone())
                .expect("the servo's record");
        edited.pose = None;
        let assembly = assembly_in_view(h);
        h.state_mut()
            .submit(Command::EditComponent {
                assembly,
                component: Box::new(edited),
            })
            .expect("the servo is placed by a joint");
        h.run_steps(3);
        ok(h);
        assert_eq!(
            assembly_record(h)["joints"].as_object().unwrap().len(),
            1,
            "OK is one AddJoint"
        );
        assert_eq!(applied(h), [1, 1, 1, 1, 1, 1, 1, 1]);
        settled_at(h, 8);
        let servo_id = servo.0.to_string();
        let landed = pose(h, &servo_id);
        assert!(
            near(landed, HINGED, 1e-9),
            "the servo lands on the bore as the original places it: {landed:?}"
        );

        // Drag the servo: the preview is FK at the trial value, the
        // release is one command, and the evaluated pose is the preview's.
        let before = pose(h, &servo_id);
        let other = pose(h, &servo_2.0.to_string());
        let at = click_on(h, before[0], &[servo]);
        press_and_walk(h, at, at + egui::vec2(70.0, 40.0), 6);
        let drag = h.state().debug_state().joint_drag.expect("dragging");
        assert!(drag.q != 0.0, "{drag:?}");
        let preview = pose(h, &servo_id);
        assert!(
            (turned(before, preview) - drag.q.abs()).abs() < 1e-6,
            "turned {} for q {}",
            turned(before, preview),
            drag.q
        );
        assert_eq!(
            pose(h, &servo_2.0.to_string()),
            other,
            "the grounded one stays"
        );
        button(h, at + egui::vec2(70.0, 40.0), false);
        assert_eq!(h.state().debug_state().gesture_commands, Some(1));
        settled_at(h, 9);
        let landed = pose(h, &servo_id);
        assert!(
            near(landed, preview, 2e-9),
            "landed {landed:?}, shown {preview:?}"
        );
        let joints = assembly_record(h)["joints"].clone();
        assert_eq!(
            joints.as_object().unwrap().values().next().unwrap()["value"],
            drag.value
        );

        // The second servo, grounded, does not drag; a pick on it selects
        // its path and not the first's.
        let at = click_on(h, other[0], &[servo_2]);
        assert_eq!(
            h.state().debug_state().selection_path,
            [servo_2.0.to_string()]
        );
        let gestures = h.state().debug_state().gestures.len();
        press_and_walk(h, at, at + egui::vec2(50.0, 20.0), 4);
        assert!(h.state().debug_state().joint_drag.is_none());
        assert!(hint(h).contains("grounded"), "{}", hint(h));
        button(h, at + egui::vec2(50.0, 20.0), false);
        assert_eq!(h.state().debug_state().gestures.len(), gestures);
        assert_eq!(pose(h, &servo_2.0.to_string()), other);

        // Undo walks back each gesture, one command each, to the document
        // as it was opened; the gestures' counts add up to the commands.
        let commands = applied(h).len();
        assert_eq!(commands, 9, "{:?}", applied(h));
        for _ in 0..commands {
            h.key_press_modifiers(egui::Modifiers::CTRL, egui::Key::Z);
            h.run_steps(3);
        }
        let state = h.state().debug_state();
        assert_eq!(state.commands_applied, Some(2 * commands));
        assert_eq!(
            state.gestures.iter().map(|g| g.commands).sum::<usize>(),
            state.commands_applied.unwrap(),
            "the log holds every gesture"
        );
        assert_eq!(
            h.state().saved().unwrap().0,
            opened,
            "back to the document as opened"
        );

        render_matches_the_eval_line();
    });
}

/// `arrix-app render tests/docs/assembly --context <assembly>` writes a PNG
/// and a JSON whose instances are the poses the eval line holds.
fn render_matches_the_eval_line() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("arrix-accept-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    let (png, json) = (dir.join("a.png"), dir.join("a.json"));
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_arrix-app"))
        .current_dir(&root)
        .args([
            "render",
            "tests/docs/assembly",
            "--context",
            "Servo on bracket",
            "--size",
            "320x200",
            "-o",
        ])
        .arg(&png)
        .arg("--json")
        .arg(&json)
        .output()
        .expect("arrix-app runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(png.exists(), "the PNG is written");
    let rendered: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).expect("the JSON")).expect("parses");
    let golden = std::fs::read_to_string(
        root.join("crates/arrix-cli/tests/golden/eval_assembly_live.jsonl"),
    )
    .expect("the eval golden");
    let eval: serde_json::Value =
        serde_json::from_str(golden.lines().next().expect("a line")).expect("parses");
    let poses = eval["assemblies"][0]["components"].as_array().unwrap();
    let instances = rendered["debug_state"]["instances"]
        .as_array()
        .expect("instances");
    assert_eq!(instances.len(), poses.len());
    for pose in poses {
        let instance = instances
            .iter()
            .find(|i| i["path"] == pose["path"])
            .expect("an instance of the component");
        for key in ["origin", "x_axis", "z_axis"] {
            let got = instance[key].as_array().unwrap();
            let want = pose["pose"][key].as_array().unwrap();
            for (g, w) in got.iter().zip(want) {
                assert!(
                    (g.as_f64().unwrap() - w.as_f64().unwrap()).abs() < 1e-9,
                    "{key}: {got:?} against {want:?}"
                );
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
