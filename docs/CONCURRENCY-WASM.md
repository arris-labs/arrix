# Concurrency and wasm

Where work runs, how an edit supersedes an evaluation, what determinism
promises, how many documents are evaluated at once, what the browser build
is, and the performance budgets. Built, as of C3: the evaluator with its cache and
cancellation, `LocalSession`'s session thread and evaluator worker with the
clients' replicas, `arrix eval` and its batch mode, the budgets measured,
and Tier 1 fuel and epoch cancellation.
The browser is C4, designed in ADR-0042 (§The browser build).

## Roles and threads

Three roles, named after the server-and-thin-client split they must not
preclude (`SEED.md` §6.1). Locally all three are in one process.

| Role | Thread (native) | Does |
|---|---|---|
| **Client** | the UI thread (or the CLI's main thread) | input, egui, the viewport, the sketch solver during sketch edits; keeps a **replica** of the document; submits commands |
| **Authority** | the session thread | holds the document; applies or rejects each command; publishes the applied stream; hands snapshots to the evaluator |
| **Evaluator** | one worker thread (C1) | evaluates snapshots through `arrix-kernel`, tessellates, publishes `EvalEvent`s |

- **Replicas.** A client does not read the authority's document. It
  applies the same applied-command stream to its own copy, which is cheap
  (applying is pure and fast) and makes the protocol boundary real on every
  run. A debug build checks the replica's hash against the authority's at
  each generation. As built: `LocalSession` spawns both threads through
  the executor (`crates/arrix-doc/src/executor`, the one place a thread
  is spawned; on wasm32 spawning fails with an error until C4). A
  `Replica` follows `applied` events and refuses a gap in generations;
  each `applied` event carries the document's BLAKE3 hash over its saved
  files in a debug build, and the replica refuses one it differs from.
- **Snapshots** are immutable and cheap to clone: the document is a tree
  of `Arc`ed parts and records, so a snapshot shares everything a command
  did not touch. As built: a snapshot is a whole clone of the document
  in an `Arc`, until a budget says sharing pays.
- Tier 2 plugins are separate processes (ADR-0021); the evaluator waits
  on them like on any other feature, serving their kernel calls on its own
  thread while it does. As built: one reader thread per plugin process
  turns its stdout into lines, so a wait has a timeout
  (`crates/arrix-plugin-host/src/tier2`, native only).

## The evaluator

- **Newest generation wins.** When a command is applied during an
  evaluation, the evaluator finishes or cancels its current feature, then
  continues from the new snapshot. Outputs whose input hash did not change
  are reused from the cache, so continuing is cheap.
- **Cancellation.** A superseded evaluation stops inside its kernel call,
  not after it. The evaluator's `Kernel` owns an interrupt flag (an
  `Arc<AtomicBool>`, no clock, so the same on wasm) and passes its poll to
  every Arris call as a `Control` (Arris ask A3); a raised flag stops the
  running call at its next loop boundary with the model rolled back, and
  fails every later call until it is lowered. No thread is ever killed.
  As built: the session thread raises the flag, then queues each commit's
  snapshot (in that order, so a worker that lowers the flag on taking a job
  never lowers a raise whose job is still to come); the worker lowers it on
  taking a job. A feature stopped by it (`kernel.interrupted`) publishes no
  event and caches nothing, and the evaluation returns at once; the worker
  starts over on the newest snapshot, with the cache warm for the features
  the edit kept. A mesh stopped the same way publishes no event. Only a
  commit raises the flag: a preview never stops a commit, and a preview a
  commit or a newer preview supersedes stops at its next feature or mesh.
  A spent step budget (`kernel.interrupted.budget`) is a failure like any
  other, not a stop. A Tier 1 component running its own code between
  kernel calls is stopped by the same flag (ADR-0035 §7): the evaluator
  hands it to the type in `FeatureArgs`, never to the plugin, one thread
  ticks the wasmtime engine's epoch every 5 ms, and each store's deadline
  callback traps on a tick that finds the flag raised, which ends the
  feature as `kernel.interrupted`, a stop and not `plugin.fuel`
  (`crates/arrix-plugin-host/tests/tier1_cancel.rs`, held to the 50 ms
  budget).
- **Events carry their generation.** A client drops an event older than
  the latest generation it has drawn for that feature.
  As built: the worker publishes each feature's event as it is made and
  checks for a newer snapshot after each one; it starts over on the
  newest queued, and an evaluation it left has no `finished` event. A
  `Replica` keeps each feature's newest outcome and, at a `finished`,
  drops outcomes older than it. A subscription opens with the document
  as it is, and hears the evaluations made after it.
- **The cache** maps input hash to outputs (`docs/DATA-MODEL.md`
  §Evaluation), bounded in bytes, least recently used first out, shared
  across generations. Outputs are Arris bodies in one long-lived model per
  evaluator; after each completed evaluation, `Model::retain` drops the
  bodies no cache entry holds.
  Built: the cache and the long-lived kernel (`arrix_doc::Evaluator`),
  bounded by entry count (`DEFAULT_CAPACITY`, 256) until a measurement
  asks for a bound in bytes; an entry the latest evaluation used is never
  evicted. The kernel's call records outlive the bodies they made, so a
  later failure's operands still name them.
- **Order.** C1 evaluates features one at a time in topological order.
  Parallel evaluation of independent branches (parts, patterns) is
  clone-evaluate-import across Arris models, through body bytes (ask A2,
  available) once a measurement says it pays; it is not built before.
- **Tessellation** runs on the evaluator once a part is evaluated, on its
  *final* bodies only (each slot's last version that nothing consumed):
  every body coarse first, then every body refined, each a `Meshed` event
  of its generation. A newer snapshot stops it between meshes, and a
  version the evaluator's cache holds keeps its meshes.
- **A registry change is a generation** (ADR-0024). Loading or reloading a
  plugin bumps the generation without stamping a node; the session hands
  the worker the new registry ahead of that generation's commit, and the
  evaluator swaps it between evaluations, never within one. A reload drops
  the cache entries of its types first, since an equal version no longer
  promises equal code; other types' entries stay. Undo and redo stacks
  are untouched.
- **Previews come after commits** (ADR-0019). A preview waits until the
  newest commit is evaluated and meshed, and never stops it; a running
  preview stops between features or meshes for a commit, a newer preview
  or a clear, and a commit drops a waiting one, which was taken on an older
  document. It evaluates on the same evaluator and cache, so the command
  applied after it is a cache hit, and only the bodies it changed against
  the newest commit's are meshed. Its plugin results are not kept for a
  save.

## Determinism

Evaluating the same snapshot gives byte-identical bodies, names, meshes
and saved files, on any machine, native or wasm (`SEED.md` §6.3, §6.6).
Replay, memoisation, frozen results, fixture records and golden tests all
stand on it.

- No clock, randomness or environment reads in `arrix-core`,
  `arrix-kernel`, `arrix-sketch` or `arrix-doc` evaluation paths. Ids are
  minted by the client when it builds a command, never during evaluation.
- No `HashMap` or `HashSet` iteration reaches an output; ordered maps or
  sorted collections only.
- Floating point: no fast-math, no platform intrinsics whose result
  differs by target. Arris's `parallel` feature is byte-identical by its
  own contract; the tests run both ways.
- A plugin's `evaluate` has the same obligation, checked by the test kit
  (`docs/PLUGINS.md` §The test kit), and its result does not depend on its
  tier: `f64::sin` and its kin are the system's libm natively and Rust's
  on wasm32, a last-place apart, so a plugin's transcendental math is
  `arrix_plugin_api::math` (`libm`, compiled into the plugin), the same
  code everywhere (ADR-0036). `sqrt` and the arithmetic are exact under
  IEEE 754 and need nothing.
- **Measured on wasm32** (plans/c4 step 2): every document under
  `tests/docs`, evaluated under Node, prints `arrix eval`'s native line to
  the bit, and so does each point of a sweep of the bracket's width;
  `arrix-plugin-host`'s `wasm_determinism` holds it, with no tolerance.
  Under arris 0.8 each document with a gear differed in the last place:
  its extrude's three-point arcs took their sweeps from `f64::atan2`,
  glibc's natively and Rust's `libm` on wasm32. Arris 0.9 runs its
  transcendentals on one `libm` on every target (`arris_math::real`).
- An export is the same: a pure function of the snapshot and its options,
  run twice by the test kit and identical to the byte at every tier
  (ADR-0037; `robotics.urdf` at Tier 0 and as a component, `--test c3_m2`).
- **Time budgets are not determinism.** A wall-clock timeout makes a
  result depend on the machine. A Tier 1 component's fuel (ADR-0035 §5)
  is the same kind of budget for its own code, counted in wasm
  instructions and the same on every run, beside Arris's for the kernel's.
  Batch evaluation uses Arris's step budget
  (ask A3) where it exists; a wall-clock timeout is a last resort, and a
  result it cut short is reported as `timeout`, never as a kernel failure.

## Batch evaluation

Batch evaluation is a first-class API, not a test helper (`SEED.md` §6.6):
`arrix eval` in C1, the same through the Python API as `arrix.eval(paths,
…)`, which returns the parsed lines and checks their `format`. Dataset
replay, agent users and continuity sweeps are built on it.

```
arrix eval <docs or dirs>… [--sweep <feature>.<param>=<from>..<to>:<n>]
           [--without-plugin <id>]… [--save-fixtures <dir>]
           [--budget <kernel steps>] [--jobs <n>] [--summary] [--timings]
```

- A document is a directory or an `.arrx` file (`arrix eval <path>` reads
  either, telling them apart on disk).
- One JSON line per document (per sweep point), deterministic unless
  `--timings` is given:

  ```json
  {"format":1,"doc":"bracket.arrx","sweep":null,"status":"failed",
   "params":[{"id":"…","name":"w","value":0.012,"diagnostic":null}],
   "features":[{"id":"…","type":"core.fillet","status":"failed",
                "category":"kernel.degenerate.blend-too-large",
                "diagnostic":{"code":"…","message":"…","refs":["…"]}}],
   "bodies":[{"part":"…","feature":"…","slot":"body","volume":1.2e-5,"area":4.1e-3,
              "faces":14,"edges":36,"vertices":24}],
   "assemblies":[{"id":"…","name":"…","components":[{"path":["…"],"pose":{"origin":[…],
                  "x_axis":[…],"z_axis":[…]}}],"joints":{"<id>":{"status":"ok","value":1.57,…}}}]}
  ```

  `assemblies` (in SI, a pose a frame) is absent when the document has none,
  so a line of a document without them is unchanged (ADR-0026, additive); a
  joint that failed (`ref.lost` and the rest) makes the line `failed`.
  Likewise `materials` (`id`, `name`, `density` in kg/m³ or null,
  `appearance` as r, g, b) and `mass` (ADR-0031: `bodies`, `parts`,
  `assemblies`, each volume, an optional `mass` `{mass, centroid,
  inertia}` in SI about the centre of mass, `partial` and the
  `mass.no-material` diagnostics) are absent when there is no material or
  body, and are additive.

- `--summary` ends with the failure histogram: one row per category with
  its count, the documents that hit it and the plugins that called it.
- `--save-fixtures` writes the failing call record of every kernel failure
  as an Arris fixture recipe (`docs/ARCHITECTURE.md` §The kernel choke
  point).
- `--jobs` evaluates documents in parallel, each with its own evaluator
  and Arris model: parallelism across documents, never inside one.
- Exit status: 0 when every document evaluated without a failed
  parameter or feature (suppressed and rolled-back ones are not
  failures), 1 otherwise, 2 on usage or I/O errors.
- **As built.** `arrix eval <paths>…`: a path is a document directory, an
  `.arrx`, or a directory searched for those (sorted by path); one line per
  document in the order given, whatever `--jobs` is. The line type,
  `EvalLine`, and `arrix_doc::run` and `sweep` live in `arrix-doc`, so
  tests and the CLI share one code path; the core feature types and the
  CLI's compiled-in plugins are registered (`crates/arrix-cli/src/plugins.rs`).
  An empty document prints
  `{"doc":"<dir>","sweep":null,"status":"ok","params":[],"features":[],"bodies":[]}`.
  A path that cannot be read is said on stderr and the rest still print.
  - `--sweep <target>=<from>..<to>:<n>`: `target` is a document parameter
    or `<feature>.<field>`; the points are edits on one evaluator, so the
    cache carries over; the line's `sweep` is `{"target","index","value"}`.
  - `--summary` ends stdout with `{"summary":[{"category","count","docs",
    "plugins"}]}`, rows in category order: a failed feature counts under its
    category and its type's plugin (`core` for the core types), a failed
    parameter under its diagnostic's code and no plugin. A sweep counts each
    point's failure.
  - Every line carries `"format"`, its version (ADR-0026), and `arrix eval
    --schema` prints the schema of the line, the `--timings` field and the
    `--summary` line, held by `tests/golden/eval.schema.json`.
  - `--timings` appends `"timings":{"wall_ms","lines"}` to each line: the
    wall time of the whole document (every sweep point), the one field that
    is not deterministic.
  - `--jobs <n>` evaluates documents on `n` threads of the CLI's own, each
    on its own evaluator and model; results are emitted in the documents'
    order, so stdout, stderr and the exit status are `--jobs 1`'s.
  - `--budget <steps>` is `Kernel::set_budget`: a cap on each kernel call,
    not on the document, so a result depends on the document and the number
    alone. A call it stops fails its feature as
    `kernel.interrupted.budget`; features reading it have no input, as for
    any failure. Its record replays to success, so `--save-fixtures` files
    nothing for it.
  - An unreadable document exits 2 with the reason on stderr (a missing
    directory, a `document.json` that is absent or not JSON, a schema newer
    than the build's, a malformed document); over several documents the exit
    status is the worst of them.

## The browser build

The same Rust codebase runs in the browser (`SEED.md` §1). C1 keeps it
compiling; C4 makes it run.

- **C1**: every crate but `arrix-cli` and the native plugin tiers builds
  for `wasm32-unknown-unknown` in the gate and CI. Nothing runs in
  a browser yet, and nothing is built that would make the browser path a
  special case (the wasm lint, `docs/ARCHITECTURE.md` §Gates).
- **C4** (ADR-0042): `arrix-app` as an eframe web app. The client and the
  authority run on the main thread (the app reads the session
  synchronously, and the authority makes no kernel call); the evaluator
  runs alone in one **web worker**. They exchange the same plain data the
  session thread and the evaluator thread exchange natively. The kernel
  never runs on the main thread, in the browser too.
- **Cancellation in the browser**: a `SharedArrayBuffer` of two counters,
  on a cross-origin-isolated page (COOP `same-origin`, COEP
  `require-corp`). **commits** is bumped by the authority after it applies
  a commit and before it posts the job, which carries the count; the
  worker's Arris poll (`Control::poll`, through `Atomics.load`) answers
  `commits > taken`. **supersedes** is read at feature and mesh
  boundaries, so a preview never stops a commit. Without isolation the
  poll answers false: a superseded evaluation runs to its end, previews
  give way at job boundaries, and the status line says so. Terminating the
  worker and threads on nightly are rejected.
- **The probe** (plans/c4 step 3, `arrix-kernel`'s `tests/web_probe.rs`
  through the browser harness, `arrix-web-test`): served with COOP and
  COEP, headless Chromium's page is cross-origin isolated, and a module
  worker running the same wasm module shares the page's
  `SharedArrayBuffer`; served without them, the page has none. A bump from
  the page stops the worker's Arris call as `Stop::Poll`, heard at the
  first step after it. One `Atomics.load` through `js-sys` costs about
  30 ns and the median Arris step a few µs (`cut_many` of a box less 64
  cylinders: 908 steps in 0.8 s, dev profile), so the poll reads the
  counter at every step. A stop's latency is Arris's step size and the
  rollback: it waits for the next step, and that `cut_many` has a 0.2 s
  stretch without one (`docs/BACKLOG.md`), then unwinds in about 2 ms.
  Step 8 gates the latency on the bracket.
- **The web session** (plans/c4 step 7, `arrix-doc`'s `session/web.rs`):
  `WebSession` runs the authority's core on the main thread and
  `run_evaluator` the native `EvalWorker` in a module worker the executor
  starts from a script the embedder serves. Jobs go as
  `[commits, supersedes, job]`, the job in its wire form; answers come
  back as text. The worker hears messages only between passes, so its
  queue also reads the counters: a commit evaluation and its meshes stop
  for a commit posted and not yet received, a preview for any commit,
  preview or clear, and a pass waits for those to arrive. It takes
  messages into its queue and runs the pass on a zero timer, so the jobs
  that came during a pass are all received before the next. The
  interrupt's source is `commits`, against the count the newest job
  received carries. A client reads a generation's eval line from its
  events (`Evaluation::heard`), wherever the evaluator runs.
  `arrix-plugin-host`'s `tests/web_session.rs` opens the bracket from
  `.arrx` bytes in headless Chromium, isolated and not, and hears an edit
  evaluate and mesh.
- **Files** come from the browser's file APIs as bytes; there is no
  filesystem. Importers already take bytes (`docs/PLUGINS.md` §4).
- **Plugins**: Tier 0 only, and no `PluginLoader`, so `LoadPlugin` and
  `ReloadPlugin` are `Rejected::NoPluginLoader` (ADR-0024). A document using a Tier 1 or Tier 2 plugin
  opens with those features frozen (`docs/DATA-MODEL.md` §Frozen results).
- **Picking** uses the same asynchronous readback as native.

## Budgets

Targets, measured by the C1 benchmark on the reference machine. A target
becomes a gate in the plan that first meets it, with the measured number
written here; a missed one stays a target and a backlog line.

| What | Target |
|---|---|
| A UI frame with the C1 acceptance part on screen | ≤ 16 ms; measured 0.09 ms median |
| Applying a command (authority and replica) | ≤ 1 ms; gated, measured 0.1 ms median |
| One sketch-drag solve on a 200-entity sketch | ≤ 4 ms, on the UI thread; gated, measured 0.38 ms median |
| A UI frame dragging a joint, the accept's assembly on screen | ≤ 16 ms, on the UI thread; gated, measured 0.22 ms median |
| One inference verdict (`check_candidate`) on a drawn part's sketch | inside the frame, on the UI thread; measured 0.1–0.6 ms median |
| First coarse mesh after a parameter edit of the acceptance part | ≤ 150 ms; missed, measured 324 ms median |
| Cancellation latency | ≤ 50 ms; gated, measured 10 ms median natively and 9 ms in the browser (isolated page) |
| Save and load of the acceptance document | ≤ 100 ms each; gated, measured under 0.5 ms |
| `arrix eval` throughput over the scenario documents | reported, no target yet; 6.9 documents a second, 21 with `--jobs 4` |

The UI frame was measured with the slice (its one pocketed body, refined,
1,788 triangles) and its tree card on screen at 1440 × 900: the app's frame as the CPU does it, which is the view built,
egui's layout and paint, the viewport's draw preparation and the ViewCube,
plus egui's tessellation, and not the GPU (lavapipe says nothing about
one). Median 0.09 ms, p95 0.10 ms, at rest and with a middle drag
orbiting the camera (which re-asks for the hover pick each frame), in the
dev profile (optimised, with debug info) on a Ryzen 9 7950X
(`cargo test -p arrix-app --test visual -- ui_frame_budget --nocapture`,
reported, not asserted). The scene's cost is the GPU's: a body is one draw
and its meshes are uploaded once, so the UI thread's frame does not grow
with the triangle count; the GPU side is measured when a reference GPU is
in CI.

The sketch drag is measured on a 199-entity sketch (a 9 × 10 grid of
10 mm cells, every line horizontal or vertical, one connected subsystem of
218 variables, its far corner dragged): 0.38 ms median per frame in a
release build on a Ryzen 9 7950X, 0.7 ms in the dev profile, and the test
asserts the 4 ms median in both
(`crates/arrix-sketch/tests/solver_benchmarks.rs`). It was 34 ms. A drag
frame reports no rank (`SolverOptions::compute_rank`), whose dense analysis
alone cost 5.6 ms per solve; and the tangent step eliminates the rows of
two equal and opposite entries (a coincidence, a horizontal, a vertical) by
merging their variables, and the rows of one entry by holding theirs,
before the dense null-space work, which then grows with the sketch's
freedom and not its size (`solver/tangent.rs`; exact in arithmetic, the
positions within 1e-8 m of the unreduced solver's).

A sketch of nonlinear rows is still over: a zigzag of 200 lines, each with
a length and perpendicular to the last, drags at 300 ms median per frame
(7.2 s before; reported, not gated), a dense 400-variable basis of about
30 ms per follow iteration and about 13 iterations a frame on a chain that
cannot reach the cursor (`docs/BACKLOG.md`).

The joint-drag frame is the app's frame (as the UI frame above, without
the GPU) while a left drag turns the hinged servo of `tests/docs/assembly`
with a second servo grounded beside it, the pointer walked in a circle:
forward kinematics over the last solve's frames and the instances moved,
nothing uploaded. Median 0.22 ms, p95 0.28 ms, in the dev profile on a
Ryzen 9 7950X; the test asserts the 16 ms median
(`cargo test -p arrix-app --test visual -- joint_drag_frame_budget
--nocapture`).

An inference verdict is what the draw preview pays when its candidate set
changes (`Sketch::check_candidate`, one per candidate): on a fully
constrained plate of 31 constraints (a rectangle, a bolt circle of four
holes, a slot) with a line mid-draw, 0.11 ms median for `Ok`, 0.12 ms for
`Redundant` and 0.58 ms for `Conflicting` in a release build on a Ryzen 9
7950X, 0.2, 0.2 and 1.0 ms in the dev profile
(`crates/arrix-sketch/tests/inference_probe.rs`, reported, asserted only
under a generous 50 ms ceiling).

The C1 benchmark measures the rest on the bracket, C1's acceptance part
(`tests/docs/bracket`: a plate with a pocket, three counterbored holes,
two filleted edges, a spur gear joined and an edge of it filleted), in the
dev profile on a Ryzen 9 7950X (`cargo test -p arrix-cli --test
c1_benchmark -- --nocapture`); each is a median over runs, and a met budget
is asserted at the budget itself, five times or more above the number.
A width edit applied by the authority (the debug build's document hash
included) takes 0.10 ms, and by a replica (its hash checked) 0.10 ms;
0.011 ms each in a release build. Saving the bracket as the session saves
it, to a directory or an `.arrx`, takes 0.2 to 0.4 ms, and loading it as
much. A newer width edit submitted while the one before is joining the gear
hears its first event 10 ms later (6 ms in release), which is the stop
inside the boolean, the restart, and the plate's sketch evaluated again.
In the browser the same measurement, read on the page (headless Chromium
155, dev profile, `cargo test -p arrix-plugin-host --test web_cancel --
--nocapture`), is 9 ms median over 11 runs (6 to 14 ms): the counter bump,
the stop at Arris's next step, the worker's next message, and the plate's
sketch evaluated again. It is gated at 50 ms. A preview posted during a
commit leaves the commit alone and runs behind it; a commit posted during a
preview stops it, and the preview is never heard. On a page without
isolation the stale evaluation runs to its end: the newer generation's
first event comes about 320 ms after the edit, and it still finishes.
`arrix eval tests/docs` (nine documents) takes 1.30 s on one thread and
0.43 s with `--jobs 4`.

The first coarse mesh after a width edit is missed: 324 ms median, 290 ms
in a release build. The evaluation is 315 ms of it, and two features are
most of that, the gear's join (135 ms) and the fillet on its edge
(112 ms); the pattern is 38 ms and the hole 11, and the gear itself is
cached (its inputs do not move with the width). Inside those two, the
boolean is 38 ms and the blend 13 ms, and the rest is the mass properties
each feature takes of its body: about 95 ms each, in Arris's
`measure::mass_properties`, for a body that carries the gear's 240 faces
(the gear's extrude alone measures in 84 ms). Arris 0.5, 0.6 and 0.7-dev replay
it alike in a release build, about 90 ms (the benchmark on 0.6: 348 ms), from the recipe
`crates/arrix-kernel/tests/recipes/bracket-mass-properties`. ArriX also
measures some bodies more than once (the hole's and the pattern's
results, 2 to 8 ms each). The coarse mesh after the evaluation is 9 ms.
`docs/BACKLOG.md` has both lines.

## Open questions

