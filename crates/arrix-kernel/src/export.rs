//! Bodies as exchange files (docs/ARCHITECTURE.md §The kernel choke point):
//! STL and OBJ of the refined meshes, each one recorded call over the
//! bodies given, in that order. Lengths are the model's: metres, and the
//! STEP file declares them.
//!
//! The writers are deterministic, so two exports of the same bodies are
//! the same bytes.

use arris::io::step::{self, LengthUnit, Occurrence, ProductTree, Rgb, WriteOptions};
use arris::io::{obj, stl};
use arris::mesh::{MeshRequest, TriMesh};
use arrix_core::{ExportFormat, MeshDetail};

use crate::convert;
use crate::{CallIndex, ExportNode, ExportTree, Kernel, KernelBody, KernelError, KernelOp};

/// The name an STL file gives its solid.
const SOLID: &str = "arrix";

impl Kernel {
    /// `bodies` as a STEP file declaring metres, no coordinate converted
    /// (Arris's `write_with`, ask A12), so a reader of the same unit gets
    /// every coordinate bit for bit.
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
        let options = WriteOptions {
            length_unit: LengthUnit::Metre,
        };
        step::write_with(&self.model, &targets, &options)
            .map_err(|e| KernelError::export(call, "write", e.to_string()))
    }

    /// `bodies` as a STEP product structure declaring metres (Arris's
    /// `write_products_with`, ADR-0032 §5): `tree` numbers its solids by
    /// their place in `bodies`, each body in its product's own frame.
    pub fn export_step_products(
        &mut self,
        bodies: &[KernelBody],
        tree: &ExportTree,
    ) -> Result<String, KernelError> {
        let call = self.begin(KernelOp::ExportProducts { tree: tree.clone() }, bodies)?;
        let mut targets = Vec::with_capacity(bodies.len());
        for &body in bodies {
            targets.push(self.entry(body)?.body);
        }
        let products = ProductTree {
            roots: tree.roots.iter().map(occurrence).collect(),
            faces: Vec::new(),
        };
        let options = WriteOptions {
            length_unit: LengthUnit::Metre,
        };
        self.controlled(|m, control| {
            step::write_products_with(m, &targets, &products, &options, control)
        })
        .map_err(|e| KernelError::export(call, "write", e.to_string()))
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

fn occurrence(node: &ExportNode) -> Occurrence {
    Occurrence {
        product: node.product,
        name: node.name.clone(),
        placement: Ok(convert::frame(&node.placement).as_isometry()),
        colour: node.colour.map(Rgb),
        solids: node.solids.iter().map(|&i| i as usize).collect(),
        children: node.children.iter().map(occurrence).collect(),
    }
}
