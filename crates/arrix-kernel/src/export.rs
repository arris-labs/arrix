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
