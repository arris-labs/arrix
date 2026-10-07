//! The application: the shell, the command registry, and the translation
//! between document and view types (docs/UI-RENDERING.md §The shell).

mod commands;
#[cfg(not(target_arch = "wasm32"))]
pub mod config_files;
mod debug;
#[cfg(not(target_arch = "wasm32"))]
mod document_files;
mod documents;
mod keys;
mod palette;
mod theme;

pub use debug::DebugState;

use arrix_config::{
    CONFIG_RELOAD, CommandRegistry, Config, ConfigDiagnostic, ConfigSource, DOCUMENT_CLOSE,
    DOCUMENT_NEXT, DOCUMENT_PREVIOUS, Keymap, MemorySource, PALETTE_CLOSE, PALETTE_NEXT,
    PALETTE_OPEN, PALETTE_PREVIOUS, PALETTE_RUN, PANEL_PROPERTIES_TOGGLE, PANEL_TREE_TOGGLE,
    ParamDefaults, Settings, Theme,
};
use arrix_core::RenderMesh;
use arrix_plugin_api::FeatureTypeSpec;
use arrix_ui::{
    CardsView, DocumentTab, DocumentTabsView, ShellLayout, ShellView, StatusView, UiIntent,
};
use arrix_viewport::{BodyKey, Renderer};
use documents::{CardToggles, OpenDocument, ViewState};
use palette::Palette;

/// Client-side state that is not in the document (SEED.md §6.1 rule 4).
struct Session {
    ribbon_tab: String,
    /// The palette, while it is open.
    palette: Option<Palette>,
    /// The cards toggled on the empty start; with a document open, its own
    /// (`OpenDocument::cards`) are the ones in force.
    cards: CardToggles,
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
    /// The renderer; `None` without a wgpu render state (a renderer other
    /// than wgpu), where the viewport stays an empty panel.
    renderer: Option<Renderer>,
    /// What the viewport shows while no document is open.
    idle_view: ViewState,
    /// The open documents, in the order they were opened, and which one is
    /// shown (an index; session state, no command).
    documents: Vec<OpenDocument>,
    active: usize,
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
                cards: CardToggles::default(),
                hovered_command: None,
            },
            drawn: None,
            renderer: cc
                .wgpu_render_state
                .as_ref()
                .map(|rs| Renderer::new(&rs.device, rs.target_format)),
            idle_view: ViewState::default(),
            documents: Vec::new(),
            active: 0,
        }
    }

    /// Opens the document `source` holds, named `path`, and shows it: its
    /// session starts and the evaluation and meshes arrive over the next
    /// frames (docs/CONCURRENCY-WASM.md §Roles and threads). Fails with the
    /// reason when it does not open; nothing changes then.
    pub fn open(
        &mut self,
        path: &str,
        source: &impl arrix_doc::DocumentSource,
    ) -> Result<(), String> {
        self.open_without(path, source, &[])
    }

    /// [`Self::open`] as a build without the plugins `without` would: their
    /// features open frozen (docs/DATA-MODEL.md §Frozen results).
    pub fn open_without(
        &mut self,
        path: &str,
        source: &impl arrix_doc::DocumentSource,
        without: &[&str],
    ) -> Result<(), String> {
        let mut registry = commands::feature_types();
        for id in without {
            let removed = arrix_core::PluginId::new(*id)
                .map(|plugin| registry.remove_plugin(&plugin))
                .unwrap_or(false);
            if !removed {
                return Err(format!("no plugin `{id}` in this build"));
            }
        }
        let title = path
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(path);
        let document = OpenDocument::open(title, path, source, registry)?;
        self.documents.push(document);
        self.active = self.documents.len() - 1;
        Ok(())
    }

    /// [`Self::open`] on a document directory or an `.arrx` file.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_path(&mut self, path: &std::path::Path) -> Result<(), String> {
        self.open_path_without(path, &[])
    }

    /// [`Self::open_without`] on a document directory or an `.arrx` file.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_path_without(
        &mut self,
        path: &std::path::Path,
        without: &[&str],
    ) -> Result<(), String> {
        let source = document_files::PathSource::read(path)?;
        self.open_without(&path.to_string_lossy(), &source, without)
    }

    /// The active document as it saves: its directory form's files, with
    /// each plugin feature's current frozen result. Not yet a file written
    /// (the UI's saving is a later plan); it is how a scenario opens the same
    /// document again as another build would.
    pub fn saved(&self) -> Option<arrix_doc::MemorySource> {
        self.active_document().map(OpenDocument::save)
    }

    fn active_document(&self) -> Option<&OpenDocument> {
        self.documents.get(self.active)
    }

    /// The card toggles in force: the active document's, else the empty
    /// start's.
    fn toggles_mut(&mut self) -> &mut CardToggles {
        match self.documents.get_mut(self.active) {
            Some(d) => &mut d.cards,
            None => &mut self.session.cards,
        }
    }

    /// Shows the document `delta` tabs along, wrapping.
    fn step_document(&mut self, delta: isize) {
        let n = self.documents.len() as isize;
        if n > 0 {
            self.active = (self.active as isize + delta).rem_euclid(n) as usize;
        }
    }

    /// Closes document `index`, which ends its session. The tab that takes
    /// its place (or the new last) is shown if it was the active one; none
    /// left is the empty start.
    fn close_document(&mut self, index: usize) {
        if index >= self.documents.len() {
            return;
        }
        self.documents.remove(index);
        if index < self.active || self.active >= self.documents.len() {
            self.active = self.active.saturating_sub(1);
        }
    }

    fn context(&self) -> commands::Context {
        commands::Context {
            document_open: !self.documents.is_empty(),
        }
    }

    /// Takes in what each session has said since the last frame. A document
    /// still waiting for its meshes asks for another frame soon; one that
    /// has said everything does not, so an idle app does not repaint.
    fn drain_documents(&mut self, ctx: &egui::Context) {
        const POLL: std::time::Duration = std::time::Duration::from_millis(16);
        let mut waiting = false;
        for document in &mut self.documents {
            document.drain(self.renderer.as_ref());
            waiting |= !document.settled();
        }
        if waiting {
            ctx.request_repaint_after(POLL);
        }
    }

    /// Shows `mesh` as `key`'s body in the active view, as a `Meshed` event
    /// does: how a test puts a hand-made mesh on screen. Without a wgpu
    /// render state it does nothing.
    pub fn show_mesh(&mut self, key: &BodyKey, mesh: &RenderMesh) {
        if let Some(renderer) = &self.renderer {
            let view = match self.documents.get_mut(self.active) {
                Some(d) => &mut d.view,
                None => &mut self.idle_view,
            };
            view.scene.set_body(renderer.device(), key, mesh);
        }
    }

    /// The active view, a document's or the empty start's.
    fn view_mut(&mut self) -> &mut ViewState {
        match self.documents.get_mut(self.active) {
            Some(d) => &mut d.view,
            None => &mut self.idle_view,
        }
    }

    /// The active view's pick, once its read-back has arrived; while it
    /// has not, another frame is asked for, since nothing else may come to
    /// wake the app.
    fn answer_pick(&mut self, ctx: &egui::Context) -> Option<UiIntent> {
        self.renderer.as_ref()?.poll();
        let view = self.view_mut();
        view.pick.as_ref()?;
        let answer = view.take_pick();
        if answer.is_none() {
            ctx.request_repaint();
        }
        answer.map(|name| UiIntent::Picked { name })
    }

    /// Where the world point `p` (metres) is drawn in the active view, in
    /// points, as of the last frame; `None` before one, or behind the
    /// camera. How a scenario finds a face to click.
    pub fn screen_point(&self, p: [f64; 3]) -> Option<egui::Pos2> {
        let rect = self.drawn.as_ref()?.layout.viewport;
        let view = self.active_document().map_or(&self.idle_view, |d| &d.view);
        let aspect = rect.width().max(1.0) / rect.height().max(1.0);
        let clip =
            view.camera.view_proj(aspect) * arrix_core::DVec3::from_array(p).as_vec3().extend(1.0);
        if clip.w <= 0.0 {
            return None;
        }
        let ndc = clip.truncate() / clip.w;
        Some(egui::pos2(
            rect.min.x + (ndc.x + 1.0) * 0.5 * rect.width(),
            rect.min.y + (1.0 - ndc.y) * 0.5 * rect.height(),
        ))
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
                self.context(),
            ),
            palette: self
                .session
                .palette
                .as_ref()
                .map(|p| p.view(&self.commands, &self.keymap, self.context())),
            tree: self
                .active_document()
                .map(OpenDocument::tree)
                .unwrap_or_default(),
            documents: DocumentTabsView {
                tabs: self
                    .documents
                    .iter()
                    .enumerate()
                    .map(|(i, d)| DocumentTab {
                        title: d.title.clone(),
                        path: d.path.clone(),
                        active: i == self.active,
                    })
                    .collect(),
            },
            cards: self.cards(),
            status: StatusView {
                hint: self.hint(),
                generation: self
                    .active_document()
                    .map_or(0, |d| d.replica.generation().0),
                diagnostics: self.config_diagnostics.len()
                    + self.active_document().map_or(0, |d| d.problems.len()),
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
        let toggles = self
            .active_document()
            .map_or(self.session.cards, |d| d.cards);
        CardsView {
            // `auto` shows the tree once the document has a part, and the
            // property card never: no feature is being edited yet.
            tree: shown(
                toggles.tree,
                "ui.panel.tree",
                self.active_document()
                    .is_some_and(|d| !d.replica.document().parts().is_empty()),
            ),
            properties: shown(toggles.properties, "ui.panel.properties", false),
        }
    }

    /// The status line's help text: the hovered command, its keys and why
    /// it is off; else what to do on an empty start.
    fn hint(&self) -> String {
        let cx = self.context();
        let hovered = self
            .session
            .hovered_command
            .as_deref()
            .and_then(|id| self.commands.command(id));
        let Some(spec) = hovered else {
            if let Some(document) = self.active_document() {
                return match document.settled() {
                    true => format!("{}: ready", document.title),
                    false => format!("{}: evaluating…", document.title),
                };
            }
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
            UiIntent::DocumentTabSelected { index } => {
                if index < self.documents.len() {
                    self.active = index;
                }
            }
            UiIntent::DocumentTabClosed { index } => self.close_document(index),
            UiIntent::Picked { name } => self.view_mut().selection = name,
        }
    }

    /// Runs a command, from a button or a key. One that doesn't apply in the
    /// current state does nothing.
    fn run(&mut self, command: &str) {
        let cx = self.context();
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
            PANEL_TREE_TOGGLE => {
                let shown = self.cards().tree;
                self.toggles_mut().tree = Some(!shown);
            }
            PANEL_PROPERTIES_TOGGLE => {
                let shown = self.cards().properties;
                self.toggles_mut().properties = Some(!shown);
            }
            DOCUMENT_NEXT => self.step_document(1),
            DOCUMENT_PREVIOUS => self.step_document(-1),
            DOCUMENT_CLOSE => self.close_document(self.active),
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
        // Not `egui_wants_keyboard_input`: that is any focused widget, a tab
        // reached by Tab included, and would silence every key.
        if ctx.text_edit_focused() {
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
        self.drain_documents(ui.ctx());
        self.run_keys(ui.ctx());
        self.apply_theme(ui);
        let window = ui.max_rect();
        let view = self.shell_view();
        let view_state = match self.documents.get_mut(self.active) {
            Some(d) => &mut d.view,
            None => &mut self.idle_view,
        };
        let renderer = &mut self.renderer;
        let response = arrix_ui::shell(ui, &view, |ui| {
            let rect = ui.max_rect();
            // The viewport's own response: a card over it is a layer above
            // and takes its clicks, so a click on a card picks nothing.
            let response = ui.interact(rect, ui.id().with("arrix.viewport"), egui::Sense::click());
            let click = response
                .clicked()
                .then(|| response.interact_pointer_pos())
                .flatten();
            if let Some(renderer) = renderer {
                let scene = &view_state.scene;
                let pick = renderer.paint(ui.painter(), rect, scene, &view_state.camera, click);
                // The latest click wins over one still on its way.
                if pick.is_some() {
                    view_state.pick = pick;
                }
            }
        });
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
        if let Some(intent) = self.answer_pick(ui.ctx()) {
            self.apply(intent);
        }
    }
}
