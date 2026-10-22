//! The application: the shell, the command registry, and the translation
//! between document and view types (docs/UI-RENDERING.md §The shell).

mod assembly_commands;
mod badges;
mod command_form;
mod commands;
mod component_form;
#[cfg(not(target_arch = "wasm32"))]
pub mod config_files;
mod context;
mod debug;
#[cfg(not(target_arch = "wasm32"))]
mod document_files;
mod documents;
mod file_dialog;
mod file_export;
mod file_import;
mod form;
mod form_pick;
mod gestures;
mod heard_line;
mod joint_drag;
mod joint_drag_app;
mod joint_form;
mod joint_form_app;
mod keys;
mod material_form;
mod palette;
mod part_sketch;
mod preview;
#[cfg(not(target_arch = "wasm32"))]
pub mod render;
mod sketch_constrain;
mod sketch_dimension;
mod sketch_drag;
mod sketch_mode;
mod sketch_modify;
mod sketch_tools;
mod sketch_view;
mod theme;
mod tree_assembly;
mod viewport;
#[cfg(target_arch = "wasm32")]
pub mod web;
#[cfg(target_arch = "wasm32")]
mod web_files;

pub use debug::DebugState;
pub use file_dialog::{NativeDialog, OpenDialog, SaveDialog};
pub use gestures::Gesture;

use arrix_config::{
    ASSEMBLY_CREATE, COMPONENT_CREATE, CONFIG_RELOAD, CommandRegistry, Config, ConfigDiagnostic,
    ConfigSource, DOCUMENT_CLOSE, DOCUMENT_NEW, DOCUMENT_NEXT, DOCUMENT_PREVIOUS, EDIT_REDO,
    EDIT_UNDO, FILE_EXPORT, FORM_CANCEL, FORM_OK, IMPORT_STEP_CREATE, JOINT_FIXED, JOINT_PRISMATIC,
    JOINT_REVOLUTE, Keymap, MemorySource, PALETTE_CLOSE, PALETTE_NEXT, PALETTE_OPEN,
    PALETTE_PREVIOUS, PALETTE_RUN, PANEL_PROPERTIES_TOGGLE, PANEL_TREE_TOGGLE, ParamDefaults,
    SELECTION_CLEAR, SKETCH_ARC, SKETCH_CIRCLE, SKETCH_CONSTRUCTION, SKETCH_CREATE, SKETCH_DELETE,
    SKETCH_DIMENSION, SKETCH_EDIT, SKETCH_EXTEND, SKETCH_FINISH, SKETCH_LINE, SKETCH_MIRROR,
    SKETCH_OFFSET, SKETCH_RECTANGLE, SKETCH_SELECT, SKETCH_TOOL_CANCEL, SKETCH_TRIM, Settings,
    Theme, VIEW_FRONT, VIEW_HOME, VIEW_ISO, VIEW_PROJECTION_TOGGLE, VIEW_RIGHT, VIEW_TOP,
};
use arrix_core::{FeatureId, IdMinter, RenderMesh};
use arrix_plugin_api::FeatureTypeSpec;
use arrix_ui::{
    CardsView, DocumentTab, DocumentTabsView, FieldEvent, ShellLayout, ShellView, StatusView,
    UiIntent,
};
use arrix_viewport::{BodyKey, Renderer, StandardView};
use documents::{CardToggles, OpenDocument, PickFor, ViewState};
use palette::Palette;
use sketch_constrain::Shape;
use viewport::SketchDrag;

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
    /// Why the last gesture did nothing, for the status line until the next
    /// one: nothing fails silently.
    notice: Option<String>,
    gestures: gestures::Gestures,
    /// How many forms this session has opened: names each one's text
    /// buffers.
    forms_opened: u64,
    /// Whether shift is held, as of the frame's start: a tree click that
    /// picks into a list field adds to it.
    shift: bool,
    /// When, on egui's clock, a dwell under way completes: a frame is
    /// asked for then, since a pointer at rest sends nothing.
    wake_at: Option<f64>,
    /// Why the feature whose badge the pointer is over is frozen, as of the
    /// last frame: the status line's hint.
    badge_hover: Option<String>,
    /// Whether the status line has said that evaluation can't be
    /// interrupted on this page: it says so once (ADR-0042).
    said_uninterruptible: bool,
}

/// What the status line says, once, when a document opens on a page that
/// is not cross-origin isolated (ADR-0042 §Without isolation).
pub const UNINTERRUPTIBLE: &str =
    "Evaluation can't be interrupted on this page: it is not cross-origin isolated";

/// A seed for the id minter: the clock, which a session's ids need only to
/// differ from the last session's, whose ids a document may hold.
/// The entity a pick names, of the component at `path` (the ids as the
/// intent carries them); a path that does not parse is no hit.
fn hit_of(
    name: Option<arrix_core::PersistentName>,
    path: &[String],
) -> Option<arrix_viewport::Hit> {
    let path: Result<Vec<arrix_core::ComponentId>, _> = path.iter().map(|c| c.parse()).collect();
    Some(arrix_viewport::Hit {
        path: path.ok()?,
        name: name?,
    })
}

fn entropy() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0x5EED, |d| d.as_nanos() as u64)
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
    /// The plugin commands the registry holds, each a button on its
    /// plugin's tab (ADR-0038 §10).
    plugin_commands: Vec<arrix_plugin_api::CommandSpec>,
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
    /// Mints the ids of what this client's commands create (SEED.md §6.1).
    ids: IdMinter,
    /// Asked where an export goes.
    dialog: Box<dyn SaveDialog>,
    /// Asked which file an import reads.
    open_dialog: Box<dyn OpenDialog>,
}

struct Drawn {
    window: egui::Rect,
    view: ShellView,
    layout: ShellLayout,
    /// The frozen badges the viewport drew.
    badges: Vec<arrix_ui::BadgeView>,
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

    /// The app on `ctx` with no window, rendering through `render_state`
    /// when there is one: what the headless render (`render`) draws.
    pub fn headless(
        ctx: &egui::Context,
        render_state: Option<&eframe::egui_wgpu::RenderState>,
        source: &impl ConfigSource,
    ) -> Self {
        Self::build_on(ctx, render_state, source)
    }

    fn build(cc: &eframe::CreationContext<'_>, source: &impl ConfigSource) -> Self {
        Self::build_on(&cc.egui_ctx, cc.wgpu_render_state.as_ref(), source)
    }

    fn build_on(
        ctx: &egui::Context,
        render_state: Option<&eframe::egui_wgpu::RenderState>,
        source: &impl ConfigSource,
    ) -> Self {
        let types = commands::feature_types();
        let commands = commands::command_registry(&types);
        let specs = commands::feature_type_specs(&types);
        let config = Config::load(source, &commands, &specs, &arrix_doc::expr::check_constant);
        let visuals = theme::view(&config.theme).visuals(&ctx.global_style().visuals);
        ctx.set_visuals(visuals);
        Self {
            commands,
            settings: config.settings,
            keymap: config.keymap,
            theme: config.theme,
            feature_types: specs,
            plugin_commands: Vec::new(),
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
                notice: None,
                gestures: gestures::Gestures::default(),
                forms_opened: 0,
                shift: false,
                wake_at: None,
                badge_hover: None,
                said_uninterruptible: false,
            },
            drawn: None,
            renderer: render_state.map(|rs| Renderer::new(&rs.device, rs.target_format)),
            idle_view: ViewState::default(),
            documents: Vec::new(),
            active: 0,
            ids: IdMinter::new(entropy()),
            dialog: Box::new(NativeDialog),
            open_dialog: Box::new(NativeDialog),
        }
    }

    /// Mints the ids of every later command from `seed` instead of the
    /// clock, so a scenario's records and goldens are the same every run.
    pub fn seed_ids(&mut self, seed: u64) {
        self.ids = IdMinter::new(seed);
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
        remove_plugins(&mut registry, without)?;
        self.open_in(path, source, registry)
    }

    /// Opens the document `source` holds on `registry`'s feature types.
    fn open_in(
        &mut self,
        path: &str,
        source: &impl arrix_doc::DocumentSource,
        registry: arrix_doc::Registry,
    ) -> Result<(), String> {
        let title = path
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(path);
        self.take_in_plugins(&registry);
        let document = OpenDocument::open(title, path, source, registry)?;
        self.show_document(document);
        Ok(())
    }

    /// The shown document's newest finished generation's eval line, as
    /// `arrix eval` prints it, read from its session's events alone
    /// (`Evaluation::heard`); `None` before one has finished.
    pub fn eval_line(&self) -> Option<arrix_doc::EvalLine> {
        Some(self.active_document()?.eval_line()?.1.clone())
    }

    /// Adds `document` as the last tab and shows it. The first whose
    /// evaluation can't be interrupted (a browser page without isolation)
    /// has the status line say so.
    fn show_document(&mut self, document: OpenDocument) {
        if !document.interruptible() && !self.session.said_uninterruptible {
            self.session.said_uninterruptible = true;
            self.session.notice = Some(UNINTERRUPTIBLE.to_owned());
        }
        self.documents.push(document);
        self.active = self.documents.len() - 1;
    }

    /// The feature types and commands `registry` holds that the app does not
    /// know yet, such as a plugin directory's: they join the command
    /// registry and so the ribbon.
    fn take_in_plugins(&mut self, registry: &arrix_doc::Registry) {
        let mut changed = false;
        for spec in registry.creatable_specs() {
            if !self.feature_types.iter().any(|t| t.id == spec.id) {
                self.feature_types.push(spec);
                changed = true;
            }
        }
        for spec in registry.command_specs() {
            match self.plugin_commands.iter_mut().find(|c| c.id == spec.id) {
                Some(held) => *held = spec,
                None => self.plugin_commands.push(spec),
            }
            changed = true;
        }
        if changed {
            self.rebuild_commands();
        }
    }

    /// The command registry again, from the feature types and the plugin
    /// commands the app holds.
    fn rebuild_commands(&mut self) {
        self.commands = arrix_config::CommandRegistry::new(&self.feature_types)
            .with_plugin_commands(&self.plugin_commands);
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

    /// [`Self::open_path_without`] with each plugin directory of
    /// `plugins` (Tier 1 or 2) loaded first (`arrix-app render --plugin`), so its
    /// features evaluate live; one of `without` may name such a plugin too.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn open_path_with(
        &mut self,
        path: &std::path::Path,
        plugins: &[std::path::PathBuf],
        without: &[&str],
    ) -> Result<(), String> {
        let source = document_files::PathSource::read(path)?;
        let mut registry = commands::feature_types();
        let compiled: Vec<_> = registry.ids().filter_map(|id| id.plugin()).collect();
        for dir in plugins {
            let named = |err: &dyn std::fmt::Display| format!("{}: {err}", dir.display());
            let manifest = arrix_plugin_host::manifest_in(dir).map_err(|err| named(&err))?;
            if compiled.contains(&manifest.id) {
                registry.remove_plugin(&manifest.id);
            }
            arrix_plugin_host::register_dir(&mut registry, dir).map_err(|err| named(&err))?;
        }
        remove_plugins(&mut registry, without)?;
        self.open_in(&path.to_string_lossy(), &source, registry)
    }

    /// The active document as it saves: its directory form's files, with
    /// each plugin feature's current frozen result. Not yet a file written
    /// (the UI's saving is a later plan); it is how a scenario opens the same
    /// document again as another build would.
    pub fn saved(&self) -> Option<arrix_doc::MemorySource> {
        self.active_document().map(OpenDocument::save)
    }

    /// A new, empty document in a new tab, shown. Nothing happens when no
    /// thread can be had; the reason is the notice.
    fn new_document(&mut self) {
        let n = self.documents.len() + 1;
        match OpenDocument::empty(&format!("Untitled {n}"), commands::feature_types()) {
            Ok(document) => self.show_document(document),
            Err(reason) => self.session.notice = Some(reason),
        }
    }

    /// Runs `command` for the shown document and says why it did nothing.
    fn edit(&mut self, f: impl FnOnce(&mut OpenDocument) -> Result<bool, String>) {
        if let Some(document) = self.documents.get_mut(self.active)
            && let Err(reason) = f(document)
        {
            self.session.notice = Some(reason);
        }
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
        let mode = self.active_document().and_then(|d| d.sketch.as_ref());
        commands::Context {
            document_open: !self.documents.is_empty(),
            sketching: mode.is_some(),
            tool: mode.map(|m| m.drawing.tool.command()),
            form_open: self.form_open(),
            assembly_in_view: self
                .active_document()
                .is_some_and(|d| matches!(d.context(), Some(context::Context::Assembly(_)))),
            sketch_selected: self.selected_sketch().is_some(),
            sketch_selection: self
                .active_document()
                .and_then(|d| d.sketch.as_ref())
                .is_some_and(|m| !m.selection.is_empty()),
            shape: mode
                .and_then(|m| Some(Shape::of(&m.solved.as_ref()?.sketch, &m.selection)))
                .unwrap_or_default(),
        }
    }

    /// Rebuilds the command registry and the forms' types from the feature
    /// types a session loaded or dropped (ADR-0024). The app loads none
    /// itself.
    fn follow_registry(&mut self) {
        let mut changed = false;
        for document in &mut self.documents {
            let changes = document.take_type_changes();
            for id in &changes.removed {
                self.feature_types.retain(|t| &t.id != id);
                changed = true;
            }
            for spec in changes.added {
                match self.feature_types.iter_mut().find(|t| t.id == spec.id) {
                    Some(held) => *held = spec,
                    None => self.feature_types.push(spec),
                }
                changed = true;
            }
        }
        if changed {
            self.rebuild_commands();
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
            waiting |=
                !document.settled() || document.exports.pending() || document.imports.pending();
        }
        #[cfg(target_arch = "wasm32")]
        self.take_in_files(ctx);
        self.follow_registry();
        self.write_exports();
        self.finish_imports(ctx);
        self.follow_sketches();
        self.follow_forms();
        self.follow_expansions();
        self.follow_previews();
        for document in &mut self.documents {
            document.follow_part_sketches();
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

    /// Draws the active view's bodies as `instances`, or each once at its
    /// origin with `None`: how a test places hand-made meshes as an
    /// assembly's components would be.
    pub fn show_instances(&mut self, instances: Option<Vec<arrix_viewport::Instance>>) {
        self.view_mut().scene.set_instances(instances);
    }

    /// What the active view's last click landed on, its component path
    /// included.
    pub fn last_click(&self) -> Option<arrix_viewport::Hit> {
        let view = self.active_document().map_or(&self.idle_view, |d| &d.view);
        view.last_click.clone()
    }

    /// The active tab looks at `context` from now on, unless it is already
    /// there: a tree click is session state, no command.
    fn look_at(&mut self, context: context::Context) {
        if let Some(document) = self.documents.get_mut(self.active)
            && document.context() != Some(context)
        {
            document.view.component_selection = None;
            document.set_context(context);
        }
    }

    /// Switches the active tab to the part or assembly `spec` names, by id
    /// or by name (`arrix-app render --context`).
    pub fn set_context(&mut self, spec: &str) -> Result<(), String> {
        let document = self
            .documents
            .get_mut(self.active)
            .ok_or("no document is open")?;
        let context = context::Context::parse(document.replica.document(), spec)?;
        document.set_context(context);
        Ok(())
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
        // The answer is drawn next frame; nothing else may wake the app for
        // it, nor for a read-back still on its way.
        ctx.request_repaint();
        let (purpose, hit) = answer?;
        if purpose == PickFor::Drag {
            self.begin_joint_drag(hit);
            return None;
        }
        let (name, path) = match hit {
            Some(hit) => (
                Some(hit.name),
                hit.path.iter().map(|c| c.0.to_string()).collect(),
            ),
            None => (None, Vec::new()),
        };
        Some(match purpose {
            PickFor::Click => UiIntent::Picked { name, path },
            PickFor::Hover | PickFor::Drag => UiIntent::Hovered { name, path },
        })
    }

    /// Where the world point `p` (metres) is drawn in the active view, in
    /// points, as of the last frame; `None` before one, or behind the
    /// camera. How a scenario finds a face to click.
    pub fn screen_point(&self, p: [f64; 3]) -> Option<egui::Pos2> {
        let rect = self.drawn.as_ref()?.layout.viewport;
        let view = self.active_document().map_or(&self.idle_view, |d| &d.view);
        badges::project(&view.camera, rect, p)
    }

    /// Finishes every running camera animation at once, so a scenario sees
    /// the view a command or a click flew to without waiting for the clock.
    pub fn skip_animations(&mut self) {
        self.idle_view.skip_animation();
        for document in &mut self.documents {
            document.view.skip_animation();
        }
    }

    /// Where the ViewCube's facet for `orientation` is, in points, as of the
    /// last frame; `None` before one, or for a facet the view has culled.
    /// How a scenario clicks the cube.
    pub fn viewcube_facet(
        &self,
        orientation: arrix_viewport::ViewOrientation,
    ) -> Option<egui::Pos2> {
        let drawn = self.drawn.as_ref()?;
        let rect = arrix_ui::viewcube_rect(drawn.layout.viewport, &drawn.view.cards);
        let view = self.active_document().map_or(&self.idle_view, |d| &d.view);
        let camera = &view.camera;
        arrix_ui::viewcube::project_viewcube(rect, camera.yaw, camera.pitch)
            .into_iter()
            .find(|facet| facet.orientation == orientation)
            .map(|facet| facet.center_2d)
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
            form: self.form_view(),
            status: StatusView {
                hint: self.hint(),
                generation: self
                    .active_document()
                    .map_or(0, |d| d.replica.generation().0),
                diagnostics: self.config_diagnostics.len()
                    + self.active_document().map_or(0, |d| d.problems.len()),
                length_unit: "mm".into(),
                angle_unit: "deg".into(),
                sketch: self.sketch_status(),
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
            // property card while a form is open.
            tree: shown(
                toggles.tree,
                "ui.panel.tree",
                self.active_document()
                    .is_some_and(|d| !d.replica.document().parts().is_empty()),
            ),
            properties: shown(toggles.properties, "ui.panel.properties", self.form_open()),
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
            if let Some(notice) = &self.session.notice {
                return notice.clone();
            }
            if let Some(hint) = self.joint_drag_hint() {
                return hint;
            }
            if let Some(reason) = &self.session.badge_hover {
                return format!("{reason}.");
            }
            if let Some(document) = self.active_document() {
                if let Some(mode) = &document.sketch {
                    return self.sketch_hint(document, mode);
                }
                if let Some(hit) = &document.view.hover {
                    let name = match document.component_label(&hit.path) {
                        Some(label) => format!("{} in {label}", hit.name),
                        None => hit.name.to_string(),
                    };
                    return match document.view.selection.as_ref() == Some(hit) {
                        true => format!("{name}: selected. Escape clears the selection."),
                        false => format!("{name}: click to select it."),
                    };
                }
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
        if let Some(why) = commands::needs(spec, cx) {
            hint = format!("{hint}: {why}");
        }
        hint
    }

    fn apply(&mut self, intent: UiIntent) {
        // A hover is not a gesture, nor is a drag's start or a frame of it:
        // the gesture is the drag, closed by its release.
        let gesture = !matches!(
            intent,
            UiIntent::Hovered { .. }
                | UiIntent::SketchHovered { .. }
                | UiIntent::SketchPointerMoved { .. }
                | UiIntent::SketchDragStarted { .. }
                | UiIntent::SketchDragMoved { .. }
                | UiIntent::DimensionEdited { .. }
                | UiIntent::FieldEdited { .. }
        );
        match gesture {
            true => self.gesture(&gestures::what(&intent), |app| app.apply_intent(intent)),
            false => self.apply_intent(intent),
        }
    }

    fn apply_intent(&mut self, intent: UiIntent) {
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
            UiIntent::Picked { name, path } => {
                let hit = hit_of(name, &path);
                if !self.form_pick_topo(hit.clone()) {
                    self.view_mut().selection = hit;
                }
            }
            UiIntent::SketchHovered { hit } => self.hover_sketch(hit),
            UiIntent::SketchClicked {
                hit,
                extend,
                pointer,
            } => self.click_sketch(hit, extend, pointer),
            UiIntent::SketchPointerMoved { pointer } => self.sketch_pointer(pointer),
            UiIntent::SketchDragStarted { hit, pointer } => self.drag_started(hit, pointer),
            UiIntent::SketchDragMoved { pointer } => self.drag_moved(pointer),
            UiIntent::SketchDragEnded => self.drag_ended(),
            UiIntent::SketchDoubleClicked { hit } => self.open_dimension_field(hit),
            UiIntent::DimensionCommitted { constraint, text } => {
                self.commit_dimension(&constraint, &text);
            }
            UiIntent::DimensionCancelled => self.close_dimension_field(),
            UiIntent::DimensionEdited { constraint, text } => {
                self.edit_dimension_field(&constraint, text);
            }
            UiIntent::TreePartActivated { part } => {
                if let Ok(part) = part.parse::<arrix_core::PartId>() {
                    self.look_at(context::Context::Part(part));
                }
            }
            UiIntent::TreePartEdited { part } => {
                if let Ok(id) = part.parse::<arrix_core::PartId>() {
                    self.look_at(context::Context::Part(id));
                }
                self.open_material_form(&part);
            }
            UiIntent::TreeAssemblyActivated { assembly } => {
                if let Ok(assembly) = assembly.parse::<arrix_core::AssemblyId>() {
                    self.look_at(context::Context::Assembly(assembly));
                }
            }
            UiIntent::TreeComponentSelected { assembly, path } => {
                let path: Result<Vec<arrix_core::ComponentId>, _> =
                    path.iter().map(|c| c.parse()).collect();
                if let (Ok(assembly), Ok(path)) = (assembly.parse::<arrix_core::AssemblyId>(), path)
                {
                    self.view_mut().component_selection = Some((assembly, path));
                }
            }
            UiIntent::TreeComponentActivated { assembly, path } => {
                self.open_component_edit(&assembly, &path);
            }
            UiIntent::TreeJointActivated { assembly, joint } => {
                if let Ok(id) = assembly.parse::<arrix_core::AssemblyId>() {
                    self.look_at(context::Context::Assembly(id));
                }
                self.open_joint_edit(&assembly, &joint);
            }
            UiIntent::TreeFeatureSelected { feature } => {
                if let Ok(feature) = feature.parse::<FeatureId>()
                    && !self.form_pick_feature(feature)
                {
                    self.view_mut().feature_selection = Some(feature);
                }
            }
            UiIntent::TreeFeatureActivated { feature } => {
                if let Ok(feature) = feature.parse::<FeatureId>() {
                    self.activate_feature(feature);
                }
            }
            UiIntent::Hovered { name, path } => self.view_mut().hover = hit_of(name, &path),
            intent @ (UiIntent::FieldEdited { .. }
            | UiIntent::FieldCommitted { .. }
            | UiIntent::NameCommitted { .. }
            | UiIntent::ChoiceSelected { .. }
            | UiIntent::ReferenceFieldActivated { .. }
            | UiIntent::ReferenceRemoved { .. }
            | UiIntent::CandidatePicked { .. }
            | UiIntent::FormOk
            | UiIntent::FormCancelled) => self.form_intent(intent),
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
            SELECTION_CLEAR => {
                let view = self.view_mut();
                view.selection = None;
                view.component_selection = None;
            }
            VIEW_HOME => self.view_mut().home(),
            VIEW_TOP => self.view_mut().turn_to(StandardView::Top),
            VIEW_FRONT => self.view_mut().turn_to(StandardView::Front),
            VIEW_RIGHT => self.view_mut().turn_to(StandardView::Right),
            VIEW_ISO => self.view_mut().turn_to(StandardView::Iso),
            VIEW_PROJECTION_TOGGLE => self.view_mut().camera.toggle_projection(),
            DOCUMENT_NEXT => self.step_document(1),
            DOCUMENT_PREVIOUS => self.step_document(-1),
            DOCUMENT_CLOSE => self.close_document(self.active),
            DOCUMENT_NEW => self.new_document(),
            FILE_EXPORT => self.export_file(),
            IMPORT_STEP_CREATE => self.import_step(),
            ASSEMBLY_CREATE => self.create_assembly(),
            COMPONENT_CREATE => self.open_component_create(),
            JOINT_FIXED => self.open_joint_create(arrix_doc::JointKind::Fixed),
            JOINT_REVOLUTE => self.open_joint_create(arrix_doc::JointKind::Revolute),
            JOINT_PRISMATIC => self.open_joint_create(arrix_doc::JointKind::Prismatic),
            SKETCH_CREATE => self.create_sketch(),
            SKETCH_EDIT => self.edit_sketch(),
            SKETCH_FINISH => self.finish_sketch(),
            SKETCH_DELETE => self.delete_selection(),
            SKETCH_SELECT => self.set_tool(sketch_tools::Tool::Select),
            SKETCH_LINE => self.set_tool(sketch_tools::Tool::Line),
            SKETCH_RECTANGLE => self.set_tool(sketch_tools::Tool::Rectangle),
            SKETCH_CIRCLE => self.set_tool(sketch_tools::Tool::Circle),
            SKETCH_ARC => self.set_tool(sketch_tools::Tool::Arc),
            SKETCH_DIMENSION => self.set_tool(sketch_tools::Tool::Dimension),
            SKETCH_TRIM => self.set_tool(sketch_tools::Tool::Trim),
            SKETCH_EXTEND => self.set_tool(sketch_tools::Tool::Extend),
            SKETCH_OFFSET => self.set_tool(sketch_tools::Tool::Offset),
            SKETCH_MIRROR => self.set_tool(sketch_tools::Tool::Mirror),
            SKETCH_TOOL_CANCEL => self.cancel_tool(),
            SKETCH_CONSTRUCTION => self.toggle_construction(),
            EDIT_UNDO => self.edit(OpenDocument::undo),
            EDIT_REDO => self.edit(OpenDocument::redo),
            CONFIG_RELOAD => self.reload(),
            FORM_OK => self.form_ok(),
            FORM_CANCEL => self.form_cancel(),
            other => {
                if self.plugin_commands.iter().any(|c| c.id == other) {
                    self.open_command_form(other);
                    return;
                }
                if let Some(ty) = other.strip_suffix(".create") {
                    self.open_create_form(ty);
                    return;
                }
                if let Some(kind) = sketch_constrain::Kind::of_command(other) {
                    self.constrain(kind);
                }
            }
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

    /// The keymap contexts active now, most specific first. The palette,
    /// while open, owns the keyboard: `text` and its own context, so its
    /// field keeps the keys. Otherwise `text` while a text field has
    /// focus, then `sketch-tool` with a drawing tool and `sketch` in sketch
    /// mode, or `part` outside it.
    fn key_contexts(&self, ctx: &egui::Context) -> Vec<&'static str> {
        if self.session.palette.is_some() {
            return vec!["text", "palette"];
        }
        let mut contexts = Vec::new();
        // Not `egui_wants_keyboard_input`: that is any focused widget, a tab
        // reached by Tab included, and would silence every key. A focused
        // text field owns the keyboard: the letters of a dimension's
        // expression are not the sketch's tool keys.
        // An open dimension field counts though Escape, which egui takes
        // focus away with before any key is read, has just left it.
        let field_open = self
            .active_document()
            .and_then(|d| d.sketch.as_ref())
            .is_some_and(|m| m.field.is_some());
        if ctx.text_edit_focused() || field_open {
            return vec!["text"];
        }
        if self.form_open() {
            contexts.push("form");
        }
        match self.context().tool {
            Some(tool) if tool != SKETCH_SELECT => contexts.extend(["sketch-tool", "sketch"]),
            Some(_) => contexts.push("sketch"),
            None => {
                contexts.push("part");
                let looking = self.active_document().and_then(|d| d.context());
                if matches!(looking, Some(context::Context::Assembly(_))) {
                    contexts.push("assembly");
                }
            }
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
                self.gesture(&format!("key {command}"), |app| app.run(&command));
            }
        }
    }
}

/// Leaves each plugin of `without` out of `registry`, as a build without
/// it would; fails on the first it does not have.
fn remove_plugins(registry: &mut arrix_doc::Registry, without: &[&str]) -> Result<(), String> {
    for id in without {
        let removed = arrix_core::PluginId::new(*id)
            .map(|plugin| registry.remove_plugin(&plugin))
            .unwrap_or(false);
        if !removed {
            return Err(format!("no plugin `{id}` in this build"));
        }
    }
    Ok(())
}

impl eframe::App for ArrixApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }

    /// The page's handle reaches the app through this (`web::WebHandle`).
    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(&mut *self)
    }
}

impl ArrixApp {
    /// One frame of the app, as eframe's `ui` runs it; the headless render
    /// (`render`) runs it on its own context.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        self.poll_config(ui.ctx());
        self.drain_documents(ui.ctx());
        self.run_keys(ui.ctx());
        self.apply_theme(ui);
        self.session.shift = ui.input(|i| i.modifiers.shift);
        let marks = self.form_marks();
        let component_marks = self
            .active_document()
            .map_or_else(Vec::new, OpenDocument::component_marks);
        let window = ui.max_rect();
        let view = self.shell_view();
        let overlay = self.sketch_overlay();
        let part = self.part_sketches();
        let anchors = self
            .active_document()
            .map_or_else(Vec::new, OpenDocument::badge_anchors);
        let drags = self
            .active_document()
            .is_some_and(OpenDocument::drags_joints);
        let view_state = match self.documents.get_mut(self.active) {
            Some(d) => &mut d.view,
            None => &mut self.idle_view,
        };
        view_state.marked = marks;
        view_state.component_marks = component_marks;
        let renderer = self.renderer.as_mut();
        let mut frame = None;
        let response = arrix_ui::shell(ui, &view, |ui| {
            frame = Some(viewport::show(
                ui,
                view_state,
                renderer,
                &view.cards,
                !view.documents.tabs.is_empty(),
                overlay.as_ref(),
                viewport::Overlays {
                    part: &part,
                    anchors: &anchors,
                    drags,
                },
            ));
        });
        let hover_left = frame.as_ref().is_some_and(|f| f.hover_left);
        let now = ui.input(|i| i.time);
        let part_pointer = frame.as_mut().map(|f| std::mem::take(&mut f.part));
        let (badges, badge_hover) = frame
            .as_mut()
            .map(|f| (std::mem::take(&mut f.badges), f.badge_hover.take()))
            .unwrap_or_default();
        let scene_drag = frame
            .as_mut()
            .map(|f| std::mem::take(&mut f.drag))
            .unwrap_or_default();
        let sketch_pointer = frame.and_then(|f| f.sketch);
        self.drawn = Some(Drawn {
            window,
            view,
            layout: response.layout,
            badges,
        });
        if badge_hover != self.session.badge_hover {
            self.session.badge_hover = badge_hover;
            // The hint is drawn from it next frame.
            ui.ctx().request_repaint();
        }
        if response.hovered_command != self.session.hovered_command {
            self.session.hovered_command = response.hovered_command;
            // The hint is drawn from it next frame.
            ui.ctx().request_repaint();
        }
        for intent in response.intents {
            self.apply(intent);
        }
        if hover_left {
            self.apply(UiIntent::Hovered {
                name: None,
                path: Vec::new(),
            });
            ui.ctx().request_repaint();
        }
        for step in scene_drag {
            self.joint_drag_step(step);
            ui.ctx().request_repaint();
        }
        if let Some(pointer) = part_pointer {
            if let Some(hover) = pointer.hover {
                self.hover_part_sketch(hover);
                ui.ctx().request_repaint();
            }
            if let Some((feature, item, extend)) = pointer.click {
                self.gesture("part sketch pick", |app| {
                    app.form_pick_sketch(feature, item, extend);
                });
            }
        }
        if let Some(pointer) = sketch_pointer {
            if let Some(hit) = pointer.hover {
                self.apply(UiIntent::SketchHovered { hit });
                ui.ctx().request_repaint();
            }
            self.apply(UiIntent::SketchPointerMoved {
                pointer: pointer.at,
            });
            for step in pointer.drag {
                self.apply(match step {
                    SketchDrag::Started(hit, pointer) => {
                        UiIntent::SketchDragStarted { hit, pointer }
                    }
                    SketchDrag::Moved(pointer) => UiIntent::SketchDragMoved { pointer },
                    SketchDrag::Ended => UiIntent::SketchDragEnded,
                });
                ui.ctx().request_repaint();
            }
            if let Some(hit) = pointer.double_click {
                self.apply(UiIntent::SketchDoubleClicked { hit });
                ui.ctx().request_repaint();
            }
            match pointer.field {
                Some(FieldEvent::Commit { constraint, text }) => {
                    self.apply(UiIntent::DimensionCommitted { constraint, text });
                    ui.ctx().request_repaint();
                }
                Some(FieldEvent::Edited { constraint, text }) => {
                    self.apply(UiIntent::DimensionEdited { constraint, text });
                    ui.ctx().request_repaint();
                }
                Some(FieldEvent::Cancel) => {
                    self.apply(UiIntent::DimensionCancelled);
                    ui.ctx().request_repaint();
                }
                None => {}
            }
            if let Some((hit, extend, at)) = pointer.click {
                self.apply(UiIntent::SketchClicked {
                    hit,
                    extend,
                    pointer: at,
                });
                ui.ctx().request_repaint();
            }
        }
        if let Some(at) = self.session.wake_at.take() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64((at - now).max(0.0)));
        }
        if let Some(intent) = self.answer_pick(ui.ctx()) {
            self.apply(intent);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrix_doc::{Generation, SessionEvent};

    fn spec(id: &str) -> FeatureTypeSpec {
        FeatureTypeSpec {
            id: id.into(),
            version: 1,
            title: "Standoff".into(),
            params: vec![],
            choices: vec![],
            inputs: vec![],
            outputs: vec![],
        }
    }

    /// A `SessionEvent::Registry` it hears gives the app the commands and
    /// the form of a type it never registered, and takes them away again
    /// (ADR-0024 §6).
    #[test]
    fn the_app_follows_the_registry_a_session_announces() {
        let ctx = egui::Context::default();
        let mut app = ArrixApp::headless(&ctx, None, &MemorySource::default());
        let doc = OpenDocument::empty("t", commands::feature_types()).unwrap();
        app.documents.push(doc);
        assert!(app.commands.command("standoff.post.create").is_none());

        let announce =
            |added: Vec<FeatureTypeSpec>, removed: Vec<&str>, generation| SessionEvent::Registry {
                generation: Generation(generation),
                added,
                removed: removed
                    .into_iter()
                    .map(|id| arrix_doc::FeatureTypeId::new(id).unwrap())
                    .collect(),
            };
        app.documents[0].receive(&announce(vec![spec("standoff.post")], vec![], 1), None);
        app.follow_registry();
        assert!(app.commands.command("standoff.post.create").is_some());
        assert!(app.feature_types.iter().any(|t| t.id == "standoff.post"));

        app.documents[0].receive(&announce(vec![], vec!["standoff.post"], 2), None);
        app.follow_registry();
        assert!(app.commands.command("standoff.post.create").is_none());
        assert!(!app.feature_types.iter().any(|t| t.id == "standoff.post"));
    }
}
