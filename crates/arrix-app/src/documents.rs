//! The open documents: each one a [`LocalSession`], the replica of its
//! applied stream, and the scene and camera it is looked at through
//! (docs/UI-RENDERING.md §The shell; the viewport's state is session state,
//! SEED.md §6.1 rule 4).

use std::collections::BTreeSet;
use std::time::Duration;

use arrix_core::MeshDetail;
use arrix_doc::{
    DocumentSource, EventStream, Generation, LocalSession, Registry, Replica, Session, SessionEvent,
};
use arrix_viewport::{BodyKey, GpuMesh, OrbitCamera, Renderer, Scene};
use web_time::Instant;

/// How long after an evaluation finished with no mesh heard it is taken to
/// have no body to mesh. Meshing starts the moment `Finished` is
/// published, so this only has to outlast a hop between threads and a busy
/// machine; the wall clock, since egui's can run ahead of it in a test.
const NO_BODIES_AFTER: Duration = Duration::from_secs(1);

/// What is drawn and from where: the bodies, and the camera on them.
#[derive(Default)]
pub struct ViewState {
    pub scene: Scene<GpuMesh>,
    pub camera: OrbitCamera,
}

/// A card the user toggled: `None` leaves the setting in charge.
#[derive(Clone, Copy, Default)]
pub struct CardToggles {
    pub tree: Option<bool>,
    pub properties: Option<bool>,
}

/// One open document.
pub struct OpenDocument {
    pub title: String,
    /// Where it was opened from; the tab's tooltip.
    pub path: String,
    /// The cards this document's user toggled: they win over `ui.panel.*`
    /// (ADR-0017), and stay with the tab.
    pub cards: CardToggles,
    events: EventStream,
    _session: LocalSession,
    pub replica: Replica,
    pub view: ViewState,
    /// Whether the camera has been fitted to the bodies: it is once, on
    /// the first mesh, and never again, so a later edit does not take the
    /// view away from the user.
    framed: bool,
    mesh: MeshProgress,
    /// When `Finished` of the current generation was heard.
    finished_at: Option<(Generation, Instant)>,
    settled: bool,
    /// The stream could not be followed: nothing more will be heard.
    broken: bool,
    /// What went wrong that is not the document's own: a body that did not
    /// mesh, a stream the replica could not follow.
    pub problems: Vec<String>,
}

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
            framed: false,
            mesh: MeshProgress::default(),
            finished_at: None,
            settled: false,
            broken: false,
            problems: Vec::new(),
        })
    }

    /// Whether everything the document's latest generation will say has
    /// been heard: evaluated, and every body meshed refined. Meshes and
    /// events arrive from the session's threads, so a frame that wants to
    /// show the settled document asks this, and repaints until it is.
    pub fn settled(&self) -> bool {
        self.settled
    }

    /// Whether the latest generation's evaluation has finished.
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

    fn receive(&mut self, event: &SessionEvent, renderer: Option<&Renderer>) {
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
                        self.frame_once();
                    }
                    Err(diagnostic) => self.problems.push(diagnostic.message.clone()),
                }
            }
            _ => {}
        }
    }

    /// Fits the camera to the bodies, the first time there are any.
    fn frame_once(&mut self) {
        if self.framed {
            return;
        }
        if let Some((centre, radius)) = self.view.scene.bounding_sphere() {
            self.view.camera.frame_bounds(centre, radius);
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
