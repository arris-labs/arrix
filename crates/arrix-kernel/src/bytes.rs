//! A body as Arris body bytes and the table of its names (docs/DATA-MODEL.md
//! §Frozen results). The bytes are canonical: the body is copied into a
//! fresh model first, so they hold nothing of the long-lived model's ids and
//! the same body gives the same bytes in any evaluation. The table names
//! each entity by its index in that copy, which is the written body's.

use std::collections::{BTreeMap, BTreeSet};

use arris::Control;
use arris::io::body::{self, BodyError};
use arris::topo::{Body, EntityId, IdMap, Model, Provenance};
use arrix_core::{PersistentName, TopoKind};
use serde::{Deserialize, Serialize};

use crate::naming::{BodyNames, entities, kind_of};

/// One body's bytes, and what names its entities.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BodyBytes {
    pub bytes: Vec<u8>,
    pub names: NameTable,
}

/// Every name of a body with the index of its entity in the written body,
/// in the names' order. The name carries the entity's kind.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NameTable(pub Vec<NameEntry>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NameEntry {
    pub name: PersistentName,
    pub index: u32,
}

fn index_of(id: EntityId) -> Option<u32> {
    match id {
        EntityId::Vertex(v) => Some(v.index()),
        EntityId::Edge(e) => Some(e.index()),
        EntityId::Face(f) => Some(f.index()),
        EntityId::Shell(_) | EntityId::Body(_) => None,
    }
}

/// `body` of `model` as canonical bytes, with `names` re-indexed into them.
pub(crate) fn encode(model: &Model, body: Body, names: &BodyNames) -> Result<BodyBytes, BodyError> {
    let mut fresh = Model::new(model.precision())?;
    let (copy, map) = fresh.import(model, body)?;
    let bytes = body::write(&fresh, copy, &Provenance::default())?;
    let mut table = Vec::new();
    for (name, id) in names.pairs() {
        let dense = match id {
            EntityId::Vertex(v) => map.vertices.get(&v).copied().map(EntityId::Vertex),
            EntityId::Edge(e) => map.edges.get(&e).copied().map(EntityId::Edge),
            EntityId::Face(f) => map.faces.get(&f).copied().map(EntityId::Face),
            EntityId::Shell(_) | EntityId::Body(_) => None,
        };
        let index = dense
            .and_then(index_of)
            .ok_or_else(|| BodyError::Encode(format!("{name} has no entity in the copy")))?;
        table.push(NameEntry {
            name: name.clone(),
            index,
        });
    }
    Ok(BodyBytes {
        bytes,
        names: NameTable(table),
    })
}

/// `bytes` read into `model`, named by `table`. `Err` on the second part
/// says what in the table does not fit the body.
pub(crate) fn decode(
    model: &mut Model,
    bytes: &[u8],
    table: &NameTable,
) -> Result<(Body, Result<BodyNames, String>), BodyError> {
    let imported = body::read(model, bytes, &Control::NONE)?;
    let names = place(model, imported.body, &imported.map, table);
    Ok((imported.body, names))
}

/// The entities the writer's `index` of each kind became, from the read's
/// map.
fn by_index(map: &IdMap) -> BTreeMap<(TopoKind, u32), EntityId> {
    let mut found = BTreeMap::new();
    for (from, &to) in &map.vertices {
        found.insert((TopoKind::Vertex, from.index()), EntityId::Vertex(to));
    }
    for (from, &to) in &map.edges {
        found.insert((TopoKind::Edge, from.index()), EntityId::Edge(to));
    }
    for (from, &to) in &map.faces {
        found.insert((TopoKind::Face, from.index()), EntityId::Face(to));
    }
    found
}

/// The table as names of the body just read: every entry one entity of its
/// kind, every entity of the body one entry, no name or entity twice.
fn place(model: &Model, body: Body, map: &IdMap, table: &NameTable) -> Result<BodyNames, String> {
    let found = by_index(map);
    let mut by_name = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for NameEntry { name, index } in &table.0 {
        let id = *found
            .get(&(name.kind, *index))
            .ok_or_else(|| format!("{name} names {:?} {index}, which the body lacks", name.kind))?;
        if kind_of(id) != Some(name.kind) {
            return Err(format!("{name} is a {:?}, not what {id} is", name.kind));
        }
        if !seen.insert(id) {
            return Err(format!("{id} has more than one name"));
        }
        if by_name.insert(name.clone(), id).is_some() {
            return Err(format!("{name} is in the table twice"));
        }
    }
    let all = entities(model, body)?;
    if let Some(id) = all.iter().find(|id| !seen.contains(id)) {
        return Err(format!("{id} has no name in the table"));
    }
    if seen.len() != all.len() {
        return Err("the table names an entity outside the body".into());
    }
    Ok(BodyNames::from_map(by_name))
}
