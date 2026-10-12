//! The shell: two docked bars and the cards over the viewport
//! (docs/UI-RENDERING.md §The shell, ADR-0017).
//!
//! The ribbon is on top. At the bottom the tab strip of the open documents
//! sits over the status line. The viewport fills what is between them, and
//! the feature tree and the property panel are cards floating over it, drawn
//! only while they show.

use crate::view::{
    CardsView, DocumentTabsView, FeatureState, JointState, RibbonView, ShellView, StatusView,
    TreeAssembly, TreeComponent, TreeFeature, TreeJoint, TreeView, UiIntent,
};

/// Fixed so a frame's layout is a function of the window size and of which
/// cards show.
const RIBBON_HEIGHT: f32 = 88.0;
const TABS_HEIGHT: f32 = 24.0;
const STATUS_HEIGHT: f32 = 20.0;
const TREE_WIDTH: f32 = 240.0;
const PROPERTIES_WIDTH: f32 = 280.0;
/// The gap between a card and the viewport's edge.
const CARD_MARGIN: f32 = 8.0;

/// Where each region of the shell landed this frame, in points. A card that
/// is not showing has no rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShellLayout {
    pub ribbon: egui::Rect,
    pub viewport: egui::Rect,
    pub tabs: egui::Rect,
    pub status: egui::Rect,
    pub tree: Option<egui::Rect>,
    pub properties: Option<egui::Rect>,
}

/// The cube's own rect in the viewport `viewport`, clear of the property
/// card when that is showing: cards are a layer above the viewport and
/// would cover it.
pub fn viewcube_rect(viewport: egui::Rect, cards: &CardsView) -> egui::Rect {
    let reserved = if cards.properties {
        PROPERTIES_WIDTH + CARD_MARGIN
    } else {
        0.0
    };
    crate::viewcube::viewcube_rect_in(egui::Rect::from_min_max(
        viewport.min,
        egui::pos2(viewport.max.x - reserved, viewport.max.y),
    ))
}

pub struct ShellResponse {
    pub layout: ShellLayout,
    pub intents: Vec<UiIntent>,
    /// The command of the ribbon button or palette row under the pointer:
    /// the status line's hint is about it next frame.
    pub hovered_command: Option<String>,
}

/// Draws the shell from `view`. `viewport` draws into the central region;
/// the viewport's own content is not this crate's.
pub fn shell(
    ui: &mut egui::Ui,
    view: &ShellView,
    viewport: impl FnOnce(&mut egui::Ui),
) -> ShellResponse {
    let mut intents = Vec::new();
    let mut hovered_command = None;
    let ribbon = egui::Panel::top("arrix.ribbon")
        .exact_size(RIBBON_HEIGHT)
        .show(ui, |ui| {
            ribbon_ui(ui, &view.ribbon, &mut intents, &mut hovered_command);
        })
        .response
        .rect;
    // The first bottom panel is the lowest.
    let status = egui::Panel::bottom("arrix.status")
        .exact_size(STATUS_HEIGHT)
        .show(ui, |ui| status_ui(ui, &view.status))
        .response
        .rect;
    let tabs = egui::Panel::bottom("arrix.tabs")
        .exact_size(TABS_HEIGHT)
        .show(ui, |ui| tabs_ui(ui, &view.documents, &mut intents))
        .response
        .rect;
    let viewport = egui::CentralPanel::default()
        .frame(egui::Frame::new().fill(ui.visuals().extreme_bg_color))
        .show(ui, viewport)
        .response
        .rect;
    let tree = view
        .cards
        .tree
        .then(|| tree_card(ui.ctx(), viewport, &view.tree, &mut intents));
    let properties = view
        .cards
        .properties
        .then(|| properties_card(ui.ctx(), viewport, view.form.as_ref(), &mut intents));
    if let Some(view) = &view.palette {
        crate::palette::palette(ui.ctx(), view, &mut intents, &mut hovered_command);
    }
    ShellResponse {
        layout: ShellLayout {
            ribbon,
            viewport,
            tabs,
            status,
            tree,
            properties,
        },
        intents,
        hovered_command,
    }
}

fn ribbon_ui(
    ui: &mut egui::Ui,
    ribbon: &RibbonView,
    intents: &mut Vec<UiIntent>,
    hovered: &mut Option<String>,
) {
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
    // A tab with many groups scrolls sideways rather than clipping.
    egui::ScrollArea::horizontal()
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| ribbon_groups(ui, ribbon, intents, hovered));
        });
}

fn ribbon_groups(
    ui: &mut egui::Ui,
    ribbon: &RibbonView,
    intents: &mut Vec<UiIntent>,
    hovered: &mut Option<String>,
) {
    for group in &ribbon.groups {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                for command in &group.commands {
                    let button = ui.add_enabled(
                        command.enabled,
                        egui::Button::new(&command.title).selected(command.active),
                    );
                    // `contains_pointer`, not `hovered`: a disabled
                    // button is never hovered, and its hint says why.
                    if button.contains_pointer() {
                        *hovered = Some(command.id.clone());
                    }
                    if button.clicked() {
                        intents.push(UiIntent::CommandInvoked {
                            command: command.id.clone(),
                        });
                    }
                }
            });
            ui.weak(&group.title);
        });
        ui.separator();
    }
}

fn tabs_ui(ui: &mut egui::Ui, documents: &DocumentTabsView, intents: &mut Vec<UiIntent>) {
    ui.horizontal_centered(|ui| {
        for (index, tab) in documents.tabs.iter().enumerate() {
            let label = ui
                .selectable_label(tab.active, &tab.title)
                .on_hover_text(&tab.path);
            if label.clicked() && !tab.active {
                intents.push(UiIntent::DocumentTabSelected { index });
            }
            if ui.small_button("×").on_hover_text("Close").clicked() {
                intents.push(UiIntent::DocumentTabClosed { index });
            }
            ui.add_space(6.0);
        }
    });
}

fn status_ui(ui: &mut egui::Ui, status: &StatusView) {
    ui.horizontal_centered(|ui| {
        ui.label(&status.hint);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(format!("{}, {}", status.length_unit, status.angle_unit));
            if let Some(sketch) = &status.sketch {
                ui.separator();
                ui.label(format!("{} DoF, {}", sketch.dof, sketch.state.label()));
            }
            ui.separator();
            ui.label(format!("{} diagnostics", status.diagnostics));
            ui.separator();
            ui.label(format!("Generation {}", status.generation));
        });
    });
}

/// A card floating over `viewport`: fixed (not draggable) so the layout is a
/// function of the window, and above the viewport, so a pointer over it
/// never reaches what is under it. `right` pins it to the right edge.
fn card(
    ctx: &egui::Context,
    id: &'static str,
    viewport: egui::Rect,
    right: bool,
    width: f32,
    content: impl FnOnce(&mut egui::Ui),
) -> egui::Rect {
    let (pivot, pos) = if right {
        (
            egui::Align2::RIGHT_TOP,
            viewport.right_top() + egui::vec2(-CARD_MARGIN, CARD_MARGIN),
        )
    } else {
        (
            egui::Align2::LEFT_TOP,
            viewport.left_top() + egui::vec2(CARD_MARGIN, CARD_MARGIN),
        )
    };
    egui::Area::new(egui::Id::new(id))
        .pivot(pivot)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(width);
                content(ui);
            });
        })
        .response
        .rect
}

fn tree_card(
    ctx: &egui::Context,
    viewport: egui::Rect,
    tree: &TreeView,
    intents: &mut Vec<UiIntent>,
) -> egui::Rect {
    card(ctx, "arrix.tree", viewport, false, TREE_WIDTH, |ui| {
        if !tree.assemblies.is_empty() {
            ui.heading("Assemblies");
            for assembly in &tree.assemblies {
                assembly_rows(ui, assembly, intents);
            }
        }
        ui.heading("Parts");
        if tree.parts.is_empty() {
            ui.weak("No parts");
        }
        for part in &tree.parts {
            let row = name_row(ui, &part.name, part.current);
            if row.double_clicked() {
                intents.push(UiIntent::TreePartEdited {
                    part: part.id.clone(),
                });
            }
            if row.clicked() {
                intents.push(UiIntent::TreePartActivated {
                    part: part.id.clone(),
                });
            }
            for feature in &part.features {
                feature_row(ui, feature, intents);
            }
        }
    })
}

/// A part's or an assembly's name, strong, accented while the tab looks
/// at it; the response says whether it was clicked.
fn name_row(ui: &mut egui::Ui, name: &str, current: bool) -> egui::Response {
    let text = egui::RichText::new(name).strong();
    let text = if current {
        text.color(ui.visuals().selection.stroke.color)
    } else {
        text
    };
    // `CLICK` alone is not focusable: a name that is focused and then
    // hidden with its card leaves the accessibility tree holding a node
    // that is gone.
    ui.add(egui::Label::new(text).sense(egui::Sense::CLICK))
}

fn assembly_rows(ui: &mut egui::Ui, assembly: &TreeAssembly, intents: &mut Vec<UiIntent>) {
    if name_row(ui, &assembly.name, assembly.current).clicked() {
        intents.push(UiIntent::TreeAssemblyActivated {
            assembly: assembly.id.clone(),
        });
    }
    for component in &assembly.components {
        component_row(ui, &assembly.id, component, 0, intents);
    }
    for joint in &assembly.joints {
        joint_row(ui, &assembly.id, joint, intents);
    }
}

fn component_row(
    ui: &mut egui::Ui,
    assembly: &str,
    component: &TreeComponent,
    depth: usize,
    intents: &mut Vec<UiIntent>,
) {
    let visuals = ui.visuals();
    let weak = visuals.weak_text_color();
    let fill = if component.selected {
        visuals.selection.bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    let colour = match component.placement {
        crate::Placement::Grounded | crate::Placement::ByJoint => visuals.text_color(),
        crate::Placement::Unplaced | crate::Placement::Loop => visuals.warn_fg_color,
        crate::Placement::Pending => weak,
    };
    let row = egui::Frame::new().fill(fill).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add_space(8.0 + 12.0 * depth as f32);
            ui.label(egui::RichText::new(&component.name).color(colour));
            ui.label(egui::RichText::new(&component.of).color(weak).small());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let tag = match &component.joint {
                    Some(joint) => format!("{} {joint}", component.placement.tag()),
                    None => component.placement.tag().to_owned(),
                };
                ui.label(egui::RichText::new(tag).color(colour).small());
            });
        });
    });
    let response = row.response.interact(egui::Sense::CLICK);
    if response.double_clicked() {
        intents.push(UiIntent::TreeComponentActivated {
            assembly: assembly.to_owned(),
            path: component.path.clone(),
        });
    } else if response.clicked() {
        intents.push(UiIntent::TreeComponentSelected {
            assembly: assembly.to_owned(),
            path: component.path.clone(),
        });
    }
    for child in &component.children {
        component_row(ui, assembly, child, depth + 1, intents);
    }
}

fn joint_row(ui: &mut egui::Ui, assembly: &str, joint: &TreeJoint, intents: &mut Vec<UiIntent>) {
    let visuals = ui.visuals();
    let colour = match joint.state {
        JointState::Ok => visuals.text_color(),
        JointState::Evaluating => visuals.weak_text_color(),
        JointState::Limit | JointState::Loop => visuals.warn_fg_color,
        JointState::Lost | JointState::Failed => visuals.error_fg_color,
    };
    let weak = visuals.weak_text_color();
    let row = egui::Frame::new().show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&joint.name).color(colour));
            ui.label(egui::RichText::new(&joint.kind).color(weak).small());
            if let Some(tag) = joint.state.tag() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(tag).color(colour).small());
                });
            }
        });
    });
    let hover = match &joint.note {
        Some(note) => format!("{} joint\n{note}", joint.kind),
        None => format!("{} joint", joint.kind),
    };
    let response = row
        .response
        .interact(egui::Sense::CLICK)
        .on_hover_text(hover);
    if response.double_clicked() {
        intents.push(UiIntent::TreeJointActivated {
            assembly: assembly.to_owned(),
            joint: joint.id.clone(),
        });
    }
}

fn feature_row(ui: &mut egui::Ui, feature: &TreeFeature, intents: &mut Vec<UiIntent>) {
    let visuals = ui.visuals();
    let colour = match feature.state {
        FeatureState::Failed => visuals.error_fg_color,
        FeatureState::Frozen | FeatureState::FrozenStale => visuals.warn_fg_color,
        FeatureState::Ok => visuals.text_color(),
        _ => visuals.weak_text_color(),
    };
    let fill = if feature.selected {
        visuals.selection.bg_fill
    } else {
        egui::Color32::TRANSPARENT
    };
    let row = egui::Frame::new().fill(fill).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.add_space(8.0);
            ui.label(egui::RichText::new(&feature.name).color(colour));
            if let Some(tag) = feature.state.tag() {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(tag).color(colour).small());
                });
            }
        });
    });
    let hover = match &feature.note {
        Some(note) => format!("{}\n{note}", feature.kind),
        None => feature.kind.clone(),
    };
    let response = row.response.interact(egui::Sense::CLICK);
    let response = response.on_hover_text(hover);
    if response.double_clicked() {
        intents.push(UiIntent::TreeFeatureActivated {
            feature: feature.id.clone(),
        });
    } else if response.clicked() {
        intents.push(UiIntent::TreeFeatureSelected {
            feature: feature.id.clone(),
        });
    }
}

fn properties_card(
    ctx: &egui::Context,
    viewport: egui::Rect,
    form: Option<&crate::FormView>,
    intents: &mut Vec<UiIntent>,
) -> egui::Rect {
    card(
        ctx,
        "arrix.properties",
        viewport,
        true,
        PROPERTIES_WIDTH,
        |ui| match form {
            Some(form) => intents.extend(crate::form(ui, form)),
            None => {
                ui.heading("Properties");
                ui.weak("Nothing is being edited");
            }
        },
    )
}
