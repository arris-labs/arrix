//! The open documents: each one a [`LocalSession`], the replica of its
//! applied stream, and the scene and camera it is looked at through
//! (docs/UI-RENDERING.md §The shell; the viewport's state is session state,
//! SEED.md §6.1 rule 4).

use std::collections::BTreeSet;
use std::time::Duration;

use crate::context::{self, Context};
use crate::sketch_mode::SketchMode;
use arrix_core::{Diagnostic, FeatureId, Id, MeshDetail, PersistentName};
use arrix_doc::{
    AuthorId, Command, CommandEnvelope, Document, DocumentSource, EventStream, FeatureOutcome,
    FrozenInfo, FrozenReason, Generation, LocalSession, Outcome, Registry, Replica, Session,
    SessionEvent,
};
use arrix_ui::{FeatureState, TreeFeature, TreePart, TreeView};
use arrix_viewport::{BodyKey, GpuMesh, Hit, OrbitCamera, PendingPick, Renderer, Scene};
use web_time::Instant;

/// How long after an evaluation finished with no mesh heard it is taken to
/// have no body to mesh. Meshing starts the moment `Finished` is
/// published, so this only has to outlast a hop between threads and a busy
/// machine; the wall clock, since egui's can run ahead of it in a test.
const NO_BODIES_AFTER: Duration = Duration::from_secs(1);

/// The local user, as the authority knows them: undo is per author
/// (SEED.md §6.1), and in local mode there is one.
pub(crate) const LOCAL_AUTHOR: AuthorId = AuthorId(Id(1));

/// What is drawn and from where: the bodies, and the camera on them; what
/// is selected among them, and the pick on its way.
#[derive(Default)]
pub struct ViewState {
    pub scene: Scene<GpuMesh>,
    pub camera: OrbitCamera,
    pub selection: Option<PersistentName>,
    /// The feature selected in the tree.
    pub feature_selection: Option<FeatureId>,
    /// What the pointer is over, as of the last hover pick that came back.
    pub hover: Option<PersistentName>,
    /// Where the pointer was when a hover pick was last asked for; a pick
    /// is asked for again only once it has moved.
    pub hover_at: Option<egui::Pos2>,
    /// The last pick asked for, until its read-back arrives, and what it
    /// was asked for.
    pub pick: Option<(PendingPick, PickFor)>,
    /// The item of a finished sketch the pointer is over, while a form's
    /// field asks for one.
    pub part_hover: Option<(FeatureId, arrix_ui::PartSketchItem)>,
    /// Whether shift was held at the last click in the viewport: a pick
    /// into a list field adds to it.
    pub click_extend: bool,
    /// What the open form's reference fields hold, drawn as the selection
    /// is. Set every frame from the form.
    pub marked: Vec<PersistentName>,
    /// What the last click's pick landed on, its component path included;
    /// the selection keeps the name alone until picks carry the path
    /// (plans/c2-m2 step 6).
    pub last_click: Option<Hit>,
}

/// What a pick is for: a click selects, the pointer resting hovers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickFor {
    Click,
    Hover,
}

impl ViewState {
    /// What the pick on its way resolved to, once its read-back has
    /// arrived: what it was for, and the entity's name or `None` for empty
    /// space. Polled once a frame, after [`Renderer::poll`].
    pub fn take_pick(&mut self) -> Option<(PickFor, Option<PersistentName>)> {
        let (pending, purpose) = self.pick.as_ref()?;
        let hit = pending.poll()?;
        let hit = hit.and_then(|h| self.scene.resolve(&pending.bodies, h));
        let purpose = *purpose;
        self.pick = None;
        if purpose == PickFor::Click {
            self.last_click = hit.clone();
        }
        Some((purpose, hit.map(|h| h.name)))
    }
}

/// Where a feature stands and what evaluation said of it: the tree's tag
/// and the form's header read the same answer.
pub(crate) struct Standing {
    pub state: FeatureState,
    /// The failure, or the first thing worth knowing of a feature that
    /// evaluated.
    pub diagnostic: Option<Diagnostic>,
    /// Set when the feature shows a stored result.
    pub frozen: Option<FrozenInfo>,
}

/// Why a feature of `type_id` shows a stored result, in the words the form
/// and the badge share.
pub(crate) fn frozen_reason(type_id: impl std::fmt::Display, info: &FrozenInfo) -> String {
    match info.reason {
        FrozenReason::Missing => format!("Feature type {type_id} is not available"),
        FrozenReason::Version => {
            format!("Feature type {type_id} is not at the version the result was made under")
        }
    }
}

/// `record`, at `index` of `part`, as the replica's heard outcomes have it.
pub(crate) fn standing_of(
    replica: &Replica,
    part: &arrix_doc::Part,
    index: usize,
    record: &arrix_doc::FeatureRecord,
) -> Standing {
    let plain = |state| Standing {
        state,
        diagnostic: None,
        frozen: None,
    };
    if part.rollback.is_some_and(|bar| index >= bar) {
        return plain(FeatureState::RolledBack);
    }
    if record.suppressed {
        return plain(FeatureState::Suppressed);
    }
    match replica.outcome(record.id).map(|(_, o)| o) {
        None => plain(FeatureState::Evaluating),
        Some(FeatureOutcome::Suppressed) => plain(FeatureState::Suppressed),
        Some(FeatureOutcome::RolledBack) => plain(FeatureState::RolledBack),
        Some(FeatureOutcome::Failed { diagnostic, .. }) => Standing {
            state: FeatureState::Failed,
            diagnostic: Some(diagnostic.clone()),
            frozen: None,
        },
        Some(FeatureOutcome::Ok {
            frozen,
            diagnostics,
            ..
        }) => Standing {
            state: match frozen {
                Some(info) if info.stale => FeatureState::FrozenStale,
                Some(_) => FeatureState::Frozen,
                None => FeatureState::Ok,
            },
            diagnostic: diagnostics.first().cloned(),
            frozen: *frozen,
        },
    }
}

/// The tree card's view of the part `part`: each feature with where it
/// stands, from the replica's document and the outcomes it has heard.
fn tree_part(replica: &Replica, part: &arrix_doc::Part, selected: Option<FeatureId>) -> TreePart {
    let features = part
        .history
        .iter()
        .enumerate()
        .filter_map(|(index, id)| Some((index, part.features.get(id)?)))
        .map(|(index, record)| {
            let standing = standing_of(replica, part, index, record);
            let noted = matches!(
                standing.state,
                FeatureState::Failed | FeatureState::Frozen | FeatureState::FrozenStale
            );
            TreeFeature {
                id: record.id.0.to_string(),
                name: record.name.clone(),
                kind: record.type_id.to_string(),
                state: standing.state,
                note: standing.diagnostic.filter(|_| noted).map(|d| d.message),
                selected: selected == Some(record.id),
            }
        })
        .collect();
    TreePart {
        id: part.id.0.to_string(),
        name: part.name.clone(),
        features,
    }
}

/// A card the user toggled: `None` leaves the setting in charge.
#[derive(Clone, Copy, Default)]
pub struct CardToggles {
    pub tree: Option<bool>,
    pub properties: Option<bool>,
}

/// One open document.
/// What a session said about its feature types after it opened.
#[derive(Debug, Default)]
pub struct TypeChanges {
    /// Specs loaded, new or replacing.
    pub added: Vec<arrix_plugin_api::FeatureTypeSpec>,
    /// Ids no longer there.
    pub removed: Vec<String>,
}

pub struct OpenDocument {
    pub title: String,
    /// Where it was opened from; the tab's tooltip.
    pub path: String,
    /// The cards this document's user toggled: they win over `ui.panel.*`
    /// (ADR-0017), and stay with the tab.
    pub cards: CardToggles,
    events: EventStream,
    _session: LocalSession,
    /// The feature types the session has said it loaded or dropped
    /// (ADR-0024), until the app takes them (`take_type_changes`).
    type_changes: TypeChanges,
    pub replica: Replica,
    pub view: ViewState,
    /// The sketch being edited, while there is one: session state, one
    /// sketch per tab.
    pub sketch: Option<SketchMode>,
    /// The form open in the property card, while there is one: a create
    /// form's draft. Session state, one per tab.
    pub form: Option<crate::form::Form>,
    /// The finished sketches, solved, for part mode to draw.
    pub part_sketches: std::collections::BTreeMap<FeatureId, crate::part_sketch::PartSketch>,
    /// The create form's preview, while one is asked for (ADR-0019).
    pub preview: crate::preview::FormPreview,
    /// What the user switched this tab to; else it looks where the
    /// document opens (ADR-0030 §1).
    chosen: Option<Context>,
    /// Whether the camera has been fitted to the bodies: it is once, on
    /// the first mesh, and never again, so a later edit does not take the
    /// view away from the user.
    framed: bool,
    /// An assembly's first fit is redone as each coarse mesh lands, since
    /// the first body to arrive is not the whole: until a refined one does.
    fitting: bool,
    mesh: MeshProgress,
    /// When `Finished` of the current generation was heard.
    finished_at: Option<(Generation, Instant)>,
    settled: bool,
    /// The stream could not be followed: nothing more will be heard.
    broken: bool,
    /// The exports asked for and the files that came back.
    pub(crate) exports: crate::file_export::Exports,
    /// What went wrong that is not the document's own: a body that did not
    /// mesh, a stream the replica could not follow.
    pub problems: Vec<String>,
    applied: usize,
    /// Which open document this is, for the gesture log: unique in the
    /// process, unlike a title or a tab's index.
    pub(crate) serial: u64,
    /// The commands closed gestures have counted on this document; every
    /// command runs inside one, so it equals `applied` between gestures.
    pub(crate) gestured: usize,
}

/// The next document's serial.
static SERIALS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// The meshes heard of one generation: every body coarse, then every body
/// refined, so the refined ones start once the coarse ones are all in.
#[derive(Default)]
struct MeshProgress {
    generation: Generation,
    coarse: usize,
    refined: usize,
    /// The bodies of this generation, to drop the others when the refined
    /// meshes start.
    seen: BTreeSet<BodyKey>,
}

impl OpenDocument {
    /// Opens the document `source` holds and starts its session. Fails with
    /// the reason, for the user: it does not open, or no thread can be had.
    pub fn open(
        title: &str,
        path: &str,
        source: &impl DocumentSource,
        registry: Registry,
    ) -> Result<Self, String> {
        let document = arrix_doc::open(source).map_err(|e| format!("{path}: {e}"))?;
        Self::start(title, path, document, registry)
    }

    /// A new document with nothing in it: no part until something needs one.
    pub fn empty(title: &str, registry: Registry) -> Result<Self, String> {
        Self::start(title, "", Document::default(), registry)
    }

    fn start(
        title: &str,
        path: &str,
        document: Document,
        registry: Registry,
    ) -> Result<Self, String> {
        let session = LocalSession::open(document, registry)
            .map_err(|e| format!("{path}: the session does not start: {e}"))?;
        Ok(Self {
            title: title.to_owned(),
            path: path.to_owned(),
            cards: CardToggles::default(),
            events: session.subscribe(),
            _session: session,
            replica: Replica::default(),
            view: ViewState::default(),
            sketch: None,
            form: None,
            part_sketches: std::collections::BTreeMap::new(),
            preview: crate::preview::FormPreview::default(),
            chosen: None,
            framed: false,
            fitting: false,
            mesh: MeshProgress::default(),
            finished_at: None,
            settled: false,
            broken: false,
            exports: Default::default(),
            problems: Vec::new(),
            applied: 0,
            serial: SERIALS.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            gestured: 0,
            type_changes: TypeChanges::default(),
        })
    }

    /// The session the tab's requests go to.
    pub(crate) fn session(&self) -> &LocalSession {
        &self._session
    }

    /// How many commands the authority has applied for this tab's author,
    /// undo and redo included: a gesture's count is the difference.
    pub fn applied(&self) -> usize {
        self.applied
    }

    /// Submits `command` against the generation this replica has heard. The
    /// reason it was not applied, for the user, is the error; a command
    /// whose effect is the current state is `Ok(false)`.
    pub fn submit(&mut self, command: Command) -> Result<bool, String> {
        let envelope = CommandEnvelope {
            author: LOCAL_AUTHOR,
            base: self.replica.generation(),
            command,
        };
        self.count(self._session.submit(envelope))
    }

    pub fn undo(&mut self) -> Result<bool, String> {
        self.count(self._session.undo(LOCAL_AUTHOR))
    }

    pub fn redo(&mut self) -> Result<bool, String> {
        self.count(self._session.redo(LOCAL_AUTHOR))
    }

    fn count(&mut self, outcome: Result<Outcome, arrix_doc::Rejected>) -> Result<bool, String> {
        match outcome {
            Ok(Outcome::Applied(_)) => {
                self.applied += 1;
                Ok(true)
            }
            // A preview is not counted: it applies nothing (ADR-0019).
            Ok(
                Outcome::NoOp | Outcome::Previewing | Outcome::Exporting | Outcome::Loaded { .. },
            ) => Ok(false),
            Err(rejected) => Err(rejected.to_string()),
        }
    }

    /// Whether everything the document's latest generation will say has
    /// been heard: evaluated, and every body meshed refined. Meshes and
    /// events arrive from the session's threads, so a frame that wants to
    /// show the settled document asks this, and repaints until it is.
    pub fn settled(&self) -> bool {
        self.settled
    }

    /// The document as it saves.
    pub fn save(&self) -> arrix_doc::MemorySource {
        self._session.save()
    }

    /// Whether the latest generation's evaluation has finished.
    /// The feature tree: every part of the document as the replica holds
    /// it, in id order.
    pub fn tree(&self) -> TreeView {
        TreeView {
            parts: self
                .replica
                .document()
                .parts()
                .values()
                .map(|part| tree_part(&self.replica, part, self.view.feature_selection))
                .collect(),
        }
    }

    pub fn finished(&self) -> bool {
        self.finished_at
            .is_some_and(|(g, _)| g == self.replica.generation())
    }

    /// Takes in what the session has said since the last call, without
    /// waiting. Meshes go to the scene through
    /// `renderer`, if there is one: a mesh of a superseded generation is
    /// dropped, never drawn.
    pub fn drain(&mut self, renderer: Option<&Renderer>) {
        while let Ok(event) = self.events.try_recv() {
            self.receive(&event, renderer);
        }
        self.settled = self.compute_settled();
    }

    /// The feature types the session loaded or dropped since the last
    /// call: the app rebuilds its command registry and forms from them.
    pub fn take_type_changes(&mut self) -> TypeChanges {
        std::mem::take(&mut self.type_changes)
    }

    pub(crate) fn receive(&mut self, event: &SessionEvent, renderer: Option<&Renderer>) {
        if let Err(err) = self.replica.receive(event) {
            self.problems.push(err.to_string());
            self.broken = true;
            return;
        }
        match event {
            SessionEvent::Finished { generation, .. } => {
                self.finished_at = Some((*generation, Instant::now()));
            }
            SessionEvent::Meshed {
                generation,
                part,
                slot,
                detail,
                mesh,
            } => {
                if *generation < self.replica.generation() {
                    return;
                }
                if self.mesh.generation != *generation {
                    self.mesh = MeshProgress {
                        generation: *generation,
                        ..MeshProgress::default()
                    };
                }
                let key = BodyKey {
                    part: *part,
                    feature: slot.feature,
                    slot: slot.slot.clone(),
                };
                match detail {
                    MeshDetail::Coarse => self.mesh.coarse += 1,
                    MeshDetail::Refined => {
                        if self.mesh.refined == 0 {
                            let seen = &self.mesh.seen;
                            self.view.scene.retain(|k| seen.contains(k));
                        }
                        self.mesh.refined += 1;
                    }
                }
                match mesh {
                    Ok(mesh) => {
                        self.mesh.seen.insert(key.clone());
                        if let Some(renderer) = renderer {
                            self.view.scene.set_body(renderer.device(), &key, mesh);
                        }
                        self.refresh_view();
                        self.frame_once();
                        if *detail == MeshDetail::Refined {
                            self.fitting = false;
                        }
                    }
                    Err(diagnostic) => self.problems.push(diagnostic.message.clone()),
                }
            }
            SessionEvent::Assembled { .. } => self.refresh_view(),
            SessionEvent::Opened { types, .. } => self.type_changes.added.extend(types.clone()),
            SessionEvent::Registry { added, removed, .. } => {
                self.type_changes.added.extend(added.clone());
                self.type_changes
                    .removed
                    .extend(removed.iter().map(|id| id.as_str().to_owned()));
            }
            SessionEvent::Exported { bytes, .. } => self.exports.heard(bytes),
            SessionEvent::Previewed { .. } | SessionEvent::PreviewMeshed { .. } => {
                self.receive_preview(event, renderer);
            }
            _ => {}
        }
    }

    /// What this tab looks at: the user's choice while the document still
    /// has it, else where the document opens. `None` for an empty one.
    pub fn context(&self) -> Option<Context> {
        let doc = self.replica.document();
        self.chosen
            .filter(|c| c.exists(doc))
            .or_else(|| Context::opening(doc))
    }

    /// Looks at `context` from now on: the instances change, and the
    /// camera is fitted to them.
    pub fn set_context(&mut self, context: Context) {
        self.chosen = Some(context);
        self.framed = false;
        self.fitting = false;
        self.refresh_view();
        self.frame_once();
    }

    /// Makes the scene draw the context: a part's bodies alone at their
    /// origin, or an assembly's components at the poses last heard.
    fn refresh_view(&mut self) {
        let context = self.context();
        let scene = &mut self.view.scene;
        match context {
            Some(Context::Assembly(id)) => {
                let keys: Vec<BodyKey> = scene.keys().cloned().collect();
                let list = context::instances(&self.replica, id, &keys);
                scene.set_instances(Some(list));
            }
            context => {
                scene.set_instances(None);
                let keys: Vec<BodyKey> = scene.keys().cloned().collect();
                for key in keys {
                    let shown = match context {
                        Some(Context::Part(part)) => key.part == part,
                        _ => true,
                    };
                    scene.set_body_visible(&key, shown);
                }
            }
        }
    }

    /// Fits the camera to the bodies, the first time there are any (an
    /// assembly's, until the first refined mesh: see `fitting`).
    fn frame_once(&mut self) {
        if self.framed && !self.fitting {
            return;
        }
        if let Some((centre, radius)) = self.view.scene.bounding_sphere() {
            self.view.camera.frame_bounds(centre, radius);
            self.fitting = matches!(self.context(), Some(Context::Assembly(_)))
                && (self.fitting || self.mesh.refined == 0);
            self.framed = true;
        }
    }

    fn compute_settled(&self) -> bool {
        if self.broken {
            return true;
        }
        let Some((generation, at)) = self.finished_at else {
            return false;
        };
        if generation != self.replica.generation() {
            return false;
        }
        let m = &self.mesh;
        if m.generation == generation && m.coarse > 0 {
            return m.refined >= m.coarse;
        }
        at.elapsed() >= NO_BODIES_AFTER
    }
}
