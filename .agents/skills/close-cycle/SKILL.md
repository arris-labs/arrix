---
name: close-cycle
description: Close a finished cycle — verify nothing is still open, run the mandated drift review of every design doc against the code, compress the finished section of docs/ROADMAP.md to its status line, confirm the next cycle's section with the human, update the spine and AGENTS.md, and hand the tag to the human. Use when the human says "close the cycle", "C1 is done", "what's next after this cycle", or when the last plan of a cycle has been retired. Never tags and never pushes.
argument-hint: <cycle, e.g. C1>
---

# /close-cycle — one roadmap, one section per cycle

`docs/ROADMAP.md` is a **living design doc**, not a log. It is never forked
into a second roadmap file and never accumulates narrative: a finished cycle
shrinks to its status line, and the next one is already below it.

## Do

1. **Check it is actually finished.** Every in-list bullet either done or
   explicitly moved to `docs/BACKLOG.md`; `docs/plans/` holding nothing but
   `TEMPLATE.md`; the cycle's **Accept** run (executable where it is, the
   manual part named in the reply); the whole gate green
   (`docs/ARCHITECTURE.md` §Gates). If not, stop and report exactly what is
   open. Don't close around it.
2. **Drift review** (the only place it is mandated): read **every** design
   doc (`ARCHITECTURE`, `DATA-MODEL`, `PLUGINS`, `UI-RENDERING`,
   `CONCURRENCY-WASM`) against the code and fix each sentence that is no
   longer true. Check `SEED.md` too. It is not edited here, but a
   contradiction with the code is reported to the human as a needed ADR.
   Bring *Arris dependencies* up to date: asks that landed in a released
   `arris`, and asks still open. The cycle is not closed until the list is
   empty. List what you fixed in the reply; if nothing, say why you believe
   nothing drifted.
3. **Compress the finished section.** Exactly five parts, in this order:

   ```
   ## CN — <theme>
   *Goal: …*                        ≤ 3 lines
   **Status: done <date>, tag `cN`.** <the risk retired, the ADRs taken>
                                    ≤ 5 lines, one paragraph
   - <in bullets, as written when the section opened>
   **Out:** …
   **Accept:** …
   ```

   A closed section over **~40 lines** is not compressed; go again. Before
   deleting a sentence, grep the fact in `docs/`, `crates/`, `README.md` and
   the workflows. One that lives *only* here is **relocated, never dropped**,
   to the topic doc that owns it:

   | Fact | Home |
   |---|---|
   | crate layout, layer rule, the kernel choke point, testing, a lint gate | `ARCHITECTURE.md` |
   | a document type, schema version, naming, frozen-result or evaluation rule | `DATA-MODEL.md` |
   | a contribution point, tier, manifest field, API version rule | `PLUGINS.md` |
   | a render-path, picking, panel, shortcut or snapshot-harness rule | `UI-RENDERING.md` |
   | executor, wasm, determinism, batch evaluation, a performance budget | `CONCURRENCY-WASM.md` |
   | why a decision went the way it did | the ADR (a new one if none says it) |
   | something deferred | `BACKLOG.md`, one line |

4. **Confirm the next section.** Re-read the next cycle's goal/in/out/accept
   against `docs/BACKLOG.md`, the ⚠ OPENs it touches, what this cycle
   changed and which Arris asks have landed, and propose amendments. The
   theme and the in/out split are the **human's call**: propose, don't
   decide. If the answer is not obvious, stop and ask.
5. **`Spine:`** at the top of the roadmap reflects the new state.
6. **`AGENTS.md` "Current state"**: one sentence for the closed cycle, a new
   `**Next:**`, block still under ~15 lines.
7. Commit as `docs: close <cycle>`, with a body listing the docs updated and
   the drift fixed. Then **tell the human to tag** `cN` on this commit. Tags
   and pushes are theirs, never yours.

## Don't

- Don't create a second roadmap file, an `ARCHIVE.md` or a `CHANGELOG.md`.
- Don't leave the finished section at full length "because it is useful".
- Don't invent the next cycle's scope. A roadmap section is a commitment the
  human makes, not one the agent proposes into existence.
- Don't tag, push, or open plans for the next cycle here. `/idea` and
  `/plan` come after.

`$ARGUMENTS`
