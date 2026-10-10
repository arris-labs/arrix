# ADR-0024: The authority owns the registry; a plugin load is a generation

- Status: Accepted
- Date: 2026-10-10

## Context

ADR-0016's scenario loads an agent-written plugin into a *running*
session. Constraint 4 asks for edit, reload, see, with no rebuild. Today
`LocalSession::open(document, registry)` moves the `Registry` into the
evaluator for the session's life. The evaluator caches by an input hash
that includes a type's version but not its code. And
`docs/CONCURRENCY-WASM.md` §The evaluator lets a client keep the newest
generation's events: one generation gives one evaluation.

There were three ways to change the feature types under a live session:

- reopen the session with a new registry;
- share the registry behind a lock;
- make the change a request of the authority's.

## Decision

1. **The authority owns the registry.** `Request::LoadPlugin { dir }` loads
   a plugin directory on the authority's machine, through a `PluginLoader`
   that is handed to `LocalSession::open` (`arrix-plugin-host` implements
   it for Tier 2). `Request::ReloadPlugin { plugin }` restarts a loaded
   plugin and takes its types again. A session with no loader (wasm, or a
   test) refuses both with `Rejected::NoPluginLoader`. A load that fails is
   `Rejected::PluginLoad(diagnostic)`.
2. **Neither is a command.** Like a preview (ADR-0019) or an export,
   neither makes an undo entry or a gesture, and the document does not
   change. A plugin is never persisted in a document.
3. **Each makes a new generation.** The authority bumps the generation
   without stamping a node, so no author's pending command goes stale. It
   publishes `SessionEvent::Registry { generation, added, removed }`
   (feature-type specs added or replaced, type ids removed) and then
   evaluates that generation like any other. A feature that was frozen for
   lack of a type evaluates live under it. A reload makes a generation
   even when no spec changed, because the code did.
4. **A reload drops the cache entries of its types** before evaluating,
   since an equal version no longer promises equal code. Other types'
   entries stay.
5. **A subscriber learns the types the authority holds from `Opened`**,
   which gains the feature-type specs loaded after open. Before that, a
   client knows only the registry it opened the session with.
6. **A replica follows `Registry`** by taking its generation, with no
   document change. The app rebuilds its command registry and forms from
   the specs.

## Consequences

- "Newest generation wins" stays the only ordering rule. A client that
  never loads a plugin needs nothing new beyond taking `Registry`'s
  generation.
- Undo and redo stacks survive a load. Undo stays per author and by
  inverse command, and a registry change is in neither stack.
- In a future remote mode, plugins run on the server, which is where the
  directory path is resolved.
- Unloading a plugin is not built in M2 (backlog). It would be a third
  request that re-freezes that plugin's features.

## Alternatives considered

- **Reopen the session with the new registry.** It needs no change in
  `arrix-doc`, but it loses the undo stacks, restarts generations at 0 and
  makes every base generation an agent holds meaningless.
- **A registry shared behind a lock.** The evaluation would change outside
  commands and events, replicas would never hear of it, and one generation
  would evaluate two ways. That breaks `SEED.md` §6.1 and the determinism
  contract.
- **A registry epoch on the same generation.** It avoids a generation per
  load, but every client would have to order events by a second key.
