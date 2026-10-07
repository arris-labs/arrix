//! The application: the shell, the command registry, and the translation
//! between document and view types (docs/UI-RENDERING.md §The shell).

mod commands;
mod debug;
mod keys;

pub use debug::DebugState;

use arrix_config::{
    CommandRegistry, ConfigDiagnostic, ConfigSource, Keymap, MemorySource, Settings,
};
use arrix_ui::{ShellLayout, ShellView, StatusView, TreeView, UiIntent};

/// Client-side state that is not in the document (SEED.md §6.1 rule 4).
struct Session {
    ribbon_tab: String,
}

/// The application state eframe drives.
pub struct ArrixApp {
    session: Session,
    commands: CommandRegistry,
    settings: Settings,
    keymap: Keymap,
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
    pub fn with_config(_cc: &eframe::CreationContext<'_>, source: &impl ConfigSource) -> Self {
        let commands = commands::command_registry(&commands::feature_types());
        let (settings, mut config_diagnostics) = Settings::load(source);
        let (keymap, keymap_diagnostics) = Keymap::load(source, &commands);
        config_diagnostics.extend(keymap_diagnostics);
        Self {
            commands,
            settings,
            keymap,
            config_diagnostics,
            session: Session {
                ribbon_tab: "core.sketch".into(),
            },
            drawn: None,
        }
    }

    /// The view the shell is drawn from: a projection of the session (and,
    /// from M1, of the document and its events).
    fn shell_view(&self) -> ShellView {
        ShellView {
            ribbon: commands::ribbon(
                &self.commands,
                &self.settings,
                &self.session.ribbon_tab,
                commands::Context::default(),
            ),
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
            UiIntent::CommandInvoked { command } => self.run(&command),
        }
    }

    /// Runs a command, from a button or a key. One that doesn't apply in the
    /// current state does nothing.
    fn run(&mut self, command: &str) {
        let Some(spec) = self.commands.command(command) else {
            return;
        };
        if !commands::enabled(spec, commands::Context::default()) {
            return;
        }
        if let Some(tab) = self.commands.tab_of_command(command) {
            self.session.ribbon_tab = tab.id.clone();
        }
        // The other commands (the palette, reload, the panel toggles and the
        // creation commands) arrive with their steps and plans.
    }

    /// The keymap contexts active now: `part` (no sketch is edited yet) and
    /// `text` while a text field has focus.
    fn key_contexts(ctx: &egui::Context) -> Vec<&'static str> {
        let mut contexts = vec!["part"];
        if ctx.egui_wants_keyboard_input() {
            contexts.push("text");
        }
        contexts
    }

    fn run_keys(&mut self, ctx: &egui::Context) {
        let contexts = Self::key_contexts(ctx);
        for chord in keys::pressed(ctx) {
            if let Some(command) = self.keymap.resolve(&chord, &contexts) {
                let command = command.to_owned();
                self.run(&command);
            }
        }
    }
}

impl eframe::App for ArrixApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.run_keys(ui.ctx());
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
