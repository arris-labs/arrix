//! Plugin commands at Tier 0 (plans/c3-m3 step 5, ADR-0038): the test
//! command `place.block` puts a block part's component on each picked edge
//! of `tests/docs/assembly-exchange`. The tests hold the group it expands
//! to a golden, apply it and undo it to the document's bytes, run the whole
//! path through a session, and refuse what a command may not do: a stale
//! base, a missing capability, a record that is malformed, a plugin that
//! refuses or panics.
//! Regenerate the golden with `UPDATE_SNAPSHOTS=1`, then read the diff.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use arrix_core::{AssemblyId, ComponentId, Id, PersistentName};
use arrix_doc::{
    Command, CommandEnvelope, Document, EdgePick, Evaluator, ExpandContext, ExpandRequest,
    Expansion, Generation, LocalSession, MemorySource, Outcome, Registry, Rejected, Session,
    SessionEvent, apply, open,
};
use arrix_plugin_api::{Context, Diagnostic};
use arrix_plugin_host::{HostError, register_tier0_commands};

#[path = "plugins/place/src/place.rs"]
mod place;
use place::Place;

fn manifest(capabilities: &str) -> String {
    format!(
        "[plugin]\nid = \"place\"\nversion = \"0.1.0\"\napi = \"^0.8\"\ntier = 0\ntitle = \"Place\"\n\
         licence = \"MIT OR Apache-2.0\"\n[capabilities]\n{capabilities}\n"
    )
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn files(name: &str) -> MemorySource {
    let dir = root().join("tests/docs").join(name);
    let mut files = MemorySource::default();
    let mut pending = vec![dir.clone()];
    while let Some(d) = pending.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                let rel = path.strip_prefix(&dir).unwrap().to_str().unwrap();
                files
                    .0
                    .insert(rel.replace('\\', "/"), std::fs::read(&path).unwrap());
            }
        }
    }
    files
}

fn registry() -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0_commands(&mut r, &manifest("document-write = true"), Arc::new(Place)).unwrap();
    r
}

fn document() -> Document {
    open(&files("assembly-exchange")).unwrap()
}

fn id(n: u64) -> Id {
    Id(n)
}

/// The bracket's component, in "Servo on bracket".
fn bracket() -> ComponentId {
    "C4VR764XTFX0Z".parse().unwrap()
}

fn assembly(doc: &Document) -> AssemblyId {
    *doc.assemblies().keys().next().unwrap()
}

/// The bracket plate's top edges, by the keys of its sketch's sides.
fn pick(side: &str) -> EdgePick {
    EdgePick {
        path: vec![bracket()],
        edge: format!("edge:sweep.EBWK5C332393W.end-edge.{side}")
            .parse::<PersistentName>()
            .unwrap(),
    }
}

fn picks(sides: &[&str]) -> BTreeMap<String, Vec<EdgePick>> {
    BTreeMap::from([("edges".to_owned(), sides.iter().map(|s| pick(s)).collect())])
}

const SIDES: [&str; 2] = ["3ZPNYGV5VXAGH", "57Z6PA4YG5FQY"];

fn ids(n: u64) -> Vec<Id> {
    (1..=n).map(|i| id(1000 + i)).collect()
}

/// `command` expanded on the headless path.
struct Run {
    evaluator: Evaluator,
    document: Document,
    evaluation: arrix_doc::Evaluation,
    registry: Registry,
}

fn run_on(document: Document) -> Run {
    let registry = registry();
    let mut evaluator = Evaluator::new(registry.clone());
    let evaluation = evaluator.evaluate(&document);
    Run {
        evaluator,
        document,
        evaluation,
        registry,
    }
}

impl Run {
    fn expand(
        &mut self,
        command: &str,
        context: Context,
        picks: &BTreeMap<String, Vec<EdgePick>>,
        ids: &[Id],
    ) -> Result<Command, Diagnostic> {
        let command = self.registry.command(command).expect("registered").clone();
        let none = BTreeMap::new();
        self.evaluator.expand(
            &self.document,
            &self.evaluation,
            &Expansion {
                command: command.as_ref(),
                context,
                params: &none,
                choices: &none,
                picks,
                ids,
            },
        )
    }

    fn in_assembly(
        &mut self,
        command: &str,
        sides: &[&str],
        n: u64,
    ) -> Result<Command, Diagnostic> {
        let context = Context::Assembly(assembly(&self.document));
        self.expand(command, context, &picks(sides), &ids(n))
    }
}

#[test]
fn a_command_expands_to_one_group_held_to_a_golden() {
    let mut run = run_on(document());
    let group = run.in_assembly("place.block", &SIDES, 5).unwrap();
    let Command::Group { label, commands } = &group else {
        panic!("{group:?}")
    };
    assert_eq!(label, "Place blocks");
    // A material, a part with its marker, the material assigned, two
    // components.
    assert_eq!(commands.len(), 5);

    let got = serde_json::to_string_pretty(&group).unwrap() + "\n";
    let golden = root().join("crates/arrix-plugin-host/tests/golden/commands-place-block.json");
    if std::env::var_os("UPDATE_SNAPSHOTS").is_some() {
        std::fs::write(&golden, &got).unwrap();
    }
    let want = std::fs::read_to_string(&golden).unwrap_or_default();
    assert_eq!(
        got, want,
        "regenerate with UPDATE_SNAPSHOTS=1 and read the diff"
    );

    // Pure: the same arguments and snapshot, the same group.
    assert_eq!(run.in_assembly("place.block", &SIDES, 5).unwrap(), group);
}

#[test]
fn the_group_applies_whole_and_undoes_to_the_documents_bytes() {
    let mut run = run_on(document());
    let group = run.in_assembly("place.block", &SIDES, 5).unwrap();
    let applied = apply(&run.document, &group).unwrap();
    assert_eq!(
        applied.document.parts().len(),
        run.document.parts().len() + 1
    );
    let a = assembly(&run.document);
    assert_eq!(
        applied.document.assemblies()[&a].components.len(),
        run.document.assemblies()[&a].components.len() + 2
    );
    let back = apply(&applied.document, &applied.inverse).unwrap();
    assert_eq!(back.document, run.document);
    assert_eq!(
        arrix_doc::save(&back.document).0,
        arrix_doc::save(&run.document).0
    );
}

#[test]
fn a_part_the_command_made_is_found_through_the_snapshot_and_reused() {
    let mut run = run_on(document());
    let group = run.in_assembly("place.block", &SIDES[..1], 5).unwrap();
    let Command::Group { commands, .. } = &group else {
        unreachable!()
    };
    assert_eq!(commands.len(), 4, "material, part, assignment, component");
    let next = apply(&run.document, &group).unwrap().document;

    let mut run = run_on(next);
    let again = run.in_assembly("place.block", &SIDES[1..], 5).unwrap();
    let Command::Group { commands, .. } = &again else {
        unreachable!()
    };
    assert!(
        matches!(&commands[..], [Command::AddComponent { .. }]),
        "{commands:?}"
    );
}

/// The positions of the components a group adds, in metres.
fn positions(group: &Command) -> Vec<[f64; 3]> {
    let Command::Group { commands, .. } = group else {
        panic!("{group:?}")
    };
    commands
        .iter()
        .filter_map(|c| match c {
            Command::AddComponent { component, .. } => component.pose.as_ref(),
            _ => None,
        })
        .map(|pose| {
            pose.position
                .each_ref()
                .map(|e| e.text().trim_end_matches(" m").parse().unwrap())
        })
        .collect()
}

#[test]
fn a_geometry_is_moved_into_the_frame_of_the_context() {
    // The test command stands a block 5 mm above each edge's midpoint, in
    // the assembly's frame. Moving and turning the bracket moves the
    // midpoint with it, by the component's solved pose.
    let doc = document();
    let a = assembly(&doc);
    let here = positions(
        &run_on(doc.clone())
            .in_assembly("place.block", &SIDES[..1], 5)
            .unwrap(),
    )[0];

    let mut component = doc.assemblies()[&a].components[&bracket()].clone();
    let e = |t: &str| arrix_doc::expr::Expr::parse(t).unwrap();
    component.pose = Some(arrix_doc::Pose {
        position: [e("10 mm"), e("20 mm"), e("30 mm")],
        rpy: [e("0 deg"), e("0 deg"), e("90 deg")],
    });
    let moved = apply(
        &doc,
        &Command::EditComponent {
            assembly: a,
            component: Box::new(component),
        },
    )
    .unwrap()
    .document;
    let there = positions(
        &run_on(moved)
            .in_assembly("place.block", &SIDES[..1], 5)
            .unwrap(),
    )[0];

    // A quarter turn about z, then the translation.
    let (x, y, z) = (here[0], here[1], here[2] - 0.005);
    let want = [-y + 0.010, x + 0.020, z + 0.030 + 0.005];
    for (got, want) in there.iter().zip(want) {
        assert!((got - want).abs() < 1e-9, "{there:?} against {want:?}");
    }
}

/// The session's answer to an expansion: the next `Expanded` event.
fn expanded(events: &arrix_doc::EventStream) -> (Generation, Result<Command, Diagnostic>) {
    loop {
        let event = events
            .recv_timeout(Duration::from_secs(120))
            .expect("the session answers");
        if let SessionEvent::Expanded { generation, group } = event {
            return (generation, group);
        }
    }
}

fn request(doc: &Document, command: &str, base: Generation, n: u64) -> ExpandRequest {
    ExpandRequest {
        command: command.into(),
        context: ExpandContext::Assembly(assembly(doc)),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: picks(&SIDES),
        ids: ids(n),
        base,
    }
}

#[test]
fn a_session_expands_then_the_client_submits_the_group_as_one_undo_entry() {
    let doc = document();
    let headless = run_on(doc.clone())
        .in_assembly("place.block", &SIDES, 5)
        .unwrap();

    let session = LocalSession::open(doc.clone(), registry()).unwrap();
    let events = session.subscribe();
    let author = arrix_doc::AuthorId(id(7));
    let base = Generation(0);
    assert_eq!(
        session.expand(request(&doc, "place.block", base, 5)),
        Ok(Outcome::Expanding)
    );
    let (generation, group) = expanded(&events);
    assert_eq!(generation, base);
    // The path of the session is the headless path's.
    assert_eq!(group.as_ref(), Ok(&headless));

    // Expanding changed nothing: the same base still submits.
    let envelope = CommandEnvelope {
        author,
        base,
        command: group.unwrap(),
    };
    let Ok(Outcome::Applied(change)) = session.submit(envelope) else {
        panic!("the group applies")
    };
    assert_eq!(change.generation, Generation(1));

    // A command asked on the old base is stale now, and says so at once.
    let stale = session.expand(request(&doc, "place.block", base, 5));
    assert!(
        matches!(stale, Err(Rejected::Stale { base: b, .. }) if b == base),
        "{stale:?}"
    );

    // One undo entry takes the whole group back.
    let Ok(Outcome::Applied(back)) = session.undo(author) else {
        panic!("the group undoes")
    };
    assert_eq!(back.generation, Generation(2));
    assert_eq!(session.undo(author), Err(Rejected::NothingToUndo));
}

#[test]
fn a_session_answers_what_the_command_or_the_host_refuses() {
    let doc = document();
    let session = LocalSession::open(doc.clone(), registry()).unwrap();
    let events = session.subscribe();
    let ask = |command: &str, mutate: &dyn Fn(&mut ExpandRequest)| {
        let mut r = request(&doc, command, Generation(0), 5);
        mutate(&mut r);
        assert_eq!(session.expand(r), Ok(Outcome::Expanding), "{command}");
        expanded(&events).1.unwrap_err().code.as_str().to_owned()
    };
    assert_eq!(ask("place.nothing", &|_| {}), "command.unknown");
    assert_eq!(ask("place.refuse", &|_| {}), "place.refused");
    assert_eq!(ask("place.panic", &|_| {}), "plugin.panic");
    assert_eq!(ask("place.rogue", &|_| {}), "command.invalid");
    assert_eq!(
        ask("place.block", &|r| r.inputs =
            BTreeMap::from([("nope".into(), vec![])])),
        "command.unknown-input"
    );
    assert_eq!(ask("place.block", &|r| r.ids = ids(9)), "command.ids");
    assert_eq!(
        ask("place.block", &|r| r.context =
            ExpandContext::Part("0000000000001".parse().unwrap())),
        "command.context"
    );
    // A pick through a component path in a part, and a path that leads
    // nowhere.
    assert_eq!(
        ask("place.block", &|r| {
            r.context = ExpandContext::Part("BC6X598GYWPZN".parse().unwrap());
        }),
        "command.pick"
    );
    assert_eq!(
        ask("place.block", &|r| {
            r.inputs.get_mut("edges").unwrap()[0].path = vec!["4GM777SG0Q2K8".parse().unwrap()];
        }),
        "ref.lost"
    );
    assert_eq!(
        ask("place.block", &|r| r.params =
            BTreeMap::from([("size".into(), "3".into())])),
        "expr.unit-mismatch"
    );
}

#[test]
fn a_malformed_record_refuses_the_whole_group_naming_the_record() {
    let mut run = run_on(document());
    let d = run.in_assembly("place.rogue", &SIDES, 5).unwrap_err();
    assert_eq!(d.code.as_str(), "command.invalid");
    assert!(d.message.contains("add-component"), "{}", d.message);
    assert!(d.message.contains("not one of the ids"), "{}", d.message);
}

#[test]
fn a_command_without_document_write_is_not_registered() {
    let mut r = Registry::with_core_types();
    let err = register_tier0_commands(&mut r, &manifest("document-read = true"), Arc::new(Place))
        .unwrap_err();
    assert!(matches!(err, HostError::CommandCapability { .. }), "{err}");
    assert!(r.command("place.block").is_none(), "none of them");
}

#[test]
fn leaving_a_plugin_out_leaves_its_commands_out() {
    let mut r = registry();
    assert_eq!(r.command_specs().len(), 4);
    assert!(r.remove_plugin(&"place".parse().unwrap()));
    assert!(r.command("place.block").is_none());
}
