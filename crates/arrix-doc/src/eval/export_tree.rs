//! An assembly as a STEP product tree (ADR-0032 §5): each part a product
//! written once, each component an occurrence at its solved pose in its
//! parent assembly, each nested assembly a sub-assembly product, a part's
//! final bodies its solids and its material's appearance their colour.
//! The tree is plain data; the kernel writes it in metres.

use std::collections::BTreeMap;

use arrix_core::{AssemblyId, Diagnostic, Frame, PartId};
use arrix_kernel::{ExportNode, ExportTree, KernelBody};

use super::export::error;
use super::{Evaluator, host};
use crate::assembly::ComponentOf;
use crate::document::Document;

/// What the tree walk collects: the bodies written, once each, and where a
/// part's are among them.
struct Walk<'a> {
    doc: &'a Document,
    evaluator: &'a Evaluator,
    bodies: Vec<KernelBody>,
    of_part: BTreeMap<PartId, Vec<u32>>,
    /// The product numbers handed out: a part or an assembly is one
    /// product however many components use it.
    products: BTreeMap<Product, u64>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Product {
    Part(PartId),
    Assembly(AssemblyId),
}

impl Evaluator {
    /// The latest evaluation's assembly `id` as a STEP file with its
    /// structure, in metres. `export.unknown-assembly` names an id the
    /// document lacks, `export.unplaced` a component the solve could not
    /// place (a file with a guessed pose would be a silent lie), and
    /// `export.no-bodies` an assembly with no body to write; the kernel's
    /// refusals are `kernel.export.*`.
    pub fn export_assembly(
        &mut self,
        doc: &Document,
        id: AssemblyId,
    ) -> Result<Vec<u8>, Diagnostic> {
        if !doc.assemblies().contains_key(&id) {
            return Err(error(
                "export.unknown-assembly",
                format!("{id} is not an assembly of the document"),
            ));
        }
        let mut walk = Walk {
            doc,
            evaluator: self,
            bodies: Vec::new(),
            of_part: BTreeMap::new(),
            products: BTreeMap::new(),
        };
        let product = walk.product(Product::Assembly(id));
        let children = walk.children(id)?;
        let bodies = walk.bodies;
        if bodies.is_empty() {
            return Err(error(
                "export.no-bodies",
                "the assembly has no body to export".into(),
            ));
        }
        let tree = ExportTree {
            roots: vec![ExportNode {
                product: Some(product),
                name: doc.assemblies()[&id].name.clone(),
                placement: Frame::WORLD_XY,
                colour: None,
                solids: Vec::new(),
                children,
            }],
        };
        self.kernel
            .export_step_products(&bodies, &tree)
            .map(String::into_bytes)
            .map_err(|e| host::kernel_diagnostic(&e))
    }
}

impl Walk<'_> {
    fn product(&mut self, of: Product) -> u64 {
        let next = self.products.len() as u64;
        *self.products.entry(of).or_insert(next)
    }

    /// The occurrences of `assembly`'s components, at their poses in it.
    fn children(&mut self, assembly: AssemblyId) -> Result<Vec<ExportNode>, Diagnostic> {
        let doc = self.doc;
        let solved = self.evaluator.placed.get(&assembly);
        let mut out = Vec::new();
        for (cid, component) in &doc.assemblies()[&assembly].components {
            let placed = solved
                .and_then(|s| s.components.iter().find(|c| c.path == [*cid]))
                .filter(|c| {
                    !c.diagnostics
                        .iter()
                        .any(|d| d.code.as_str() == "assembly.unplaced")
                })
                .ok_or_else(|| {
                    error(
                        "export.unplaced",
                        format!("component {:?} is not placed", component.name),
                    )
                })?;
            out.push(match component.of {
                ComponentOf::Part(part) => self.part(part, placed.pose),
                ComponentOf::Assembly(nested) => ExportNode {
                    product: Some(self.product(Product::Assembly(nested))),
                    name: doc.assemblies()[&nested].name.clone(),
                    placement: placed.pose,
                    colour: None,
                    solids: Vec::new(),
                    children: self.children(nested)?,
                },
            });
        }
        Ok(out)
    }

    fn part(&mut self, id: PartId, pose: Frame) -> ExportNode {
        if !self.of_part.contains_key(&id) {
            let mut numbers = Vec::new();
            for body in self.evaluator.finals.iter().filter(|b| b.part == id) {
                numbers.push(u32::try_from(self.bodies.len()).expect("under 2³² bodies"));
                self.bodies.push(body.body);
            }
            self.of_part.insert(id, numbers);
        }
        let part = &self.doc.parts()[&id];
        let solids = self.of_part[&id].clone();
        let colour = part
            .material
            .and_then(|m| self.doc.materials().get(&m))
            .filter(|_| !solids.is_empty())
            .map(|m| m.appearance.0);
        ExportNode {
            product: Some(self.product(Product::Part(id))),
            name: part.name.clone(),
            placement: pose,
            colour,
            solids,
            children: Vec::new(),
        }
    }
}
