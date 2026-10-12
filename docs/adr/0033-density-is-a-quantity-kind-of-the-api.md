# ADR-0033: Density is a quantity kind of the plugin API

- Status: Accepted
- Date: 2026-10-12

## Context

ADR-0031 §1 makes a material's density an expression of a new quantity
kind and §7 says `arrix-plugin-api` is unchanged. `QuantityKind` is one
type: `arrix-core`'s, re-exported by the API, mirrored by the WIT enum
`quantity-kind` and the wire codec. A variant added to it is an API change.
Found at plans/c2-m3 step 3.

## Decision

1. **`QuantityKind::Density` joins the one enum** (units `kg/m3` and
   `g/cm3`), in `arrix-core`, the WIT world and the wire codec. Plugin API
   **0.6.0, breaking** (a `0.x` minor): a plugin built for 0.5 does not
   load, and `match`es on the enum gain an arm. Decided by the human,
   2026-10-12.
2. **ADR-0031 §7 is amended to this extent only**: the API's
   `MassProperties` stays volume, area and centroid, and no material or
   mass type enters it.
3. The expression grammar reads a compound unit (`7.85 g/cm3`) as one unit
   when it follows a number.

## Consequences

- Plugins may declare density parameters; the app's forms hint `kg/m3`.
- First-party plugins and fixtures move to `^0.6`.

## Alternatives considered

- **A document-only kind** beside the API's: two kind enums and two unit
  tables to keep in step, for a kind a plugin may want anyway.
