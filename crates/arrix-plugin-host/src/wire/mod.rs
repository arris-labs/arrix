//! The wire (ADR-0022): JSON-RPC 2.0, one JSON object per line, with the
//! shapes of the plugin API's types generated from the WIT world.
//!
//! - [`rpc`]: the framing, shared by the Tier 2 host and `arrix serve`:
//!   requests, responses, errors, and a line reader with a length limit.
//! - the codec: a pair of functions per type of the world between the
//!   plugin API's Rust types and `serde_json::Value`. `generated.rs` is
//!   written by `arrix-plugin-api`'s `tests/wire.rs`; [`custom`] holds the
//!   types whose Rust form checks what it holds (a frame, a name, a
//!   reference, a profile), by hand, held to the same schemas by this
//!   crate's tests.
//! - the method table ([`METHODS`]): every function of the world and which
//!   side serves it.

mod custom;
mod generated;
mod prim;
pub mod rpc;

pub use custom::*;
pub use generated::*;
pub use prim::*;

/// Which side of the pipe serves a method.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// A `feature` function: the host calls, the plugin answers.
    HostToPlugin,
    /// A `kernel` function: the plugin calls, the host answers.
    PluginToHost,
}

/// A function of the world, as a JSON-RPC method (`<interface>.<function>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Method {
    pub name: &'static str,
    pub direction: Direction,
}

/// The method `name`, when `served_by` is the side that serves it. A
/// method the other side serves is as unknown as one the world lacks, so
/// a plugin cannot call `feature.evaluate` on the host.
pub fn method(name: &str, served_by: Direction) -> Result<&'static Method, rpc::RpcError> {
    METHODS
        .iter()
        .find(|m| m.name == name && m.direction == served_by)
        .ok_or_else(|| rpc::RpcError::method_not_found(name))
}
