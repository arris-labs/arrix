//! The shell: ribbon, feature tree, viewport, property panel and status bar
//! (docs/UI-RENDERING.md §The shell).

use crate::view::{RibbonView, ShellView, StatusView, TreeView, UiIntent};

/// Fixed so a frame's layout is a function of the window size alone. A side
/// panel's default size is only its initial wrapping width, so each is also
/// its minimum: an empty tree would otherwise shrink to its heading.
const RIBBON_HEIGHT: f32 = 88.0;
const STATUS_HEIGHT: f32 = 24.0;
const TREE_WIDTH: f32 = 240.0;
const PROPERTIES_WIDTH: f32 = 280.0;

/// Where each region of the shell landed this frame, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellLayout {
    pub ribbon: egui::Rect,
    pub tree: egui::Rect,
    pub viewport: egui::Rect,
    pub properties: egui::Rect,
    pub status: egui::Rect,
}

pub struct ShellResponse {
    pub layout: ShellLayout,
    pub intents: Vec<UiIntent>,
}

/// Draws the shell from `view`. `viewport` draws into the central region;
/// the viewport's own content is not this crate's.
pub fn shell(
    ui: &mut egui::Ui,
    view: &ShellView,
    viewport: impl FnOnce(&mut egui::Ui),
) -> ShellResponse {
    let mut intents = Vec::new();
    let ribbon = egui::Panel::top("arrix.ribbon")
        .exact_size(RIBBON_HEIGHT)
        .show(ui, |ui| ribbon_ui(ui, &view.ribbon, &mut intents))
        .response
        .rect;
    let status = egui::Panel::bottom("arrix.status")
        .exact_size(STATUS_HEIGHT)
        .show(ui, |ui| status_ui(ui, &view.status))
        .response
        .rect;
    let tree = egui::Panel::left("arrix.tree")
        .default_size(TREE_WIDTH)
        .min_size(TREE_WIDTH)
        .show(ui, |ui| tree_ui(ui, &view.tree))
        .response
        .rect;
    let properties = egui::Panel::right("arrix.properties")
        .default_size(PROPERTIES_WIDTH)
        .min_size(PROPERTIES_WIDTH)
        .show(ui, properties_ui)
        .response
        .rect;
    let viewport = egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(ui.visuals().extreme_bg_color))
        .show(ui, viewport)
        .response
        .rect;
    ShellResponse {
        layout: ShellLayout {
            ribbon,
            tree,
            viewport,
            properties,
            status,
        },
        intents,
    }
}

fn ribbon_ui(ui: &mut egui::Ui, ribbon: &RibbonView, intents: &mut Vec<UiIntent>) {
    ui.horizontal(|ui| {
        for tab in &ribbon.tabs {
            let open = tab.id == ribbon.active;
            if ui.selectable_label(open, &tab.title).clicked() && !open {
                intents.push(UiIntent::RibbonTabSelected {
                    tab: tab.id.clone(),
                });
            }
        }
    });
    ui.separator();
}

fn tree_ui(ui: &mut egui::Ui, tree: &TreeView) {
    ui.heading("Parts");
    if tree.parts.is_empty() {
        ui.weak("No parts");
    }
    for part in &tree.parts {
        ui.label(&part.name);
    }
}

fn properties_ui(ui: &mut egui::Ui) {
    ui.heading("Properties");
    ui.weak("Nothing is being edited");
}

fn status_ui(ui: &mut egui::Ui, status: &StatusView) {
    ui.horizontal_centered(|ui| {
        ui.label(format!("Generation {}", status.generation));
        ui.separator();
        ui.label(format!("{} diagnostics", status.diagnostics));
        ui.separator();
        ui.label(format!("{}, {}", status.length_unit, status.angle_unit));
    });
}
