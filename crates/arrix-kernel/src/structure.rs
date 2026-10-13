//! A STEP file's product structure (ADR-0032): its occurrences with their
//! names, placements, colours and solids, as plain data in metres. The
//! solids themselves are read one at a time by `import_step`, each in the
//! frame the feature asks for.

use std::sync::Arc;

use arris::io::step::{self, LengthUnit, Occurrence, ProductTree, ReadOptions};
use arris::math::{Isometry, Vec3};
use arrix_core::Frame;

use crate::convert::dvec3;
use crate::record::{FileBytes, FileHash, FileSolid};
use crate::{Kernel, KernelError, KernelOp};

/// What a STEP file holds, as `read_step_structure` found it.
#[derive(Clone, Debug, PartialEq)]
pub struct StepStructure {
    /// The file's assemblies: usually one root, whose children are the
    /// parts and sub-assemblies; a file of one part is one root with
    /// solids and no children.
    pub roots: Vec<StepOccurrence>,
    /// Every solid, by `import_step`'s number minus one.
    pub solids: Vec<StepSolid>,
}

/// One solid of the file: its entity, which placement of it, and why the
/// reader refused it, if it did.
#[derive(Clone, Debug, PartialEq)]
pub struct StepSolid {
    pub entity: FileSolid,
    pub refused: Option<String>,
}

/// One use of a product in the file's tree.
#[derive(Clone, Debug, PartialEq)]
pub struct StepOccurrence {
    /// The file's product; occurrences of one product are one part.
    pub product: Option<u64>,
    pub name: String,
    /// Where it sits in its parent's frame, in metres, or why the file's
    /// transformation is not one ArriX reads.
    pub placement: Result<Frame, String>,
    /// The colour of its solids, red, green and blue in 0 to 1.
    pub colour: Option<[f64; 3]>,
    /// The solids it holds itself, as `import_step` numbers them (from 1).
    pub solids: Vec<u32>,
    pub children: Vec<StepOccurrence>,
}

impl Kernel {
    /// The product structure of the STEP file `file`, read in metres. The
    /// bodies the read made are dropped at once: a solid is read for a
    /// feature by `import_step`, in the frame the feature asks for.
    /// Errors as `import_step`'s do for the file as a whole. A recorded
    /// call.
    pub fn read_step_structure(&mut self, file: &[u8]) -> Result<StepStructure, KernelError> {
        let hash = FileHash::of(file);
        let bytes: Arc<[u8]> = self
            .files
            .entry(hash.clone())
            .or_insert_with(|| Arc::from(file))
            .clone();
        let call = self.begin(
            KernelOp::ReadStepStructure {
                file: hash,
                bytes: FileBytes(Some(bytes)),
            },
            &[],
        )?;
        let text = String::from_utf8_lossy(file);
        let options = ReadOptions {
            length_unit: LengthUnit::Metre,
            ..ReadOptions::default()
        };
        let read = self
            .controlled(|m, control| step::read(m, &text, &options, control))
            .map_err(|e| KernelError::read(call, &e))?;
        self.release_read(&read);
        let solids = read
            .solids
            .iter()
            .map(|s| StepSolid {
                entity: FileSolid {
                    id: s.entity.id,
                    instance: s.entity.instance,
                },
                refused: match &s.result {
                    Err(refusal) => Some(refusal.to_string()),
                    Ok(r) if self.is_sheet(r.body) => {
                        Some("a surface model, not a solid (kernel.import.sheet)".to_owned())
                    }
                    Ok(_) => None,
                },
            })
            .collect();
        Ok(StepStructure {
            roots: read.products.roots.iter().map(occurrence).collect(),
            solids,
        })
    }
}

fn occurrence(o: &Occurrence) -> StepOccurrence {
    StepOccurrence {
        product: o.product,
        name: o.name.clone(),
        placement: match &o.placement {
            Ok(iso) => frame_of(iso),
            Err(refusal) => Err(refusal.to_string()),
        },
        colour: o.colour.map(|c| c.0),
        solids: o.solids.iter().map(|&i| i as u32 + 1).collect(),
        children: o.children.iter().map(occurrence).collect(),
    }
}

/// The frame an isometry carries the world frame to.
fn frame_of(iso: &Isometry) -> Result<Frame, String> {
    let rotation = iso.rotation();
    Frame::new(
        dvec3(iso.translation()),
        dvec3(rotation * Vec3::x()),
        dvec3(rotation * Vec3::z()),
    )
    .map_err(|e| e.to_string())
}

/// The placement of solid `index` of the read, composed from its root, or
/// the identity for a solid no occurrence holds. `Err` says which
/// transformation Arris does not read.
pub(crate) fn product_placement(tree: &ProductTree, index: usize) -> Result<Isometry, String> {
    fn find(o: &Occurrence, above: Isometry, index: usize) -> Option<Result<Isometry, String>> {
        let here = match &o.placement {
            Ok(p) => p.then(&above),
            Err(refusal) => {
                let holds = o.solids.contains(&index) || has(o, index);
                return holds.then(|| Err(format!("{}: {refusal}", o.name)));
            }
        };
        if o.solids.contains(&index) {
            return Some(Ok(here));
        }
        o.children.iter().find_map(|c| find(c, here, index))
    }
    fn has(o: &Occurrence, index: usize) -> bool {
        o.solids.contains(&index) || o.children.iter().any(|c| has(c, index))
    }
    tree.roots
        .iter()
        .find_map(|r| find(r, Isometry::identity(), index))
        .unwrap_or(Ok(Isometry::identity()))
}
