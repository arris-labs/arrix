//! Bodies as exchange files (docs/ARCHITECTURE.md §The kernel choke point):
//! STEP of the exact B-rep, STL and OBJ of the refined meshes, each one
//! recorded call over the bodies given, in that order. Lengths are the
//! model's: metres.
//!
//! The writers are deterministic, so two exports of the same bodies are
//! the same bytes.

use arris::io::{obj, step, stl};
use arris::mesh::{MeshRequest, TriMesh};
use arrix_core::{ExportFormat, MeshDetail};

use crate::{CallIndex, Kernel, KernelBody, KernelError, KernelOp};

/// The name an STL file gives its solid.
const SOLID: &str = "arrix";

/// What reading a STEP file back found in one solid: the topology counts
/// and the volume, in the file's own numbers (metres, as written).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepSolid {
    pub faces: usize,
    pub edges: usize,
    pub vertices: usize,
    pub volume: f64,
}

/// Reads `text` with Arris's STEP reader into a scratch model and measures
/// each solid, to check an export from outside the kernel crate (the
/// layer rule keeps Arris here). Not a document import: that is C2.
pub fn read_step(text: &str) -> Result<Vec<StepSolid>, String> {
    let mut model = arris::topo::Model::new(arris::math::Precision {
        default_tolerance: Kernel::DEFAULT_TOLERANCE,
        ..arris::math::Precision::DEFAULT
    })
    .map_err(|e| e.to_string())?;
    let read = step::read(
        &mut model,
        text,
        &step::ReadOptions::default(),
        &arris::Control::NONE,
    )
    .map_err(|e| e.to_string())?;
    let mut solids = Vec::new();
    for solid in &read.solids {
        let solid = solid.result.as_ref().map_err(|e| format!("{e:?}"))?;
        let measured =
            arris::ops::measure::mass_properties(&model, solid.body, &arris::Control::NONE)
                .map_err(|e| e.to_string())?;
        solids.push(StepSolid {
            faces: model.faces(solid.body).map_err(|e| e.to_string())?.len(),
            edges: model.edges(solid.body).map_err(|e| e.to_string())?.len(),
            vertices: model.vertices(solid.body).map_err(|e| e.to_string())?.len(),
            volume: measured.volume,
        });
    }
    Ok(solids)
}

impl Kernel {
    /// `bodies` as STEP AP214 Part 21 text, one product holding a solid
    /// per body. Fails as `kernel.export.no-bodies` for none.
    pub fn export_step(&mut self, bodies: &[KernelBody]) -> Result<String, KernelError> {
        let call = self.begin(
            KernelOp::Export {
                format: ExportFormat::Step,
            },
            bodies,
        )?;
        let mut targets = Vec::with_capacity(bodies.len());
        for &body in bodies {
            targets.push(self.entry(body)?.body);
        }
        step::write(&self.model, &targets).map_err(|e| KernelError::step(call, &e))
    }

    /// `bodies` as binary STL, their refined meshes in one solid.
    pub fn export_stl(&mut self, bodies: &[KernelBody]) -> Result<Vec<u8>, KernelError> {
        let call = self.begin(
            KernelOp::Export {
                format: ExportFormat::Stl,
            },
            bodies,
        )?;
        let meshes = self.refined(call, bodies)?;
        stl::write_binary(&meshes, SOLID).map_err(|e| KernelError::mesh_write(call, &e))
    }

    /// `bodies` as OBJ text, their refined meshes one group per face.
    pub fn export_obj(&mut self, bodies: &[KernelBody]) -> Result<String, KernelError> {
        let call = self.begin(
            KernelOp::Export {
                format: ExportFormat::Obj,
            },
            bodies,
        )?;
        let meshes = self.refined(call, bodies)?;
        obj::write(&meshes).map_err(|e| KernelError::mesh_write(call, &e))
    }

    /// Each body's mesh at the refined chord, as the viewport's refined
    /// mesh is made, but in metres and without the corner block.
    fn refined(
        &mut self,
        call: CallIndex,
        bodies: &[KernelBody],
    ) -> Result<Vec<TriMesh>, KernelError> {
        let mut meshes = Vec::with_capacity(bodies.len());
        for &body in bodies {
            let target = self.entry(body)?.body;
            let request = MeshRequest::new(self.chord(target, MeshDetail::Refined));
            let mesh = self
                .controlled(|m, control| arris::mesh::tessellate_with(m, target, &request, control))
                .map_err(|e| KernelError::mesh(call, &e))?;
            meshes.push(mesh);
        }
        Ok(meshes)
    }
}
