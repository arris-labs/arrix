# ADR-0018: Forms are the feature type's spec

- Status: Accepted
- Date: 2026-10-08

## Context

M3's in-list has the property panel rendered from forms, the gear's
included, and the extrude preview. `SEED.md` §6.2 and `docs/PLUGINS.md`
§6. UI say a plugin contributes "a property form for each feature type" as
declarative data, but nothing says where that data comes from, and the
world has no UI export. A feature type already declares everything a form
needs to show but one thing: `FeatureTypeSpec` lists its parameters (name,
title, quantity kind, default expression), its inputs (name, title, kind,
whether a list) and its output slots. What it cannot declare is a choice.
The core types take words the record keeps in `choices` (extrude's `mode`,
the hole's `extent` and `kind`, the boolean's `op`, the datum plane's
`world`), each checked by its own type under its own code, and a plugin
type takes none (`docs/PLUGINS.md` §Tier 0 as built). Nor can it say that a
field matters only under a choice: extrude's `target` in `join` and `cut`,
the hole's counterbore fields under `counterbore`.

The other question is how a form commits. One gesture is one command
(`SEED.md` §8.2), but a new feature's fields are filled one at a time, and
a feature half filled does not evaluate.

Product scenario: a plugin author declares a feature type with a choice
and two inputs, registers it, and the user creates and edits it through a
property card the same as an extrude's, with its validation and its
preview, without the plugin writing a line of UI.

## Decision

1. **A feature type's form is derived from its `FeatureTypeSpec`, and from
   nothing else.** One generic renderer draws every type's form, built-in
   or plugin, so a plugin's form needs no UI export and looks like the
   rest. Parameters are quantity or count fields that take expressions,
   inputs are reference fields (a list input a list of them), and choices
   are combos. A form for a plugin's record kind, and panels, wait for a
   plugin that needs one.
2. **Choices are declared**: `ChoiceSpec { name, title, options, default }`,
   each option a word with a title, listed in `FeatureTypeSpec::choices`.
   The evaluator gives every declared choice to the feature (the record's
   value or the default) and refuses a name the type does not declare or a
   word it does not list. Plugin API 0.5.0 (breaking while 0.x); a plugin's
   `evaluate` receives its choices as a core type's does. The document
   schema does not change: the record already stores choices.
3. **`shown_when`**: a parameter or an input may be shown only while a
   choice has one of the listed values. A hidden field is neither shown
   nor required; its value, if the record has one, is kept.
4. **A create form is a draft**, session state seeded from the spec and
   the user's parameter defaults (ADR-0015). Its fields change the draft
   and issue nothing; OK is one `AddFeature` (in a `Group` with `AddPart`
   when the document has no part) and Cancel issues none. While the draft
   is complete enough to evaluate, the viewport previews it (ADR-0019).
5. **An edit form commits field by field**: each committed field or choice
   is one `EditFeature`, an equal value none. The form is rebuilt from the
   document each frame, so undo and redo show through it.
6. **Validation as a form can do it**: a field's text is checked by the
   form's own rules, the expression's parse and its quantity kind against
   the document's parameters. A refused text stays in the field with the
   reason and issues nothing. What only evaluation can say (a profile that
   does not close, a fillet too large) is the feature's diagnostic, shown
   at the head of its form; a plugin has no separate validate export.

## Consequences

Every built-in feature proves the renderer a plugin's form uses, and a
plugin type gains choices and conditional fields by declaring them. The
per-type codes for a bad choice value become one evaluator check
(`feature.choice`) wherever nothing else reaches them (`docs/DATA-MODEL.md`
§Features). A form cannot hold a widget its spec has no word for: a
toggle, free text or a custom layout is a later spec change, made when a
type needs one. A create form's half-filled draft never reaches the
document or its undo history, at the price of a preview path that
evaluates a command without applying it.

Amends `docs/PLUGINS.md` §6. UI (a feature type's form is its spec) and
§Tier 0 as built (a plugin type takes choices), and `docs/UI-RENDERING.md`
§Declarative plugin UI. Supersedes nothing.

## Alternatives considered

- **A form export in the world** (a plugin returns a form description
  beside its spec): a second description of the same fields that can
  disagree with the first, and a UI contract to version for no gain yet.
- **Choices as count parameters** (0, 1, 2 for `new`, `join`, `cut`): an
  expression where a word belongs, and a record that reads as numbers.
- **A create form that commits each field** (`AddFeature` at the first
  field, `EditFeature` after): a half-built feature in the document and in
  undo, failing until it is finished, and a Cancel that has to be an undo.
- **Every field always shown**: a cut's `target` beside a new body's form,
  and the hole's counterbore fields on a simple hole, to be explained
  rather than hidden.
