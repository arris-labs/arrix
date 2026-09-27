# ADR-0008: A body input is an owned handle, not a borrow

- Status: Accepted
- Date: 2026-09-27
- Supersedes: ADR-0007, decision 5, in the one word `borrow<body>`

## Context

ADR-0007 gave the plugin API a body input as `input-value.body
(borrow<body>)`: the host lends the feature the slot's version for its
evaluation. Building it (`plans/c1-m1-slice` step 4) showed that
`wit-bindgen` 0.62, the latest release, cannot generate a Rust guest for
it. A `borrow` of an imported resource inside a `list` parameter of an
exported function (here `evaluate`'s `list<resolved-input>`) makes the
generated code fail the borrow checker (E0506 inside `generate!`); the
same borrow outside a list compiles, and so does an owned handle inside
the list. A Tier 1 plugin written in Rust, and the equality test that
holds the world and the Rust traits equal, both go through that
generator. The human chose the owned handle (2026-09-27).

## Decision

A body input is `input-value.body(body)`: the host hands the feature a
handle of its own to the slot's version, which the feature drops when it
is done. Everything else in ADR-0007 stands. The Rust side is unchanged:
`InputValue::Body(Body)`, read through `&[ResolvedInput]`, so a feature
can use it as a kernel operand. `InputValue`, `ResolvedInput` and the
host's `FeatureArgs` are not `Clone`, since a `Body` is not.

## Consequences

- A feature could put an input's handle into an output slot as it is.
  The host treats that like any other output: for a slot that modifies
  the input it is an unchanged next version, and anywhere else it is
  checked when body inputs are resolved (the evaluator's step of the
  same plan).
- A Tier 1 host mints one handle per body input per evaluation, which
  the component drops; no body is copied, since a handle names a body in
  the host's table.
- If a later `wit-bindgen` generates the borrow correctly, going back is
  a breaking change of the world, taken by an ADR of its own, not for its
  own sake.

## Alternatives considered

- **A kernel lookup**, `kernel.input-body(name) -> body`, with the input
  value carrying nothing: inputs stay plain data, but a new kernel
  function exists only to route around the generator, and an input's
  value would no longer say what it is.
- **Waiting for `wit-bindgen`**: the plan's steps after this one need
  body inputs, and the fix is not in any release.
