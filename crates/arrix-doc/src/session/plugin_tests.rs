//! Plugins loaded into a running session (ADR-0024), through a Tier 0
//! double of the host's loader: `test.block` as a plugin `test` of version
//! `0.1.0` contributes it, whose code is the skew an edit changes.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arrix_core::{Diagnostic, Id, ParamId, PluginId, Ref, Severity, SlotName, SweepPartName};
use arrix_plugin_api::{FeatureTypeSpec, Kernel};

use super::tests::until_finished;
use super::*;
use crate::command::Command;
use crate::document::FeatureTypeId;
use crate::eval::tests::{Block, doc, face, fid, plane_slot, record};
use crate::eval::{Evaluator, FrozenReason, SlotView};
use crate::expr::Expr;
use crate::registry::{FeatureArgs, FeatureType, Registry, TypeOutput};

const ME: AuthorId = AuthorId(Id(1));
const BLOCK: u64 = 2;
const ABOVE: u64 = 3;
const WIDTH: ParamId = ParamId(Id(10));

struct Plugin(Block);

impl FeatureType for Plugin {
    fn spec(&self) -> &FeatureTypeSpec {
        self.0.spec()
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic> {
        self.0.evaluate(kernel, args)
    }

    fn plugin_version(&self) -> Option<&str> {
        Some("0.1.0")
    }
}

/// Loads the plugin directory `test`, whose code is `skew`.
struct Loader(Arc<AtomicU64>);

impl Loader {
    fn register(&self, registry: &mut Registry) -> Result<Vec<FeatureTypeId>, Diagnostic> {
        let ty = Plugin(Block::skewed(self.0.clone()));
        let id = registry.register(Arc::new(ty)).expect("a well-formed type");
        Ok(vec![id])
    }
}

fn refusal(code: &str, message: &str) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).unwrap()
}

impl PluginLoader for Loader {
    fn load(
        &mut self,
        registry: &mut Registry,
        dir: &Path,
    ) -> Result<Vec<FeatureTypeId>, Diagnostic> {
        if dir != Path::new("test") {
            return Err(refusal("plugin.load", "no such plugin directory"));
        }
        self.register(registry)
    }

    fn reload(
        &mut self,
        registry: &mut Registry,
        plugin: &PluginId,
    ) -> Result<Vec<FeatureTypeId>, Diagnostic> {
        if !registry.remove_plugin(plugin) {
            return Err(refusal("plugin.load", "that plugin is not loaded"));
        }
        self.register(registry)
    }
}

fn test_plugin() -> PluginId {
    PluginId::new("test").unwrap()
}

fn recipe() -> Document {
    let top = Ref::Topo(face(BLOCK, SweepPartName::EndCap));
    doc(
        &[(10, "w", "20 mm")],
        vec![
            record(1, "core.datum-plane", &[("offset", "10 mm")], &[]),
            record(
                BLOCK,
                "test.block",
                &[("width", "w")],
                &[("plane", plane_slot(1))],
            ),
            record(
                ABOVE,
                "core.datum-plane",
                &[("offset", "2 mm")],
                &[("plane", top)],
            ),
        ],
    )
}

/// `recipe()` with the block's result stored, as a save writes it.
fn stored() -> Document {
    let mut d = recipe();
    let mut registry = Registry::with_core_types();
    registry.register(Arc::new(Plugin(Block::new()))).unwrap();
    let mut ev = Evaluator::new(registry);
    ev.evaluate(&d);
    let frozen = ev
        .frozen()
        .get(&fid(BLOCK))
        .expect("a live plugin feature")
        .clone();
    assert!(d.freeze(fid(BLOCK), &frozen));
    d
}

fn open(doc: Document, skew: &Arc<AtomicU64>) -> LocalSession {
    let loader = Box::new(Loader(skew.clone()));
    LocalSession::open_with(doc, Registry::with_core_types(), Some(loader)).unwrap()
}

fn volume(replica: &Replica) -> f64 {
    match replica.outcome(fid(BLOCK)) {
        Some((_, FeatureOutcome::Ok { slots, .. })) => {
            match slots.get(&SlotName::new("body").unwrap()) {
                Some(SlotView::Body(m)) => m.volume,
                other => panic!("{other:?}"),
            }
        }
        other => panic!("{other:?}"),
    }
}

fn frozen(replica: &Replica, feature: u64) -> Option<FrozenReason> {
    match replica.outcome(fid(feature)) {
        Some((_, FeatureOutcome::Ok { frozen, .. })) => frozen.map(|f| f.reason),
        other => panic!("{other:?}"),
    }
}

fn loaded(outcome: Result<Outcome, Rejected>) -> Generation {
    match outcome {
        Ok(Outcome::Loaded { generation }) => generation,
        other => panic!("{other:?}"),
    }
}

fn registry_event(seen: &[SessionEvent]) -> (Generation, Vec<String>, Vec<FeatureTypeId>) {
    let found: Vec<_> = seen
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Registry {
                generation,
                added,
                removed,
            } => Some((
                *generation,
                added.iter().map(|s| s.id.clone()).collect(),
                removed.clone(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(found.len(), 1, "{seen:?}");
    found.into_iter().next().unwrap()
}

#[test]
fn a_load_makes_a_generation_and_a_frozen_feature_evaluates_live_under_it() {
    let skew = Arc::new(AtomicU64::new(0));
    let s = open(stored(), &skew);
    let (events, mut replica) = (s.subscribe(), Replica::default());
    until_finished(&events, &mut replica, 0);
    assert_eq!(frozen(&replica, BLOCK), Some(FrozenReason::Missing));
    let before = volume(&replica);

    let outcome = s.load_plugin("test");
    crate::session::crosses(&outcome.clone().unwrap(), "outcome");
    let generation = loaded(outcome);
    assert_eq!(generation, Generation(1));
    let seen = until_finished(&events, &mut replica, 1);
    let (made, added, removed) = registry_event(&seen);
    assert_eq!(
        (made, added, removed),
        (generation, vec!["test.block".to_owned()], vec![])
    );
    for e in &seen {
        crate::session::crosses(e, "event");
    }
    assert_eq!(replica.generation(), generation);
    assert_eq!(frozen(&replica, BLOCK), None);
    assert!((volume(&replica) - before).abs() < 1e-12);

    // Whoever subscribes later hears the type in `Opened`.
    let later = s.subscribe();
    match next_event(&later) {
        SessionEvent::Opened {
            generation: g,
            types,
            ..
        } => {
            assert_eq!(g, generation);
            assert_eq!(
                types.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
                ["test.block"]
            );
        }
        other => panic!("{other:?}"),
    }
}

fn next_event(events: &EventStream) -> SessionEvent {
    events
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
}

#[test]
fn a_reload_of_a_changed_type_re_evaluates_despite_an_equal_version() {
    let skew = Arc::new(AtomicU64::new(0));
    let s = open(recipe(), &skew);
    let (events, mut replica) = (s.subscribe(), Replica::default());
    until_finished(&events, &mut replica, 0);
    assert!(matches!(
        replica.outcome(fid(BLOCK)),
        Some((_, FeatureOutcome::Failed { .. }))
    ));

    assert_eq!(loaded(s.load_plugin("test")), Generation(1));
    until_finished(&events, &mut replica, 1);
    let first = volume(&replica);

    // The plugin's source is edited: the same version, a taller block.
    skew.store(0.001f64.to_bits(), Ordering::Relaxed);
    let outcome = s.reload_plugin(test_plugin());
    crate::session::crosses(
        &Request::ReloadPlugin {
            plugin: test_plugin(),
        },
        "request",
    );
    assert_eq!(loaded(outcome), Generation(2));
    let seen = until_finished(&events, &mut replica, 2);
    let (made, added, _) = registry_event(&seen);
    assert_eq!(
        (made, added),
        (Generation(2), vec!["test.block".to_owned()])
    );
    let again = volume(&replica);
    assert!((again / first - 1.2).abs() < 1e-9, "{first} then {again}");
}

#[test]
fn no_loader_and_a_failed_load_are_refused_without_a_generation() {
    let none = LocalSession::open(recipe(), Registry::with_core_types()).unwrap();
    assert_eq!(none.load_plugin("test"), Err(Rejected::NoPluginLoader));
    assert_eq!(
        none.reload_plugin(test_plugin()),
        Err(Rejected::NoPluginLoader)
    );
    crate::session::crosses(&Rejected::NoPluginLoader, "rejected");

    let skew = Arc::new(AtomicU64::new(0));
    let s = open(recipe(), &skew);
    let Err(rejected @ Rejected::PluginLoad(_)) = s.load_plugin("nowhere") else {
        panic!("a bad directory loads");
    };
    crate::session::crosses(&rejected, "rejected");
    assert!(matches!(
        s.reload_plugin(test_plugin()),
        Err(Rejected::PluginLoad(_))
    ));
    // No generation was made: a command against generation 0 applies as 1.
    let set = Command::SetParam {
        param: WIDTH,
        expr: Expr::parse("30 mm").unwrap(),
    };
    let envelope = CommandEnvelope {
        author: ME,
        base: Generation(0),
        command: set,
    };
    match s.submit(envelope).unwrap() {
        Outcome::Applied(change) => assert_eq!(change.generation, Generation(1)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn undo_and_redo_across_a_registry_change_are_unaffected() {
    let skew = Arc::new(AtomicU64::new(0));
    let s = open(recipe(), &skew);
    let (events, mut replica) = (s.subscribe(), Replica::default());
    until_finished(&events, &mut replica, 0);

    let width = |e: &str| Command::SetParam {
        param: WIDTH,
        expr: Expr::parse(e).unwrap(),
    };
    let submit = |replica: &mut Replica, base: u64, e: &str| {
        let envelope = CommandEnvelope {
            author: ME,
            base: Generation(base),
            command: width(e),
        };
        assert!(matches!(s.submit(envelope).unwrap(), Outcome::Applied(_)));
        until_applied(&events, replica);
    };
    submit(&mut replica, 0, "30 mm");
    assert_eq!(loaded(s.load_plugin("test")), Generation(2));
    // A command based on the generation before the load is not stale: the
    // load stamped no node.
    submit(&mut replica, 1, "25 mm");
    assert_eq!(replica.generation(), Generation(3));
    for (undo, want) in [(true, "30 mm"), (true, "20 mm"), (false, "30 mm")] {
        let outcome = if undo { s.undo(ME) } else { s.redo(ME) }.unwrap();
        assert!(matches!(outcome, Outcome::Applied(_)), "{outcome:?}");
        until_applied(&events, &mut replica);
        assert_eq!(
            replica.document().params[&WIDTH].expr,
            Expr::parse(want).unwrap()
        );
    }
}

/// Follows `events` up to and including the next `Applied`.
fn until_applied(events: &EventStream, replica: &mut Replica) {
    loop {
        let e = next_event(events);
        replica.receive(&e).unwrap();
        if matches!(e, SessionEvent::Applied { .. }) {
            return;
        }
    }
}
