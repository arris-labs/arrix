# Review: a plan's diff, read against the plan and the standards

Read by the reviewer subagent `/retire-plan` starts, never by the agent
implementing a step: the reviewer gets a diff and has no exploration to do,
so the judgement calls live here instead of in every session's context. A
rule a lint, the compiler or a test already enforces (fmt, clippy, the
size, wasm and layer lints, `gate-selftest`, the goldens) is left to them.

## The brief

Two axes, reported apart so one cannot mask the other.

**Plan.** Quote the plan line for each finding:

1. a *Goal*, *Design deltas* or *Acceptance* item the diff leaves missing
   or partial;
2. behaviour the diff adds that the plan did not ask for, or that its
   *Non-goals* fence out;
3. an item that looks done but looks wrong;
4. a box ticked whose commit carries no test, scenario or golden for it;
5. a *Docs to update* line or a design doc the diff makes untrue.

**Standards.** Cite the rule (this file, `SEED.md` §, an ADR) for each
finding, and quote the hunk. Mark each **hard** (a stated rule broken) or
**judgement** (a smell worth a look).

Under 500 words in all; nothing found on an axis is reported as such.

## The standards

### Boundaries
- A command, event or view type crossing the session boundary is plain,
  serialisable, versioned data: no `Rc`, closures, kernel handles or
  pointers into the document (`SEED.md` §6.1).
- Session state (selection, camera, active sketch, edit mode) stays out of
  the document; a command names its base generation.
- A first-party plugin uses `arrix-plugin-api` alone; a need the API cannot
  meet is an API change with its semver effect in the commit body.
- Every kernel call goes through `arrix-kernel` as a recorded call. A
  kernel gap is an Arris fixture recipe and ask, named in the reply; a
  workaround, fallback path or `#[ignore]` in ArriX for it is a hard
  finding.

### Behaviour
- One gesture is one command; a commit equal to the current state makes no
  command (`SEED.md` §8.2).
- A failure is soft and loud: the failed feature keeps its parameters and a
  diagnostic says why; a lost reference is a diagnostic with candidates.
  A tolerance nudge or a silent re-bind is a hard finding.
- Evaluation and plugin `evaluate` are deterministic: no `HashMap`
  iteration order, time or randomness reaching an output.
- A change to what a document persists is a schema bump with its migration
  and a fixture.
- SI inside, Z-up, right-handed; units are converted only at a boundary.

### Tests
- A wanted behaviour that does not work yet is a plan step or an Arris
  ask, never an `#[ignore]`d test.
- A refreshed golden's commit says why the look or output changed.
- A test asserts the behaviour through the surface a user or plugin has
  (a command, the CLI, the protocol, the test kit), not a private helper,
  wherever such a surface reaches it.

### Docs and words
- Docs change in the commit that changes the behaviour, in present tense,
  without the plan's history.
- A term `GLOSSARY.md` defines is used as written; one it lists under
  _Avoid_ is a judgement finding. A new concept named in public API or a
  design doc has its glossary entry.
- Code smells, always judgement: a name that does not say what it holds; the
  same logic twice in the diff; a function reaching into another module's
  data more than its own; parameters that always travel together; a string
  or number standing for a concept that deserves a type; abstraction or
  hooks for a need the plan does not have.
