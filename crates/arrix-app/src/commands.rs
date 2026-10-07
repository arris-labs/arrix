//! The command registry in use: `arrix-config` holds the data, and this
//! module knows which feature types there are, whether a command applies in
//! the current session state, and what the ribbon shows of it
//! (docs/UI-RENDERING.md §The shell).

use std::sync::Arc;

use arrix_config::{
    CommandRegistry, CommandSpec, SKETCH_CREATE, SKETCH_DELETE, SKETCH_EDIT, SKETCH_FINISH,
    SKETCH_LINE, SKETCH_RECTANGLE, SKETCH_SELECT, SKETCH_TOOL_CANCEL, Settings,
};
use arrix_doc::Registry;
use arrix_plugin_api::FeatureTypeSpec;
use arrix_plugin_host::register_tier0;
use arrix_ui::{RibbonCommand, RibbonGroup, RibbonTab, RibbonView};

/// The core feature types and every compiled-in plugin's: this binary's
/// Tier 0 list (docs/PLUGINS.md §Three tiers), like the CLI's.
pub fn feature_types() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, arrix_gears::MANIFEST, Arc::new(arrix_gears::Gears))
        .expect("a first-party plugin fits the host it is built with");
    r
}

/// The spec of every registered feature type, in registration order.
pub fn feature_type_specs(types: &Registry) -> Vec<FeatureTypeSpec> {
    types
        .ids()
        .filter_map(|id| types.get(id))
        .map(|ty| ty.spec().clone())
        .collect()
}

/// The registry of commands for every registered feature type.
pub fn command_registry(types: &Registry) -> CommandRegistry {
    CommandRegistry::new(&feature_type_specs(types))
}

/// What decides whether a command applies.
#[derive(Clone, Copy, Debug, Default)]
pub struct Context {
    pub document_open: bool,
    /// The shown document is in sketch mode.
    pub sketching: bool,
    /// The command id of sketch mode's tool, in sketch mode.
    pub tool: Option<&'static str>,
    /// The tree's selection is a sketch that can be opened.
    pub sketch_selected: bool,
    /// Something is selected in the open sketch.
    pub sketch_selection: bool,
}

/// Why `command` does not apply, in the words of the status line's hint
/// (after `title: `); `None` when it does.
pub fn needs(command: &CommandSpec, cx: Context) -> Option<&'static str> {
    if command.needs_document && !cx.document_open {
        return Some("needs an open document");
    }
    match command.id.as_str() {
        SKETCH_FINISH if !cx.sketching => Some("needs sketch mode"),
        SKETCH_EDIT if cx.sketching => Some("finish the sketch first"),
        SKETCH_EDIT if !cx.sketch_selected => Some("needs a sketch selected in the tree"),
        SKETCH_DELETE | SKETCH_SELECT | SKETCH_LINE | SKETCH_RECTANGLE | SKETCH_TOOL_CANCEL
            if !cx.sketching =>
        {
            Some("needs sketch mode")
        }
        SKETCH_DELETE if !cx.sketch_selection => Some("needs something selected in the sketch"),
        SKETCH_CREATE if cx.sketching => Some("finish the sketch first"),
        _ => None,
    }
}

pub fn enabled(command: &CommandSpec, cx: Context) -> bool {
    needs(command, cx).is_none()
}

/// The ribbon from the registry: its tabs minus `ui.ribbon.hidden-tabs`,
/// and the groups of the open one. `open` falls back to the first visible
/// tab when it is hidden or unknown.
pub fn ribbon(
    registry: &CommandRegistry,
    settings: &Settings,
    open: &str,
    cx: Context,
) -> RibbonView {
    let hidden = settings.string_list("ui.ribbon.hidden-tabs");
    let tabs: Vec<RibbonTab> = registry
        .tabs
        .iter()
        .filter(|t| !hidden.contains(&t.id))
        .map(|t| RibbonTab {
            id: t.id.clone(),
            title: t.title.clone(),
        })
        .collect();
    let active = tabs
        .iter()
        .find(|t| t.id == open)
        .or(tabs.first())
        .map(|t| t.id.clone())
        .unwrap_or_default();
    let mut groups: Vec<RibbonGroup> = Vec::new();
    for command in &registry.commands {
        let Some(placement) = command.ribbon.as_ref().filter(|p| p.tab == active) else {
            continue;
        };
        let button = RibbonCommand {
            id: command.id.clone(),
            title: command.title.clone(),
            enabled: enabled(command, cx),
            active: cx.tool == Some(command.id.as_str()),
        };
        match groups.iter_mut().find(|g| g.title == placement.group) {
            Some(group) => group.commands.push(button),
            None => groups.push(RibbonGroup {
                title: placement.group.clone(),
                commands: vec![button],
            }),
        }
    }
    RibbonView {
        tabs,
        active,
        groups,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn built() -> (CommandRegistry, Settings) {
        (command_registry(&feature_types()), Settings::default())
    }

    fn tab_ids(view: &RibbonView) -> Vec<&str> {
        view.tabs.iter().map(|t| t.id.as_str()).collect()
    }

    #[test]
    fn the_gears_plugin_adds_a_tab_with_its_creation_command() {
        let (registry, settings) = built();
        let view = ribbon(&registry, &settings, "gears", Context::default());
        assert_eq!(tab_ids(&view), ["core.sketch", "core.part", "gears"]);
        assert_eq!(view.active, "gears");
        let ids: Vec<_> = view.groups[0].commands.iter().map(|c| &c.id[..]).collect();
        assert_eq!(ids, ["gears.spur.create"]);
    }

    #[test]
    fn creation_commands_wait_for_a_document() {
        let (registry, settings) = built();
        let off = ribbon(&registry, &settings, "gears", Context::default());
        assert!(!off.groups[0].commands[0].enabled);
        let on = ribbon(
            &registry,
            &settings,
            "gears",
            Context {
                document_open: true,
                ..Context::default()
            },
        );
        assert!(on.groups[0].commands[0].enabled);
    }

    #[test]
    fn a_hidden_tab_goes_core_ones_included_and_the_open_one_falls_back() {
        let (registry, mut settings) = built();
        let d = settings.apply(
            "settings.toml",
            "[ui.ribbon]\nhidden-tabs = [\"core.part\", \"core.sketch\"]\n",
        );
        assert!(d.is_empty(), "{d:?}");
        let view = ribbon(&registry, &settings, "core.sketch", Context::default());
        assert_eq!(tab_ids(&view), ["gears"]);
        assert_eq!(view.active, "gears");
    }
}
