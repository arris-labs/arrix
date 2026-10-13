//! Transcendental math that gives the same bits on every target (ADR-0036,
//! docs/CONCURRENCY-WASM.md §Determinism).
//!
//! `f64::sin` and its kin are the system's libm natively and Rust's own on
//! wasm32, which differ in the last place, so a plugin using them gives
//! different bodies at Tier 0 and Tier 1. These are the `libm` crate's,
//! compiled into the plugin, the same code on either side. A plugin
//! evaluates with these, never with `f64`'s methods. `sqrt` and the basic
//! arithmetic are exact under IEEE 754, and are the same everywhere already.

pub use libm::{
    acos, asin, atan, atan2, cbrt, cos, cosh, exp, exp2, expm1, hypot, log, log1p, log2, log10,
    pow, sin, sincos, sinh, tan, tanh,
};
