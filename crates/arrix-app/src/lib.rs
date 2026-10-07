//! The application: the shell, the command registry, and the translation
//! between document and view types (docs/UI-RENDERING.md §The shell).

mod commands;
mod debug;
mod keys;
mod palette;
mod theme;

pub use debug::DebugState;

use arrix_config::{
    CommandRegistry, ConfigDiagnostic, ConfigSource, Keymap, MemorySource, PALETTE_CLOSE,
    PALETTE_NEXT, PALETTE_OPEN, PALETTE_PREVIOUS, PALETTE_RUN, Settings, Theme,
};
use arrix_ui::{ShellLayout, ShellView, StatusView, TreeView, UiIntent};
use palette::Palette;

/// Client-side state that is not in the document (SEED.md §6.1 rule 4).
struct Session {
    ribbon_tab: String,
    /// The palette, while it is open.
    palette: Option<Palette>,
}

/// The application state eframe drives.
pub struct ArrixApp {
    session: Session,
    commands: CommandRegistry,
    settings: Settings,
    keymap: Keymap,
    theme: Theme,
    /// What the config files got wrong, for `debug_state()` and the status bar.
    config_diagnostics: Vec<ConfigDiagnostic>,
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
        let commands = commands::command_registry(&commands::feature_types());
        let (settings, mut config_diagnostics) = Settings::load(source);
        let (keymap, keymap_diagnostics) = Keymap::load(source, &commands);
        config_diagnostics.extend(keymap_diagnostics);
        let (theme, theme_diagnostics) = Theme::load(settings.string("ui.theme"), source);
        config_diagnostics.extend(theme_diagnostics);
        let visuals = theme::view(&theme).visuals(&cc.egui_ctx.global_style().visuals);
        cc.egui_ctx.set_visuals(visuals);
        Self {
            commands,
            settings,
            keymap,
            theme,
            config_diagnostics,
            session: Session {
                ribbon_tab: "core.sketch".into(),
                palette: None,
            },
            drawn: None,
        }
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
            status: StatusView {
                generation: 0,
                diagnostics: 0,
                length_unit: "mm".into(),
                angle_unit: "deg".into(),
            },
        }
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
            // The other commands (reload, the panel toggles and the
            // creation commands) arrive with their steps and plans.
            _ => {}
        }
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
        for intent in response.intents {
            self.apply(intent);
        }
    }
}
