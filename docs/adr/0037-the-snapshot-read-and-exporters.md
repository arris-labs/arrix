# ADR-0037: the snapshot read and exporters

- Status: Accepted
- Date: 2026-10-13
- Amends: ADR-0034 §Decision 2 (its shape; its scope stands)

## Context

ADR-0034 gives C3 M2 a snapshot read imported by the world and an
`exchange` export, on all three tiers, for `robotics`' URDF. plans/c3-m2
step 1 probed a scratch world with both beside `feature`
(`crates/arrix-plugin-host/tests/probe_world.rs`):

- A guest built for a smaller world (`feature` only, or `exchange` only)
  componentises, and the component has that world's exports only. One
  host bound to the whole world loads it interface by interface; the
  world's own bindings (`instantiate`) refuse it.
- An import the guest never calls is absent from its component, so a
  component without `document-read` can be refused at load by its imports,
  as for any import beyond the world's (ADR-0035 §4).
- A 199 712-triangle mesh through `document` and back out as a 10 MB
  binary STL costs 25 ms and 86 M fuel at Tier 1 (8 % of the 2^30
  default). On the Tier 2 wire (base64 runs, ADR-0022 §5) the mesh line is
  6.4 MB (CPython reads it in 25 ms) and the STL line 13.3 MB (56 ms to
  write). Neither crossing needs a new encoding, a larger default fuel or
  a streaming call.

## Decision

1. **One world.** `world plugin` imports `kernel` and `document` and
   exports `feature` and `exchange`. A plugin implements what it
   contributes, and the SDK supplies the rest empty (`describe` returns no
   feature types, `exporters` no exporters). The alternative, a world per
   shape, makes the equality test and the generated wire carry several
   worlds and each host adapter ask which exports a component has, for no
   gain: the probe's finding makes the one-world form safe, since an
   import a guest does not call is not in its component.
2. **`export_plugin!` takes the parts by name**: `export_plugin!(Spur)`
   stays a features-only plugin, and `export_plugin!(features = A,
   exporters = B)` names either or both. A guest that names no exporter
   generates no call to `document`, so its component does not import it.
3. **`interface document`**, read-only, granted by `document-read`: plain
   records in SI, Z-up, with `arrix-core`'s ids. By function, so a plugin
   fetches only what it writes:
   - `parts()`: id, name, final bodies by `<feature>.<slot>`, each with
     the material's name and colour. `mesh(part, body)`: the body's
     triangles in the part's frame, as `f32` positions and normals and
     `u32` indices. `mass(part)`: mass, centroid and inertia about the
     centroid (`arrix_doc::Inertial`), or the `mass.*` diagnostic.
   - `assemblies()`: each one's components at every depth by path, with
     the part each is of and its solved pose in the assembly's frame; and
     its joints with kind, parent and child paths, anchor frames in each
     one's component frame, axis, limits in SI, value and status. A
     component the solver could not place has no pose, and the status says
     why.
4. **Meshes are refined, at the chord STL export uses (1e-4 m)**, fixed
   and not an option. A plugin that needs a different tolerance asks for it
   by an API change; an option every exporter must honour is not one the
   probe's sizes call for. (This closes the plan's OPEN 4.)
5. **When it may be called.** Only while an `exchange` call runs, against
   the generation the call was made on. A `document` call outside one, one
   made during `evaluate`, or one by a plugin without `document-read` is
   `plugin.capability`; a component that imports `document` without the
   capability is refused at load (`plugin.load`). `evaluate` keeps `kernel`
   and its resolved inputs only (`docs/PLUGINS.md` §Capabilities).
6. **`interface exchange`**: `exporters() -> list<exporter-spec>` (id
   `<plugin>.<name>`, title, the extension of the main file, its subject,
   a part or an assembly, `params` and `choices` as a feature's specs, so a
   form derives as for a feature) and `export(id, subject, params,
   choices) -> result<list<file>, diagnostic>`, with `file { name, bytes }`.
   The first file is the main one. Options arrive evaluated, as a
   feature's parameters do.
7. **A file's name** is a relative path of `/`-separated parts, none
   empty, `.` or `..`, with no drive or leading `/`; anything else is
   `export.path` and nothing is written. Two files of one name are
   `export.path` too. A plugin never sees a path (`docs/PLUGINS.md` §4).
8. **The host writes the files.** One file goes to the path given. Several
   go into a directory at that path, created if missing, so that readers
   (a URDF's meshes) resolve siblings relative to the main file. A file
   already there is an error unless `--force` is given. No archive format
   is part of the API. (This closes the plan's OPEN 3.)
9. **Determinism.** An export is a pure function of the snapshot and its
   options. At Tier 1 it runs in a fresh store with fuel and the
   epoch-callback interrupt, as `evaluate` does; its math is
   `arrix_plugin_api::math` (ADR-0036). The test kit runs each declared
   export twice and requires identical files. Tier 0 and Tier 1 give the
   same files, to the byte.
10. **The core's formats are exporters too.** `core.step`, `core.stl` and
    `core.obj` are in the registry, listed by `describe` and reachable by
    `--exporter`, with `--format` kept as their shorthand. They are
    implemented natively and do not go through the snapshot: STEP needs
    the B-rep, which the snapshot does not carry (Arris ask A2).
    `docs/PLUGINS.md` §4's line stands. (This closes the plan's OPEN 1.)
11. **The session.** `Request::Export` takes an exporter id beside
    `format`, a subject and its options, and `Exported` carries the files.
    The protocol schema's version is bumped (ADR-0026). The app's export
    dialog listing plugin exporters is a backlog line (the plan's OPEN 2).
12. **API 0.7.0, breaking**: the world grew, so a component must provide
    an `exchange` export the 0.6.0 world did not have. Every first-party
    manifest's `api` range follows, as do the wire's generated codec, the
    Python SDK's stubs and `wit_equality.rs`'s inventory.

## Consequences

- A URDF, a bill of materials or a mesh format is a plugin at any tier,
  from the document's evaluated state alone, with no kernel call and so no
  call record to replay.
- A large export crosses the boundary in whole meshes. The probe's 200 000
  triangles are cheap; a model far past that would want a streaming read,
  an additive change when a document needs it.
- A plugin cannot read a body's B-rep bytes (Arris ask A2); a STEP-like
  exporter in a plugin waits for that.
- The snapshot is not persisted, so the document's schema does not change.
- A guest compiled for 0.6.0 is refused by its `api` range, as any
  breaking bump does.

## Alternatives considered

- **A world per shape** (features-only, exchange-only, both): rejected for
  the reasons of §1.
- **A snapshot passed whole as an argument to `export`**: one crossing, but
  every exporter pays for every mesh, and a bill of materials needs none.
  The probe shows by-function reads are cheap.
- **Several files as a zip**: one file to hand over, but URDF readers
  resolve meshes beside the file, so the user unpacks it first.
- **The plugin writing to a path**: a capability the browser and a remote
  server do not have, and the path leaks into the output.
