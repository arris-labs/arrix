//! Persistent names from Arris's provenance (docs/DATA-MODEL.md
//! §Persistent naming). A sweep records every entity it makes as
//! `Generated` from a `Role` naming the part of the profile it came from,
//! by `(loop_index, segment)`; here that index becomes the curve's key, so
//! the name survives a re-ordered or re-drawn profile.

use std::collections::BTreeMap;

use arris::topo::provenance::SweepPart;
use arris::topo::{Body, EntityId, Model, Origin, Provenance, Role};
use arrix_core::{CurveKey, FeatureId, NameRoot, PersistentName, Profile, SweepPartName, TopoKind};

/// Every face, edge and vertex of one body, by name. Each entity has
/// exactly one name and each name exactly one entity; a result that
/// breaks either is refused where it is built.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BodyNames {
    by_name: BTreeMap<PersistentName, EntityId>,
}

impl BodyNames {
    /// Every name, in the names' order.
    pub fn iter(&self) -> impl Iterator<Item = &PersistentName> {
        self.by_name.keys()
    }

    /// The names of one kind, in order.
    pub fn of_kind(&self, kind: TopoKind) -> impl Iterator<Item = &PersistentName> {
        self.iter().filter(move |n| n.kind == kind)
    }

    pub fn count(&self, kind: TopoKind) -> usize {
        self.of_kind(kind).count()
    }

    pub fn contains(&self, name: &PersistentName) -> bool {
        self.by_name.contains_key(name)
    }

    pub(crate) fn entity(&self, name: &PersistentName) -> Option<EntityId> {
        self.by_name.get(name).copied()
    }
}

fn kind_of(id: EntityId) -> Option<TopoKind> {
    match id {
        EntityId::Face(_) => Some(TopoKind::Face),
        EntityId::Edge(_) => Some(TopoKind::Edge),
        EntityId::Vertex(_) => Some(TopoKind::Vertex),
        EntityId::Shell(_) | EntityId::Body(_) => None,
    }
}

/// The part of `feature`'s sweep that Arris's `part` is, in curve keys;
/// `None` for the body and its shells, which are not named, and for an
/// index the profile does not have.
fn sweep_part(part: SweepPart, keys: &[Vec<CurveKey>]) -> Option<SweepPartName> {
    let key = |l: usize, s: usize| keys.get(l)?.get(s).copied();
    Some(match part {
        SweepPart::Body | SweepPart::Shell => return None,
        SweepPart::StartCap => SweepPartName::StartCap,
        SweepPart::EndCap => SweepPartName::EndCap,
        SweepPart::Side {
            loop_index,
            segment,
        } => SweepPartName::Side(key(loop_index, segment)?),
        SweepPart::StartEdge {
            loop_index,
            segment,
        } => SweepPartName::StartEdge(key(loop_index, segment)?),
        SweepPart::EndEdge {
            loop_index,
            segment,
        } => SweepPartName::EndEdge(key(loop_index, segment)?),
        SweepPart::Rise { loop_index, vertex } => SweepPartName::Rise(key(loop_index, vertex)?),
        SweepPart::StartVertex { loop_index, vertex } => {
            SweepPartName::StartVertex(key(loop_index, vertex)?)
        }
        SweepPart::EndVertex { loop_index, vertex } => {
            SweepPartName::EndVertex(key(loop_index, vertex)?)
        }
        SweepPart::Cavity {
            loop_index,
            segment,
        } => SweepPartName::Cavity(key(loop_index, segment)?),
    })
}

/// The names of an extrude's output. `Err` says what could not be placed:
/// an origin that is not the extrude's own role, an index the profile
/// lacks, an entity named twice or not at all, a name given twice.
pub(crate) fn extrude_names(
    m: &Model,
    body: Body,
    feature: FeatureId,
    profile: &Profile,
    provenance: &Provenance,
) -> Result<BodyNames, String> {
    let keys: Vec<Vec<CurveKey>> = profile.loops().map(|l| l.keys()).collect();
    let mut by_name = BTreeMap::new();
    let mut by_entity = BTreeMap::new();
    for origin in provenance.origins_recorded() {
        let Origin::Role(Role::Extrude(part)) = origin else {
            return Err(format!("{origin} is not an extrude's role"));
        };
        let generated = provenance.generated_from(origin);
        let Some(part) = sweep_part(part, &keys) else {
            if matches!(part, SweepPart::Body | SweepPart::Shell) {
                continue;
            }
            return Err(format!("{origin} names a curve the profile lacks"));
        };
        for shape in generated {
            let Some(kind) = kind_of(shape.id) else {
                return Err(format!("{origin} generated {shape}, which is not topology"));
            };
            let name = PersistentName {
                kind,
                root: NameRoot::Sweep { feature, part },
                chain: Vec::new(),
            };
            if let Some(other) = by_entity.insert(shape.id, name.clone()) {
                return Err(format!("{shape} is both {other} and {name}"));
            }
            if by_name.insert(name.clone(), shape.id).is_some() {
                return Err(format!("{name} names more than one entity"));
            }
        }
    }
    let closure = m.closure(body).map_err(|e| e.to_string())?;
    let entities = closure
        .faces
        .iter()
        .map(|&f| EntityId::Face(f))
        .chain(closure.edges.iter().map(|&e| EntityId::Edge(e)))
        .chain(closure.vertices.iter().map(|&v| EntityId::Vertex(v)));
    for id in entities {
        if !by_entity.contains_key(&id) {
            return Err(format!("{id} has no name"));
        }
    }
    if by_entity.len() != closure.faces.len() + closure.edges.len() + closure.vertices.len() {
        return Err("a name was given to an entity outside the body".into());
    }
    Ok(BodyNames { by_name })
}
