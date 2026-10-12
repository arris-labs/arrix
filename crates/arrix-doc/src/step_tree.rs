//! A STEP file's product tree as the protocol carries it (ADR-0032): the
//! kernel's `StepStructure` as plain, serialisable data, so the session
//! can read a file on its worker and hand the tree to a client, which
//! turns it into one command (`import_products`).

use arrix_core::{DVec3, Diagnostic, Frame};
use arrix_kernel::{Kernel, StepOccurrence, StepStructure};
use serde::{Deserialize, Serialize};

use crate::eval::kernel_diagnostic;

/// What a STEP file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StepTree {
    /// The file's assemblies: usually one root; a file of one part is a
    /// root with solids and no children.
    pub roots: Vec<StepNode>,
    /// Every solid, by its number (from 1) minus one: why the reader
    /// refused it, if it did.
    pub solids: Vec<Option<String>>,
}

/// One use of a product in the file's tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StepNode {
    /// The file's product; occurrences of one product are one part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product: Option<u64>,
    pub name: String,
    /// Where it sits in its parent's frame, in metres, or why the file's
    /// transformation is not one ArriX reads.
    pub placement: Result<Placement, String>,
    /// The colour of its solids, red, green and blue in 0 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<[f64; 3]>,
    /// The solids it holds itself, as `core.import-step`'s `solid` counts
    /// them (from 1).
    pub solids: Vec<u32>,
    pub children: Vec<StepNode>,
}

/// A frame as the file gives it: an origin and two axes, exactly the
/// numbers the reader made (a `Frame` would normalise them again on every
/// crossing).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Placement {
    pub origin: [f64; 3],
    pub x_axis: [f64; 3],
    pub z_axis: [f64; 3],
}

impl Placement {
    /// The frame, or why the axes are none.
    pub fn frame(&self) -> Result<Frame, String> {
        let v = DVec3::from_array;
        Frame::new(v(self.origin), v(self.x_axis), v(self.z_axis)).map_err(|e| e.to_string())
    }
}

impl From<&Frame> for Placement {
    fn from(f: &Frame) -> Self {
        Placement {
            origin: f.origin().to_array(),
            x_axis: f.x_axis().to_array(),
            z_axis: f.z_axis().to_array(),
        }
    }
}

impl From<StepStructure> for StepTree {
    fn from(s: StepStructure) -> Self {
        StepTree {
            roots: s.roots.into_iter().map(StepNode::from).collect(),
            solids: s.solids.into_iter().map(|s| s.refused).collect(),
        }
    }
}

impl From<StepOccurrence> for StepNode {
    fn from(o: StepOccurrence) -> Self {
        StepNode {
            product: o.product,
            name: o.name,
            placement: o
                .placement
                .as_ref()
                .map(Placement::from)
                .map_err(Clone::clone),
            colour: o.colour,
            solids: o.solids,
            children: o.children.into_iter().map(StepNode::from).collect(),
        }
    }
}

/// The product tree of the STEP file `bytes`, read in metres on a kernel
/// of its own. `Err` is the file's refusal (`kernel.import.parse` and the
/// rest).
pub fn read_step_tree(bytes: &[u8]) -> Result<StepTree, Diagnostic> {
    Kernel::new()
        .read_step_structure(bytes)
        .map(StepTree::from)
        .map_err(|e| kernel_diagnostic(&e))
}
