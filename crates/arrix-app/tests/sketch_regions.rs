//! Regions shaded (plans/sketch-mode step 14, docs/UI-RENDERING.md §Sketch
//! mode): the overlay shades the faces of the draft's arrangement, the keys
//! it reports are the keys the evaluator's regions have (the ones
//! `core.extrude`'s region input resolves, `Ref::Region`), and a hole drawn
//! inside a region splits it in the view as it does there.

mod common;

use arrix_core::FeatureId;
use arrix_doc::{Evaluator, FeatureOutcome, Registry, SlotView};
use common::*;
use egui::Key;

/// The keys of the regions the evaluator finds in the saved document's
/// open sketch, as JSON, sorted.
fn evaluated(h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>) -> Vec<String> {
    let feature: FeatureId = h
        .state()
        .debug_state()
        .sketch
        .expect("sketch mode")
        .feature
        .parse()
        .expect("a feature id");
    let document = arrix_doc::open(&h.state().saved().expect("a document")).expect("it opens");
    let evaluation = Evaluator::new(Registry::with_core_types()).evaluate(&document);
    let Some(FeatureOutcome::Ok { slots, .. }) = evaluation.outcome(feature) else {
        panic!("the sketch evaluates");
    };
    let Some(SlotView::Sketch(view)) = slots.values().find(|s| matches!(s, SlotView::Sketch(_)))
    else {
        panic!("a sketch slot");
    };
    let mut keys: Vec<String> = view
        .regions
        .iter()
        .map(|r| serde_json::to_string(&r.key).unwrap())
        .collect();
    keys.sort();
    keys
}

/// The keys the overlay shades, sorted.
fn shaded(h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>) -> Vec<String> {
    let mut keys: Vec<String> = h
        .state()
        .debug_state()
        .sketch_view
        .map(|v| v.regions.into_iter().map(|r| r.key).collect())
        .unwrap_or_default();
    keys.sort();
    keys
}

#[test]
fn the_shaded_regions_are_the_ones_the_evaluator_resolves() {
    let mut h = with_rectangle();
    wait(&mut h, "the rectangle's region", |h| shaded(h).len() == 1);
    assert_eq!(shaded(&h), evaluated(&h));

    // A circle drawn inside it: the plate with a bore, and the disc.
    h.key_press(Key::C);
    h.run_steps(2);
    click(&mut h, [35.0, 25.0]);
    click(&mut h, [40.0, 25.0]);
    wait(&mut h, "the hole splits the region", |h| {
        shaded(h).len() == 2
    });
    assert_eq!(shaded(&h), evaluated(&h), "the same keys as the evaluator");
    let view = h.state().debug_state().sketch_view.unwrap();
    assert_eq!(
        view.regions.iter().map(|r| r.holes).collect::<Vec<_>>(),
        [1, 0],
        "the plate holds the bore, the disc none, largest first"
    );
}

#[test]
fn the_region_under_the_pointer_is_outlined() {
    let mut h = with_rectangle();
    wait(&mut h, "the rectangle's region", |h| shaded(h).len() == 1);
    let key = shaded(&h).remove(0);
    let hovered = |h: &egui_kittest::Harness<'_, arrix_app::ArrixApp>| {
        h.state().debug_state().sketch_view.unwrap().hover_region
    };
    hover(&mut h, [35.0, 25.0]);
    assert_eq!(hovered(&h), Some(key), "inside");
    hover(&mut h, [70.0, 60.0]);
    assert_eq!(hovered(&h), None, "outside");
}
