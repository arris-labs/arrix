//! STEP import (ADR-0028, ADR-0029): one solid of a file read into the
//! model in metres, its entities named from the file's own (`Role::File`).

use std::collections::BTreeMap;
use std::sync::Arc;

use arris::io::step::{self, LengthUnit, ReadOptions};
use arris::topo::{Body, EntityId, Model, Origin, Provenance, Role};
use arrix_core::{FeatureId, NameRoot, NameStep, PersistentName};

use crate::error::kebab;
use crate::naming::{self, BodyNames};
use crate::record::{FileBytes, FileHash, FileSolid, ImportFrame};
use crate::structure::product_placement;
use crate::{Kernel, KernelBody, KernelError, KernelOp};

impl Kernel {
    /// Solid number `solid` (from 1, in the reader's order: by file entity,
    /// then placement) of the STEP file `file`, read in metres whatever
    /// unit the file declares, every face, edge and vertex named
    /// `import.<feature>.<entity>[.<instance>]` from the file entity it
    /// was read from. Text that is no exchange structure is
    /// `kernel.import.parse`; a solid the reader refuses is
    /// `kernel.import.<refusal kind>`, its message naming the file entity
    /// where the reader stopped; a `solid` past the file's count is
    /// `import.no-solid`. Every one is a recorded call, the record holding
    /// the file for its recipe.
    ///
    /// The bytes are read as UTF-8 with any invalid sequence replaced: a
    /// Part 21 file is ASCII outside its string literals, which only names
    /// and descriptions use.
    pub fn import_step(
        &mut self,
        feature: FeatureId,
        file: &[u8],
        solid: u32,
    ) -> Result<KernelBody, KernelError> {
        self.import_step_in(feature, file, solid, ImportFrame::World)
    }

    /// [`import_step`](Self::import_step) with the solid read in `frame`:
    /// `Product` moves it back by the inverse of its placement composed
    /// from the root (a solid the file's products do not hold is already
    /// there), keeping every name. A placement Arris does not read is
    /// `kernel.import.placement`.
    pub fn import_step_in(
        &mut self,
        feature: FeatureId,
        file: &[u8],
        solid: u32,
        frame: ImportFrame,
    ) -> Result<KernelBody, KernelError> {
        let hash = FileHash::of(file);
        let bytes = self
            .files
            .entry(hash.clone())
            .or_insert_with(|| Arc::from(file))
            .clone();
        let call = self.begin(
            KernelOp::ImportStep {
                feature,
                file: hash,
                solid,
                frame,
                entity: None,
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
        let count = read.solids.len();
        let index = solid.checked_sub(1).map(|i| i as usize);
        let Some(picked) = index.and_then(|i| read.solids.get(i)) else {
            self.release_read(&read);
            return Err(KernelError::NoSolid {
                call,
                asked: solid,
                count,
            });
        };
        if let KernelOp::ImportStep { entity, .. } = &mut self.records[call.0 as usize].op {
            *entity = Some(FileSolid {
                id: picked.entity.id,
                instance: picked.entity.instance,
            });
        }
        let picked_entity = picked.entity;
        let picked = picked.result.clone();
        // The file's other solids are not this feature's.
        let keep = picked.as_ref().ok().map(|r| r.body);
        self.release_read_but(&read, keep);
        let read_body = picked.map_err(|refusal| {
            KernelError::import(
                call,
                &kebab(&format!("{:?}", refusal.kind())),
                refusal.to_string(),
            )
        })?;
        if self.is_sheet(read_body.body) {
            self.release_read(&read);
            return Err(KernelError::import(
                call,
                "sheet",
                format!(
                    "{:?} is a surface model, not a solid; ArriX imports solids",
                    picked_entity
                ),
            ));
        }
        let names = import_names(&self.model, read_body.body, feature, &read_body.provenance)
            .map_err(|message| KernelError::Naming { call, message })?;
        let ImportFrame::Product = frame else {
            return Ok(self.push(call, read_body.body, names));
        };
        let moved = self.move_to_product(&read, index, &read_body, &names, call);
        let (moved, names) = match moved {
            Ok(moved) => moved,
            Err(e) => {
                self.release_read(&read);
                return Err(e);
            }
        };
        // The body as the reader placed it is not this feature's.
        self.release_read_but(&read, Some(moved));
        Ok(self.push(call, moved, names))
    }

    /// The picked solid moved into its product's frame, with its names.
    fn move_to_product(
        &mut self,
        read: &step::Read,
        index: Option<usize>,
        read_body: &step::ReadBody,
        names: &BodyNames,
        call: crate::CallIndex,
    ) -> Result<(Body, BodyNames), KernelError> {
        let world = product_placement(&read.products, index.expect("a solid was picked"))
            .map_err(|why| KernelError::import(call, "placement", why))?;
        let (moved, provenance) = self
            .controlled(|m, control| {
                arris::ops::transform(m, read_body.body, &world.inverse(), control)
            })
            .map_err(|e| KernelError::op(call, &e))?;
        let names = naming::moved_names(&self.model, moved, names, &provenance)
            .map_err(|message| KernelError::Naming { call, message })?;
        Ok((moved, names))
    }

    /// Whether the reader made `body` from a surface model (Arris 0.8).
    pub(crate) fn is_sheet(&self, body: Body) -> bool {
        self.model
            .body(body.id)
            .is_ok_and(|b| b.kind() == arris::topo::entity::BodyKind::Sheet)
    }

    /// Drops every body `read` put in the model.
    pub(crate) fn release_read(&mut self, read: &step::Read) {
        self.release_read_but(read, None);
    }

    /// Drops every body `read` put in the model but `keep`, with what only
    /// they used: the model keeps the live bodies and `keep`.
    fn release_read_but(&mut self, read: &step::Read, keep: Option<Body>) {
        let made = read.solids.iter().filter_map(|s| s.result.as_ref().ok());
        if made.clone().all(|r| Some(r.body) == keep) {
            return;
        }
        let mut live: Vec<Body> = self.bodies.iter().flatten().map(|e| e.body).collect();
        live.extend(keep);
        self.model
            .retain(&live)
            .expect("every live body is in the model");
    }
}

/// The names of a read body: each face, edge and vertex is
/// `import.<feature>.<entity>[.<instance>]` of the file entity Arris
/// generated it from. Where the reader made several entities of one kind
/// from one file entity (an edge a seamless band's seam splits, the
/// degenerate edges the file left out of one face), the first in the
/// reader's order keeps the name and piece `k` adds `mod.<feature>.<k>`,
/// as a boolean's pieces do. `Err` says what could not be placed: an
/// origin that is not a file entity, or an entity with no name.
pub(crate) fn import_names(
    m: &Model,
    body: Body,
    feature: FeatureId,
    provenance: &Provenance,
) -> Result<BodyNames, String> {
    let mut by_name = BTreeMap::new();
    let mut named = BTreeMap::new();
    for origin in provenance.origins_recorded() {
        let Origin::Role(Role::File(file)) = origin else {
            return Err(format!("{origin} is not a file entity"));
        };
        let mut pieces: BTreeMap<_, u32> = BTreeMap::new();
        for shape in provenance.generated_from(origin) {
            let Some(kind) = naming::kind_of(shape.id) else {
                continue;
            };
            let piece = pieces.entry(kind).or_default();
            let mut name = PersistentName {
                kind,
                root: NameRoot::Imported {
                    feature,
                    entity: file.id,
                    instance: file.instance,
                },
                chain: Vec::new(),
            };
            if *piece > 0 {
                name.chain.push(NameStep::Modified {
                    feature,
                    boolean: 0,
                    split: *piece,
                });
            }
            *piece += 1;
            if let Some(other) = named.insert(shape.id, name.clone()) {
                return Err(format!("{} is both {other} and {name}", shape.id));
            }
            if by_name.insert(name.clone(), shape.id).is_some() {
                return Err(format!("{name} names more than one entity"));
            }
        }
    }
    let entities: Vec<EntityId> = naming::entities(m, body)?;
    for id in &entities {
        if !named.contains_key(id) {
            return Err(format!("{id} has no name"));
        }
    }
    if named.len() != entities.len() {
        return Err("a name was given to an entity outside the body".into());
    }
    Ok(BodyNames::from_map(by_name))
}
