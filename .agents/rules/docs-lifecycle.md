# Docs lifecycle: backlog → idea → plan → docs

The tiers, their files and lifetimes are in `docs/README.md` §Document
lifecycle. The skills `/idea`, `/plan`, `/work`, `/retire-plan` and
`/close-cycle` walk the pipeline. These rules hold whether or not a skill
is running:

- **An idea is a brainstorm, a plan is a todo.** An idea has options and a
  decision for the human, never steps or checkboxes. A plan has
  commit-sized checkbox steps and never re-argues its idea's decision; it
  links the ADR if one was needed. A small, obvious task skips the idea.
- **At most two active plans**, always in `docs/plans/`. A third means
  retiring one first.
- **A plan is read in two minutes**: under ~150 lines and ~10 steps, each
  step a few lines (its detail lives in *Design deltas* or an ADR). Work
  larger than that is split into milestone plans (`c4-m1`, `c4-m2`), each
  with its own accept, planned one after the other.
- **An idea one plan covers only in part** is cut down when that plan
  absorbs its part: the file keeps only what is still unplanned, with
  `Status: Accepted (<option>); <part> absorbed by plans/<slug>`, and the
  next plan absorbs the rest and deletes it.
- **`GLOSSARY.md` holds the project's words.** A new or sharpened term is
  written there in the commit that first uses it in a design doc, ADR or
  public API; a word the glossary lists under _Avoid_ is replaced by its
  term.
- **Work outside the current cycle's in-list needs an ADR first**, naming
  the product scenario it unlocks. The idea proposes it; the plan's first
  step writes it. Scope is argued, not accreted.
- **Core or plugin** is asked of every new capability (`SEED.md` §6.2). The
  idea says which and why; a domain put into the core needs an ADR.
- **A missing kernel capability is not planned around.** It goes to Arris
  (a fixture, a backlog line or an idea there), and the plan lists it under
  *Arris dependencies*.
- **Size limits keep the always-loaded docs cheap to read**: a closed
  roadmap section is five parts under ~40 lines (goal, status line, in,
  out, accept); `AGENTS.md` "Current state" stays under ~15 lines, at cycle
  granularity. A fact cut from either moves to the topic doc or ADR that
  owns it.
- **One roadmap file, no `CHANGELOG.md`.** What landed lives in the git log,
  the roadmap's status lines and the ADRs.
