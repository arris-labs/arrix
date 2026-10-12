//! C2 M3's import and materials in the app (plans/c2-m3 step 8, ADR-0032,
//! ADR-0031): `Import…` reads the probe's `arm.step` into an empty
//! document as one command, its parts drawn in the colours the file gave
//! them at the placements it gave, a screw instanced twice; a double-click
//! on a part opens its material form, where a material is made, named,
//! given a density and a colour, and the viewport follows.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use egui_kittest::kittest::Queryable as _;

use crate::c2_m2_accept::{Answer, applied, run_command, settled_at, type_into, wait_for};
use crate::forms_accept::double_click;
use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

fn near(got: [f64; 3], want: [f64; 3]) -> bool {
    got.iter().zip(want).all(|(g, w)| (g - w).abs() < 1e-9)
}

fn colour_of(h: &Harness<'_>, part: &str) -> Option<[f32; 3]> {
    let state = h.state().debug_state();
    let tree = state.shell.expect("a frame ran").tree;
    let id = &tree
        .parts
        .iter()
        .find(|p| p.name == part)
        .expect("a part")
        .id;
    let colours: Vec<_> = state
        .bodies
        .iter()
        .filter(|b| &b.part == id)
        .map(|b| b.colour)
        .collect();
    assert_eq!(colours.len(), 1, "one body for {part}");
    colours[0]
}

fn form_title(h: &Harness<'_>) -> Option<String> {
    h.state()
        .debug_state()
        .shell
        .expect("a frame ran")
        .form
        .map(|f| f.title)
}

#[test]
fn import_structure() {
    harness::with_app("import_structure", |h| {
        h.state_mut().seed_ids(5);
        harness::open_settled(h, "tests/docs/empty");
        let answer = Rc::new(RefCell::new(Some(PathBuf::from(
            "../../tests/fixtures/step/arm.step",
        ))));
        h.state_mut().set_open_dialog(Box::new(Answer(answer)));
        run_command(h, "Import STEP");
        // The session reads the tree on its worker; one command follows.
        wait_for(h, "the import's tree", |h| applied(h) == [1]);
        settled_at(h, 1);
        harness::wait_settled(h);
        h.run();

        // The file's tree: arm { bracket, mount { servo, screw, screw } },
        // the arm in view, each part in its file colour.
        let state = h.state().debug_state();
        let context = state.context.as_ref().expect("an assembly in view");
        assert_eq!(
            (context.kind.as_str(), context.name.as_str()),
            ("assembly", "arm")
        );
        let tree = state.shell.as_ref().expect("a frame ran").tree.clone();
        assert_eq!(tree.assemblies.len(), 2, "arm and mount");
        assert_eq!(tree.parts.len(), 3, "bracket, servo, screw");
        assert_eq!(state.bodies.len(), 3);
        assert_eq!(state.instances.len(), 4, "the screw twice");
        assert_eq!(colour_of(h, "bracket"), Some([0.6, 0.6, 0.6]));
        assert_eq!(colour_of(h, "servo"), Some([0.1, 0.2, 0.7]));
        assert_eq!(colour_of(h, "screw"), Some([0.8, 0.8, 0.85]));
        // The bracket sits at (5, -3, 0) mm, as the file placed it.
        let bracket = state
            .instances
            .iter()
            .find(|i| near(i.origin, [0.005, -0.003, 0.0]))
            .expect("the bracket at its placement");
        assert!(near(bracket.z_axis, [0.0, 0.0, 1.0]));
        snapshot::check(h, "import_structure");

        // A double-click on a part opens its material form, which makes a
        // material, names it, and gives it a density and a colour.
        let before = harness::render(h);
        // The part's own row is the leftmost "screw": its components and
        // its feature are indented under other names.
        let at = h
            .get_all_by_label("screw")
            .map(|n| n.rect())
            .min_by(|a, b| a.min.x.total_cmp(&b.min.x))
            .expect("the part's row")
            .center();
        double_click(h, at);
        h.run_steps(4);
        assert_eq!(form_title(h).as_deref(), Some("Material of screw"));
        let gestures = applied(h).len();
        h.get_all_by_role(egui::accesskit::Role::ComboBox)
            .next()
            .unwrap()
            .click();
        h.run_steps(2);
        h.get_by_label("New material…").click();
        h.run_steps(4);
        assert_eq!(
            applied(h).len(),
            gestures + 1,
            "one command for a new material"
        );
        settled_at(h, 2);
        type_into(h, 1, "7.85 g/cm3");
        type_into(h, 2, "#cc3322");
        settled_at(h, 4);
        assert_eq!(
            colour_of(h, "screw").map(|c| c.map(|v| (v * 255.0).round() as u8)),
            Some([0xcc, 0x33, 0x22])
        );
        assert!(
            harness::render(h) != before,
            "the screws are drawn in the new colour"
        );
        snapshot::check(h, "import_structure_material");
    });
}
