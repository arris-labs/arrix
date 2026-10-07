//! The command registry as plain data: every user action is a
//! [`CommandSpec`] with a stable id (ADR-0016, constraint 1), and the ribbon,
//! the palette and the keymap all read this one table. This crate holds the
//! data; whether a command applies right now, and what it does, is
//! `arrix-app`'s (it owns session state and dispatch).

use arrix_plugin_api::FeatureTypeSpec;
use serde::Serialize;

/// The core tabs, in ribbon order.
pub const CORE_SKETCH_TAB: &str = "core.sketch";
pub const CORE_PART_TAB: &str = "core.part";

/// Commands that are not tied to a feature type.
pub const PALETTE_OPEN: &str = "core.palette.open";
pub const CONFIG_RELOAD: &str = "core.config.reload";
/// The palette's own keys, bound in the `palette` context.
pub const PALETTE_CLOSE: &str = "core.palette.close";
pub const PALETTE_NEXT: &str = "core.palette.next";
pub const PALETTE_PREVIOUS: &str = "core.palette.previous";
pub const PALETTE_RUN: &str = "core.palette.run";
pub const PANEL_TREE_TOGGLE: &str = "core.panel.tree.toggle";
pub const PANEL_PROPERTIES_TOGGLE: &str = "core.panel.properties.toggle";
/// Clearing what is selected in the viewport.
pub const SELECTION_CLEAR: &str = "core.selection.clear";
/// Turning the camera to a standard view, and the projection.
pub const VIEW_HOME: &str = "core.view.home";
pub const VIEW_TOP: &str = "core.view.top";
pub const VIEW_FRONT: &str = "core.view.front";
pub const VIEW_RIGHT: &str = "core.view.right";
pub const VIEW_ISO: &str = "core.view.iso";
pub const VIEW_PROJECTION_TOGGLE: &str = "core.view.projection.toggle";
/// Switching and closing the open documents' tabs.
pub const DOCUMENT_NEXT: &str = "core.document.next";
pub const DOCUMENT_PREVIOUS: &str = "core.document.previous";
pub const DOCUMENT_CLOSE: &str = "core.document.close";
/// Sketch mode: opening a sketch (the creation command is the type's,
/// `core.sketch.create`) and leaving it.
pub const SKETCH_CREATE: &str = "core.sketch.create";
pub const SKETCH_EDIT: &str = "core.sketch.edit";
pub const SKETCH_FINISH: &str = "core.sketch.finish";
/// Deleting what is selected in the open sketch.
pub const SKETCH_DELETE: &str = "core.sketch.delete";
/// The sketch's tools: selecting, and drawing lines, rectangles, circles
/// and arcs.
pub const SKETCH_SELECT: &str = "core.sketch.select";
pub const SKETCH_LINE: &str = "core.sketch.line";
pub const SKETCH_RECTANGLE: &str = "core.sketch.rectangle";
pub const SKETCH_CIRCLE: &str = "core.sketch.circle";
pub const SKETCH_ARC: &str = "core.sketch.arc";
/// Escape while a drawing tool is active: ends what it is drawing, or with
/// nothing pending returns to the select tool.
pub const SKETCH_TOOL_CANCEL: &str = "core.sketch.tool.cancel";
/// A new, empty document in a new tab.
pub const DOCUMENT_NEW: &str = "core.document.new";
/// Undoing and redoing the author's last change to the shown document.
pub const EDIT_UNDO: &str = "core.edit.undo";
pub const EDIT_REDO: &str = "core.edit.redo";

/// Core feature types that are made in the Sketch tab; the rest of the core
/// types are part features.
const SKETCH_TYPES: &[&str] = &["core.sketch", "core.datum-plane"];

/// A ribbon tab: the core's two, then one per plugin that contributes a
/// feature type (a plugin's tab is one more entry, never a mode).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TabSpec {
    /// `core.sketch`, `core.part`, or the plugin's id (`gears`).
    pub id: String,
    pub title: String,
}

/// One user action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommandSpec {
    /// Stable: `core.palette.open`, `gears.spur.create`.
    pub id: String,
    pub title: String,
    /// Where the ribbon shows it, as a tab id and a group title; `None` for
    /// a command only the palette and the keymap reach.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ribbon: Option<RibbonPlacement>,
    /// The keys it has until the user's `keybindings.toml` says otherwise.
    pub keys: Vec<String>,
    /// The keymap context its default keys apply in; `None` is everywhere.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
    /// Whether it needs an open document to apply.
    pub needs_document: bool,
    /// Whether the palette lists it: the palette's own keys are not.
    pub listed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RibbonPlacement {
    pub tab: String,
    pub group: String,
}

/// Every tab and command there is, in ribbon order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommandRegistry {
    pub tabs: Vec<TabSpec>,
    pub commands: Vec<CommandSpec>,
}

impl CommandRegistry {
    /// The core table, then one `<type>.create` per registered feature type
    /// (`types`, in registration order) and one tab per plugin among them.
    pub fn new(types: &[FeatureTypeSpec]) -> Self {
        let mut tabs = vec![
            TabSpec {
                id: CORE_SKETCH_TAB.into(),
                title: "Sketch".into(),
            },
            TabSpec {
                id: CORE_PART_TAB.into(),
                title: "Part".into(),
            },
        ];
        let mut commands = vec![
            command(PALETTE_OPEN, "Command palette", &["ctrl+k"]),
            command(CONFIG_RELOAD, "Reload configuration", &[]),
            command(PANEL_TREE_TOGGLE, "Toggle feature tree", &[]),
            command(PANEL_PROPERTIES_TOGGLE, "Toggle property panel", &[]),
            CommandSpec {
                when: Some("part".into()),
                ..document_command(SELECTION_CLEAR, "Clear selection", &["escape"])
            },
            view_command(VIEW_HOME, "Home view: fit all", &["home"]),
            view_command(VIEW_TOP, "Top view", &["ctrl+7"]),
            view_command(VIEW_FRONT, "Front view", &["ctrl+1"]),
            view_command(VIEW_RIGHT, "Right view", &["ctrl+3"]),
            view_command(VIEW_ISO, "Isometric view", &["ctrl+0"]),
            view_command(
                VIEW_PROJECTION_TOGGLE,
                "Toggle perspective and orthographic",
                &["ctrl+5"],
            ),
            document_command(DOCUMENT_NEXT, "Next document", &["ctrl+tab"]),
            document_command(DOCUMENT_PREVIOUS, "Previous document", &["ctrl+shift+tab"]),
            document_command(DOCUMENT_CLOSE, "Close document", &["ctrl+w"]),
            command(DOCUMENT_NEW, "New document", &["ctrl+n"]),
            document_command(EDIT_UNDO, "Undo", &["ctrl+z"]),
            document_command(EDIT_REDO, "Redo", &["ctrl+shift+z", "ctrl+y"]),
            palette_key(PALETTE_CLOSE, "Close the palette", &["escape", "ctrl+k"]),
            palette_key(PALETTE_NEXT, "Next palette result", &["down"]),
            palette_key(PALETTE_PREVIOUS, "Previous palette result", &["up"]),
            palette_key(PALETTE_RUN, "Run the selected palette result", &["enter"]),
        ];
        for ty in types {
            let (tab, group) = placement(&ty.id);
            if !tabs.iter().any(|t| t.id == tab) {
                tabs.push(TabSpec {
                    title: capitalised(&tab),
                    id: tab.clone(),
                });
            }
            commands.push(CommandSpec {
                id: format!("{}.create", ty.id),
                title: format!("New {}", ty.title.to_lowercase()),
                ribbon: Some(RibbonPlacement { tab, group }),
                keys: Vec::new(),
                when: None,
                needs_document: true,
                listed: true,
            });
        }
        let sketch_group = || {
            Some(RibbonPlacement {
                tab: CORE_SKETCH_TAB.into(),
                group: "Sketch".into(),
            })
        };
        commands.push(CommandSpec {
            ribbon: sketch_group(),
            ..document_command(SKETCH_EDIT, "Edit sketch", &[])
        });
        commands.push(CommandSpec {
            ribbon: sketch_group(),
            when: Some("sketch".into()),
            ..document_command(SKETCH_FINISH, "Finish sketch", &["escape"])
        });
        commands.push(CommandSpec {
            ribbon: sketch_group(),
            when: Some("sketch".into()),
            ..document_command(SKETCH_DELETE, "Delete", &["delete"])
        });
        for (id, title, keys) in [
            (SKETCH_SELECT, "Select", &[][..]),
            (SKETCH_LINE, "Line", &["l"][..]),
            (SKETCH_RECTANGLE, "Rectangle", &["r"][..]),
            (SKETCH_CIRCLE, "Circle", &["c"][..]),
            (SKETCH_ARC, "Arc", &["a"][..]),
        ] {
            commands.push(CommandSpec {
                ribbon: Some(RibbonPlacement {
                    tab: CORE_SKETCH_TAB.into(),
                    group: "Draw".into(),
                }),
                when: Some("sketch".into()),
                ..document_command(id, title, keys)
            });
        }
        commands.push(CommandSpec {
            when: Some("sketch-tool".into()),
            listed: false,
            ..document_command(SKETCH_TOOL_CANCEL, "End the drawing", &["escape"])
        });
        // One command per tab, so the palette and a key can open it.
        for tab in &tabs {
            commands.push(command(
                &tab_command_id(&tab.id),
                &format!("Show the {} tab", tab.title),
                &[],
            ));
        }
        Self { tabs, commands }
    }

    pub fn command(&self, id: &str) -> Option<&CommandSpec> {
        self.commands.iter().find(|c| c.id == id)
    }

    /// The tab that command `id` opens, when it is a tab's command.
    pub fn tab_of_command(&self, id: &str) -> Option<&TabSpec> {
        self.tabs.iter().find(|t| tab_command_id(&t.id) == id)
    }
}

/// The id of the command that opens ribbon tab `tab`: `core.ribbon.part`,
/// `core.ribbon.gears`.
fn tab_command_id(tab: &str) -> String {
    format!("core.ribbon.{}", tab.trim_start_matches("core."))
}

fn command(id: &str, title: &str, keys: &[&str]) -> CommandSpec {
    CommandSpec {
        id: id.into(),
        title: title.into(),
        ribbon: None,
        keys: keys.iter().map(|k| (*k).to_owned()).collect(),
        when: None,
        needs_document: false,
        listed: true,
    }
}

/// A command about the open documents, which there must be one of.
fn document_command(id: &str, title: &str, keys: &[&str]) -> CommandSpec {
    CommandSpec {
        needs_document: true,
        ..command(id, title, keys)
    }
}

/// A command about the view of the shown document, keyed in `part`.
fn view_command(id: &str, title: &str, keys: &[&str]) -> CommandSpec {
    CommandSpec {
        when: Some("part".into()),
        ..document_command(id, title, keys)
    }
}

/// A command of the palette's own, bound in its context and not listed in it.
fn palette_key(id: &str, title: &str, keys: &[&str]) -> CommandSpec {
    CommandSpec {
        when: Some("palette".into()),
        listed: false,
        ..command(id, title, keys)
    }
}

/// The tab id and group title of a feature type's creation command.
fn placement(type_id: &str) -> (String, String) {
    let plugin = type_id.split('.').next().unwrap_or(type_id);
    if plugin != "core" {
        (plugin.to_owned(), "Features".to_owned())
    } else if SKETCH_TYPES.contains(&type_id) {
        (CORE_SKETCH_TAB.to_owned(), "Sketch".to_owned())
    } else {
        (CORE_PART_TAB.to_owned(), "Features".to_owned())
    }
}

fn capitalised(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
