---
name: idea
description: Turn a raw feature idea or design question from the human into a brainstorm document under docs/ideas/ — problem, options with trade-offs, cost, conflicts with existing decisions, core-or-plugin, a recommendation, and the decision the human has to make. Use when the human says "idea:", "what if", "should we", "think about", or hands over a backlog line; also when a plan request is too vague or contested to plan directly. Produces analysis only — never code, never a plan, never checkboxes.
argument-hint: <idea in one sentence, or a backlog line>
---

# /idea — brainstorm, don't plan

An idea is where thinking happens so a plan doesn't have to. Not every idea
becomes a plan; a clear "no, because…" is a successful outcome.

## Do

1. Pick a slug (`kebab-case`, noun phrase). If `docs/ideas/<slug>.md` exists,
   extend it rather than duplicating.
2. Read what constrains the idea before forming an opinion: `SEED.md`
   (non-goals, core vs plugin §6.2, the plugin model §6.3, the kernel's
   sensor §6.6, the kickoff decisions §9), the design doc(s) it touches,
   every ADR whose decision it might bend, `docs/ROADMAP.md` for which cycle
   it would belong to, and the code if it exists. A kernel question is
   answered from Arris's API, corpus and roadmap (`docs/notes/LOCAL.md` has
   the checkout), not from memory. Cite paths and ADR numbers.
3. Write `docs/ideas/<slug>.md` from `docs/ideas/TEMPLATE.md`. Options are
   real alternatives with honest trade-offs, including "do nothing". Cost is
   sized in plan steps, not hours. Name what the idea *conflicts* with
   explicitly: a `SEED.md` non-goal, an ADR, a layer rule, plugin-API
   stability.
4. Say whether it belongs in the **core or a plugin**, and why. A plugin
   idea that needs a new contribution point or API type says so: that part
   is a core change.
5. If it needs a kernel capability Arris lacks, name it as an Arris
   dependency. The idea may recommend an Arris ask; it never designs a
   workaround.
6. If the idea sits in no cycle's in-list, say so, and name the product
   scenario the ADR it would need has to state
   (`.agents/rules/docs-lifecycle.md`).
7. End with one recommendation and the concrete decision(s) the human must
   make, phrased as questions with your preferred answer first.
8. If the idea came from `docs/BACKLOG.md`, remove that line.
9. Tell the human the recommendation and the decisions in the reply; the file
   is the record, the reply is the conversation.

## Don't

- No implementation steps, no checkboxes, no file lists: that is `/plan`.
- Don't create an ADR; an idea *proposes* that one may be needed.
- Don't touch code or design docs.
- Don't cite `docs/notes/` as a source; restate what the idea needs.
- Don't write more than the decision needs. Two pages is long.

`$ARGUMENTS`
