//! The application: the shell, the command registry, and the translation
//! between document and view types (docs/UI-RENDERING.md §The shell).

mod commands;
#[cfg(not(target_arch = "wasm32"))]
pub mod config_files;
mod debug;
mod keys;
mod palette;
mod theme;

pub use debug::DebugState;

use arrix_config::{
    CONFIG_RELOAD, CommandRegistry, Config, ConfigDiagnostic, ConfigSource, Keymap, MemorySource,
    PALETTE_CLOSE, PALETTE_NEXT, PALETTE_OPEN, PALETTE_PREVIOUS, PALETTE_RUN,
    PANEL_PROPERTIES_TOGGLE, PANEL_TREE_TOGGLE, ParamDefaults, Settings, Theme,
};
use arrix_plugin_api::FeatureTypeSpec;
use arrix_ui::{
    CardsView, DocumentTabsView, ShellLayout, ShellView, StatusView, TreeView, UiIntent,
};
use palette::Palette;

/// Client-side state that is not in the document (SEED.md §6.1 rule 4).
struct Session {
    ribbon_tab: String,
    /// The palette, while it is open.
    palette: Option<Palette>,
    /// A card the user toggled this session: it wins over `ui.panel.*`
    /// (ADR-0017). `None` leaves the setting in charge.
    tree_card: Option<bool>,
    properties_card: Option<bool>,
    /// The command of the ribbon button or palette row under the pointer,
    /// as of the last frame: the status line's hint is about it.
    hovered_command: Option<String>,
}

/// The application state eframe drives.
pub struct ArrixApp {
    session: Session,
    commands: CommandRegistry,
    settings: Settings,
    keymap: Keymap,
    theme: Theme,
    /// The registered feature types, and the user's defaults for their forms.
    feature_types: Vec<FeatureTypeSpec>,
    defaults: ParamDefaults,
    /// What the config files got wrong, for `debug_state()` and the status bar.
    config_diagnostics: Vec<ConfigDiagnostic>,
    /// Where the configuration came from, for `core.config.reload`; `None`
    /// is the built-in configuration, which has nothing to re-read.
    source: Option<Box<dyn ConfigSource>>,
    /// The source's fingerprint when it was last read, and the time it was
    /// last compared (egui's clock, in seconds).
    fingerprint: Option<u64>,
    last_poll: f64,
    /// What the last frame drew, for [`ArrixApp::debug_state`].
    drawn: Option<Drawn>,
}

struct Drawn {
    window: egui::Rect,
    view: ShellView,
    layout: ShellLayout,
}

impl ArrixApp {
    /// The app on the built-in configuration.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::with_config(cc, &MemorySource::default())
    }

    /// The app on the configuration `source` holds over the built-in one.
    pub fn with_config(cc: &eframe::CreationContext<'_>, source: &impl ConfigSource) -> Self {
        Self::build(cc, source)
    }

    /// The app on `source`, which it keeps to re-read on
    /// `core.config.reload` and, where the source can tell, when its files
    /// change.
    pub fn with_config_source(
        cc: &eframe::CreationContext<'_>,
        source: Box<dyn ConfigSource>,
    ) -> Self {
        let mut app = Self::build(cc, &&*source);
        app.fingerprint = source.fingerprint();
        app.source = Some(source);
        app
    }

    /// The app on the user's config directory: `$ARRIX_CONFIG_DIR`, else the
    /// platform's. A directory that isn't there is the built-in configuration.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn native(cc: &eframe::CreationContext<'_>) -> Self {
        match config_files::ConfigDir::from_environment() {
            Some(dir) => Self::with_config_source(cc, Box::new(dir)),
            None => Self::new(cc),
        }
    }

    fn build(cc: &eframe::CreationContext<'_>, source: &impl ConfigSource) -> Self {
        let types = commands::feature_types();
        let commands = commands::command_registry(&types);
        let specs = commands::feature_type_specs(&types);
        let config = Config::load(source, &commands, &specs, &arrix_doc::expr::check_constant);
        let visuals = theme::view(&config.theme).visuals(&cc.egui_ctx.global_style().visuals);
        cc.egui_ctx.set_visuals(visuals);
        Self {
            commands,
            settings: config.settings,
            keymap: config.keymap,
            theme: config.theme,
            feature_types: specs,
            defaults: config.defaults,
            config_diagnostics: config.diagnostics,
            source: None,
            fingerprint: None,
            last_poll: 0.0,
            session: Session {
                ribbon_tab: "core.sketch".into(),
                palette: None,
                tree_card: None,
                properties_card: None,
                hovered_command: None,
            },
            drawn: None,
        }
    }

    /// The initial form of a new feature of `type_id`: each parameter's
    /// name and expression, with the user's defaults over the type's own.
    /// M3's document plan seeds the property panel from it.
    pub fn effective_params(&self, type_id: &str) -> Option<Vec<(String, String)>> {
        let ty = self.feature_types.iter().find(|t| t.id == type_id)?;
        Some(self.defaults.effective_params(ty))
    }

    /// The view the shell is drawn from: a projection of the session (and,
    /// from M1, of the document and its events).
    fn shell_view(&self) -> ShellView {
        ShellView {
            theme: theme::view(&self.theme),
            ribbon: commands::ribbon(
                &self.commands,
                &self.settings,
                &self.session.ribbon_tab,
                commands::Context::default(),
            ),
            palette: self
                .session
                .palette
                .as_ref()
                .map(|p| p.view(&self.commands, &self.keymap, commands::Context::default())),
            tree: TreeView::default(),
            documents: DocumentTabsView::default(),
            cards: self.cards(),
            status: StatusView {
                hint: self.hint(),
                generation: 0,
                diagnostics: self.config_diagnostics.len(),
                length_unit: "mm".into(),
                angle_unit: "deg".into(),
            },
        }
    }

    /// Which cards show: the session's toggle, else the setting, where
    /// `auto` is whether there is something to show (ADR-0017).
    fn cards(&self) -> CardsView {
        let shown = |toggled: Option<bool>, setting: &str, has_content: bool| {
            toggled.unwrap_or(match self.settings.string(setting) {
                "shown" => true,
                "hidden" => false,
                _ => has_content,
            })
        };
        CardsView {
            // No document is open yet, so there is no part to list; and no
            // feature is edited, so nothing for the property card.
            tree: shown(self.session.tree_card, "ui.panel.tree", false),
            properties: shown(self.session.properties_card, "ui.panel.properties", false),
        }
    }

    /// The status line's help text: the hovered command, its keys and why
    /// it is off; else what to do on an empty start.
    fn hint(&self) -> String {
        let cx = commands::Context::default();
        let hovered = self
            .session
            .hovered_command
            .as_deref()
            .and_then(|id| self.commands.command(id));
        let Some(spec) = hovered else {
            let keys = self.keymap.keys_of(PALETTE_OPEN);
            return match keys.first() {
                Some(key) => format!("No document is open. {key} opens the command palette."),
                None => "No document is open.".into(),
            };
        };
        let mut hint = spec.title.clone();
        let keys = self.keymap.keys_of(&spec.id);
        if !keys.is_empty() {
            hint = format!("{hint} ({})", keys.join(", "));
        }
        if !commands::enabled(spec, cx) {
            hint.push_str(": needs an open document");
        }
        hint
    }

    fn apply(&mut self, intent: UiIntent) {
        match intent {
            UiIntent::RibbonTabSelected { tab } => self.session.ribbon_tab = tab,
            UiIntent::CommandInvoked { command } => {
                // Running a result closes the palette, as Enter does.
                self.session.palette = None;
                self.run(&command);
            }
            UiIntent::PaletteQuery { text } => {
                if let Some(p) = &mut self.session.palette {
                    p.query = text;
                    p.selected = 0;
                }
            }
            UiIntent::PaletteClosed => self.session.palette = None,
        }
    }

    /// Runs a command, from a button or a key. One that doesn't apply in the
    /// current state does nothing.
    fn run(&mut self, command: &str) {
        let cx = commands::Context::default();
        let Some(spec) = self.commands.command(command) else {
            return;
        };
        if !commands::enabled(spec, cx) {
            return;
        }
        if let Some(tab) = self.commands.tab_of_command(command) {
            self.session.ribbon_tab = tab.id.clone();
            return;
        }
        match command {
            PALETTE_OPEN => self.session.palette = Some(Palette::default()),
            PALETTE_CLOSE => self.session.palette = None,
            PALETTE_NEXT => self.step_palette(1),
            PALETTE_PREVIOUS => self.step_palette(-1),
            PALETTE_RUN => self.run_selected(cx),
            PANEL_TREE_TOGGLE => self.session.tree_card = Some(!self.cards().tree),
            PANEL_PROPERTIES_TOGGLE => {
                self.session.properties_card = Some(!self.cards().properties);
            }
            CONFIG_RELOAD => self.reload(),
            // The creation commands arrive with their steps and plans.
            _ => {}
        }
    }

    /// Re-reads the config files and applies them. A bad key keeps its
    /// default and is a diagnostic; the app stays up. With no source (the
    /// built-in configuration) there is nothing to re-read.
    fn reload(&mut self) {
        let Some(source) = &self.source else {
            return;
        };
        let config = Config::load(
            &&**source,
            &self.commands,
            &self.feature_types,
            &arrix_doc::expr::check_constant,
        );
        self.fingerprint = source.fingerprint();
        self.settings = config.settings;
        self.keymap = config.keymap;
        self.theme = config.theme;
        self.defaults = config.defaults;
        self.config_diagnostics = config.diagnostics;
    }

    /// The polled check for changed files (ADR-0015, ⚠ OPEN 3): about once a
    /// second, a source that can fingerprint its files is compared with what
    /// was last read, and a change reloads. It costs a few `stat`s.
    fn poll_config(&mut self, ctx: &egui::Context) {
        const EVERY: f64 = 1.0;
        let Some(source) = &self.source else {
            return;
        };
        if source.fingerprint().is_none() {
            return;
        }
        let now = ctx.input(|i| i.time);
        if now - self.last_poll >= EVERY {
            self.last_poll = now;
            if source.fingerprint() != self.fingerprint {
                self.reload();
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(EVERY));
    }

    fn step_palette(&mut self, delta: isize) {
        if let Some(p) = &mut self.session.palette {
            p.step(&self.commands, delta);
        }
    }

    /// Runs the palette's selected result, if it applies, and closes it.
    fn run_selected(&mut self, cx: commands::Context) {
        let Some(p) = &self.session.palette else {
            return;
        };
        let results = p.results(&self.commands);
        let Some(chosen) = results.get(p.selected.min(results.len().saturating_sub(1))) else {
            return;
        };
        if commands::enabled(chosen, cx) {
            let id = chosen.id.clone();
            self.session.palette = None;
            self.run(&id);
        }
    }

    /// The keymap contexts active now. The palette, while open, owns the
    /// keyboard: its own context and `text`, so its field keeps the keys.
    /// Otherwise `part` (no sketch is edited yet), and `text` while a text
    /// field has focus.
    fn key_contexts(&self, ctx: &egui::Context) -> Vec<&'static str> {
        if self.session.palette.is_some() {
            return vec!["palette", "text"];
        }
        let mut contexts = vec!["part"];
        if ctx.egui_wants_keyboard_input() {
            contexts.push("text");
        }
        contexts
    }

    /// Makes the theme the look of this frame, and of the floating windows
    /// after it. A change of theme shows here; the first frame's is set at
    /// creation, where it costs no repaint.
    fn apply_theme(&self, ui: &mut egui::Ui) {
        let current = ui.ctx().global_style().visuals.clone();
        let visuals = theme::view(&self.theme).visuals(&current);
        if current != visuals {
            ui.ctx().set_visuals(visuals.clone());
        }
        ui.style_mut().visuals = visuals;
    }

    fn run_keys(&mut self, ctx: &egui::Context) {
        let contexts = self.key_contexts(ctx);
        for chord in keys::pressed(ctx) {
            if let Some(command) = self.keymap.resolve(&chord, &contexts) {
                let command = command.to_owned();
                // A key that ran a command is spent: a focused field must
                // not also act on it.
                keys::consume(ctx, &chord);
                self.run(&command);
            }
        }
    }
}

impl eframe::App for ArrixApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll_config(ui.ctx());
        self.run_keys(ui.ctx());
        self.apply_theme(ui);
        let window = ui.max_rect();
        let view = self.shell_view();
        let response = arrix_ui::shell(ui, &view, |_ui| {});
        self.drawn = Some(Drawn {
            window,
            view,
            layout: response.layout,
        });
        if response.hovered_command != self.session.hovered_command {
            self.session.hovered_command = response.hovered_command;
            // The hint is drawn from it next frame.
            ui.ctx().request_repaint();
        }
        for intent in response.intents {
            self.apply(intent);
        }
    }
}
