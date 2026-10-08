//! The frozen badge (plans/m3-accept step 3): the slice opened without
//! `gears` shows a chip at the gear's faces, frozen; the plate made thicker
//! through its form moves the gear's plane, and the chip says stale.

use arrix_ui::FeatureState;

use crate::forms_accept::{open_tree_row, type_into, wait};
use crate::{harness, snapshot};

type Harness<'a> = egui_kittest::Harness<'a, arrix_app::ArrixApp>;

fn badges(h: &Harness<'_>) -> Vec<arrix_ui::BadgeView> {
    h.state().debug_state().badges
}

#[test]
fn frozen_badge() {
    harness::with_app("frozen_badge", |h| {
        harness::open_settled(h, "tests/docs/slice");
        assert!(
            badges(h).is_empty(),
            "nothing is frozen while the plugin is there"
        );
        let saved = h.state().saved().expect("a document is open");
        h.state_mut()
            .open_without("slice (without gears)", &saved, &["gears"])
            .expect("the saved slice opens");
        harness::wait_settled(h);
        h.run_steps(2);

        let chips = badges(h);
        assert_eq!(chips.len(), 1, "{chips:?}");
        let chip = &chips[0];
        assert_eq!(chip.title, "Spur gear");
        assert_eq!(chip.state, FeatureState::Frozen);
        assert_eq!(chip.label(), "Spur gear · frozen");
        // The gear stands on the plate's top, so its faces are not the
        // joined body's centre: the chip is above the plate's top face.
        let top = h
            .state()
            .screen_point([0.0, 0.0, 0.005])
            .expect("in front of the camera");
        let gear = h
            .state()
            .screen_point([0.0, 0.0, 0.009])
            .expect("in front of the camera");
        let [x, y] = chip.at;
        assert!(
            (x - f64::from(gear.x)).abs() < 40.0 && (y - f64::from(gear.y)).abs() < 40.0,
            "chip at {:?}, gear at {gear:?}, plate top at {top:?}",
            chip.at
        );

        // Hovering the chip puts the reason in the status line.
        h.hover_at(egui::pos2(x as f32, y as f32));
        h.run_steps(3);
        let hint = h
            .state()
            .debug_state()
            .shell
            .expect("a frame ran")
            .status
            .hint;
        assert_eq!(hint, "Feature type gears.spur is not available.");
        h.hover_at(egui::pos2(5.0, 5.0));
        h.run_steps(3);
        snapshot::check(h, "frozen_badge");

        // The plate made thicker moves the plane the gear stands on: the
        // stored result no longer fits its inputs.
        open_tree_row(h, "Plate");
        type_into(h, 1, "6 mm");
        wait(h, "the gear is stale", |h| {
            badges(h)
                .first()
                .is_some_and(|b| b.state == FeatureState::FrozenStale)
        });
        wait(h, "the document settles", |h| {
            h.state()
                .debug_state()
                .evaluation
                .is_some_and(|e| e.settled)
        });
        assert_eq!(badges(h)[0].label(), "Spur gear · frozen, stale");
        snapshot::check(h, "frozen_badge_stale");
    });
}
