# ArriX glossary

The words this project uses for its own concepts, one meaning each. A term
here is used as written in code, docs, plans and commits; the words under
_Avoid_ are the synonyms it replaces. Definitions say what a thing **is**;
how it works lives in the design doc named after it. Arris's own terms
(provenance, refusal, body, precision) are Arris's glossary's.

## The document

**Document**:
A recipe: parameters, parts, assemblies and plugin data, everything needed
to regenerate the geometry and nothing that can be regenerated
(`docs/DATA-MODEL.md` §The document).
_Avoid_: model, project, file (the file is the `.arrx`)

**Part**:
A feature history that evaluates to bodies.
_Avoid_: object, solid

**Assembly**:
A set of components placed relative to one another, with mates.

**Component**:
One placed occurrence of a part or an assembly inside an assembly.
_Avoid_: instance (an instance is a pattern's copy), occurrence

**Feature**:
One history entry: a type, its parameters and references, and the slots it
outputs. Core or a plugin's.
_Avoid_: operation (an operation is a kernel call), step

**Slot**:
A named output of a feature: a body, a datum or a sketch. A cut or join
modifies its target's slot instead of making its own.

**Parameter**:
A named, typed document value that expressions can use.
_Avoid_: variable

**Reference**:
Plain data by which one record points at another (a parameter, a feature, a
slot, a face, a sketch entity). It resolves or fails; it never guesses.
_Avoid_: link, pointer

**Persistent name**:
The name of a face, edge or vertex by where it came from: a root and the
chain of features since, built from Arris's provenance, never from
position.
_Avoid_: id, topological id

**Lost reference**:
A reference whose persistent name no longer resolves. Always a diagnostic
with candidates, never a silent rebind.

**Frozen**:
The state of a plugin feature whose plugin is missing, the wrong version or
unable to run here: its stored last result stands in, read-only, and says
why.
_Avoid_: cached, stale (a stale command is another thing)

**Generation**:
The document's version counter, bumped by every applied command.

**Command**:
Plain, versioned data describing one change; the only way the document
changes. Made against a base generation, applied, rebased or rejected as
stale by the authority.
_Avoid_: action, operation, mutation

**Gesture**:
One user intent. One gesture is one command (a `Group` when it takes
several), so it is one undo entry.

**Author**:
Whoever submits commands; undo is per author, by inverse commands.

**Authority**:
The single writer that holds the document and applies or rejects commands
(the session thread locally).

**Session**:
A client's connection to a document: its state (selection, camera, active
sketch, edit mode) belongs to the client, never to the document.

**Web session**:
The session on a browser page: the authority on the page's main thread and
the evaluator alone in a web worker, cancelled through two shared counters
(`docs/CONCURRENCY-WASM.md` §The browser build).
_Avoid_: web worker session

**Preview**:
The document evaluated with a command that is never applied: no
generation, no undo entry.

**Evaluation**:
Replaying the features that changed, off the UI thread, into bodies and
`EvalEvent`s.

## Plugins

**Plugin**:
A package that contributes to ArriX through the public plugin API alone,
first-party ones included.

**Tier**:
How a plugin is hosted. **Tier 0**: a Rust crate compiled in. **Tier 1**: a
WebAssembly component, sandboxed. **Tier 2**: a separate process on stdio,
the user's Python.

**Contribution point**:
A kind of thing a plugin can add: features, document data, commands,
importers and exporters, analyses, declarative UI.

**Manifest**:
`arrix-plugin.toml`: a plugin's id, version, API range, tier, capabilities
and test fixtures.

**Capability**:
A permission a manifest asks for and the host grants.

**Plugin command**:
A plugin's undoable edit: the plugin expands the gesture into one `Group`
of core commands and plugin-data edits, and the core computes its inverse.

**Form**:
The declarative UI derived from a feature type's or command's spec, rendered
by the core.
_Avoid_: dialog, panel (a panel is a shell region)

**Test kit**:
The checks `arrix test` runs on any plugin: determinism, round trip, frozen
open, migration.

## The kernel

**Kernel choke point**:
`arrix-kernel`, the only crate that names an Arris type.
_Avoid_: kernel wrapper, facade

**Call record**:
The value describing one kernel call (operation, operands, parameters,
precision, Arris version), written before it runs, replayable as an Arris
fixture recipe.
_Avoid_: kernel log, trace

**Kernel's sensor**:
ArriX's role as how Arris learns about real use: call records, countable
failure categories, batch evaluation (`SEED.md` §6.6).

**Arris ask**:
A kernel capability ArriX needs and Arris lacks, recorded in Arris's
repository (`A1`, `A2`, …); never worked around here.
_Avoid_: workaround, kernel request

**Body bytes**:
Arris's serialised body, how frozen results and Tier 2 carry geometry.

## The shell

**Shell**:
The app's chrome around the viewport (ribbon, tree, property card, status
bar), configured by the user's files.
_Avoid_: UI framework, layout

**Agentic user**:
The user's own agent, a first-class user beside the user (ADR-0016).

## Process

**Cycle**:
A roadmap section (`C1`, `C2`, …): a goal, in, out and accept, closed by
`/close-cycle` and a `cN` tag.
_Avoid_: sprint, release, phase

**Milestone**:
A cycle's part with its own accept (`M1`, … ; a plan slug like `c3-m2`).

**Accept**:
The executable check that closes a plan, milestone or cycle.
_Avoid_: demo, acceptance criteria (prose)

**North star**:
The demo `SEED.md` §7 names: bracket, vendor servo, Python fasteners,
robotics URDF, opened frozen without either plugin. C3's accept.

**Gate**:
The checks a change passes: the pre-commit hook (fast) and `scripts/gate`
(the slow half), mirrored by CI.
_Avoid_: CI (CI is the backstop that runs the gate)

**Golden**:
A committed text expectation a test compares against (a JSON line, a
`debug_state` dump, a coarse frame), rewritten only with
`UPDATE_SNAPSHOTS=1` after its diff is read.
_Avoid_: snapshot (the file), baseline (the before image)

**Scenario**:
A visual test reaching one app state, or a document under `tests/docs/`
that a test evaluates.

**Probe**:
A first test that establishes what the kernel or a host does on an untried
case before the step that relies on it.
