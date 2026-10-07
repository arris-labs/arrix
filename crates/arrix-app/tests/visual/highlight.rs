//! Hover and selection as restyled ranges of the scene's buffers
//! (plans/viewport-port step 11).

use crate::viewport::{click, selection};
use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

fn on_screen(h: &Harness<'_>, p: [f64; 3]) -> egui::Pos2 {
    h.state().screen_point(p).expect("in front of the camera")
}

/// The colour drawn at `at`, from a frame rendered now.
fn pixel(h: &mut Harness<'_>, at: egui::Pos2) -> [u8; 4] {
    let ppp = h.ctx.pixels_per_point();
    // What an answered pick changed is drawn by the frame after it.
    h.step();
    let frame = harness::render(h);
    frame.get_pixel((at.x * ppp) as u32, (at.y * ppp) as u32).0
}

/// Moves the pointer to `at` and steps, rendering every frame, until the
/// hover pick it asked for has been answered.
fn hover(h: &mut Harness<'_>, at: egui::Pos2) {
    h.hover_at(at);
    let started = web_time::Instant::now();
    loop {
        h.step();
        if !h.state().debug_state().picking {
            h.step();
            return;
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "the hover was never answered"
        );
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

fn hovered(h: &Harness<'_>) -> Option<String> {
    h.state().debug_state().hover.map(|name| name.to_string())
}

fn hint(h: &Harness<'_>) -> String {
    h.state()
        .debug_state()
        .shell
        .expect("a frame ran")
        .status
        .hint
}

/// The pointer over an edge restyles it, a click on a face restyles the
/// face and its boundary, and Escape takes the selection's style away; the
/// status line says what is hovered and what a click does, and none of it
/// is an edit.
#[test]
fn hover_and_selection() {
    harness::with_app("hover_and_selection", |h| {
        harness::open_settled(h, "tests/docs/slice");
        h.set_render_every_step(true);
        let generation = h.state().debug_state().evaluation.map(|e| e.generation);
        // The plate is 40 × 30 × 5 mm about the origin; this is on its top
        // face, and this on the vertical edge below the top's near corner.
        let face = on_screen(h, [-0.016, -0.011, 0.005]);
        let edge = on_screen(h, [-0.02, -0.015, 0.0025]);
        let before = (pixel(h, face), pixel(h, edge));

        click(h, face);
        let selected = selection(h).expect("the top face");
        assert!(selected.starts_with("face:"), "{selected}");
        let tinted = pixel(h, face);
        assert!(
            tinted[0] + 15 < before.0[0] && tinted[2] > before.0[2],
            "the selected face is tinted blue: {:?} became {tinted:?}",
            before.0
        );

        hover(h, edge);
        let name = hovered(h).expect("the edge under the pointer");
        assert!(name.starts_with("edge:"), "{name}");
        assert_eq!(hint(h), format!("{name}: click to select it."));
        // Above the pointer, whose arrow the harness draws over its tip.
        let amber = pixel(h, on_screen(h, [-0.02, -0.015, 0.0038]));
        assert!(
            amber[0] > 200 && amber[0] > amber[2] + 60,
            "the hovered edge is drawn amber: {:?} became {amber:?}",
            before.1
        );

        // The hover is not the selection, and the face is still selected.
        assert_eq!(selection(h), Some(selected.clone()));
        snapshot::check(h, "hover_selection");

        // Clicking the hovered edge selects it, and the hint says so.
        click(h, edge);
        assert_eq!(selection(h), Some(name.clone()));
        // The harness takes the pointer away after a click.
        assert_eq!(hovered(h), None);
        hover(h, edge);
        assert_eq!(
            hint(h),
            format!("{name}: selected. Escape clears the selection.")
        );
        assert_eq!(pixel(h, face), before.0, "the face lost its tint");

        h.key_press(egui::Key::Escape);
        h.step();
        assert_eq!(selection(h), None, "Escape clears the selection");

        // Off the viewport, onto the ribbon: nothing is hovered.
        let ribbon = h.state().debug_state().layout.unwrap().ribbon;
        h.hover_at(egui::pos2(ribbon[0] as f32 + 5.0, ribbon[1] as f32 + 5.0));
        h.step();
        h.step();
        assert_eq!(hovered(h), None);

        assert_eq!(
            h.state().debug_state().evaluation.map(|e| e.generation),
            generation,
            "hovering, picking and clearing are not edits"
        );
    });
}
