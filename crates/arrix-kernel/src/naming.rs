//! Persistent names from Arris's provenance (docs/DATA-MODEL.md
//! §Persistent naming). A sweep records every entity it makes as
//! `Generated` from a `Role` naming the part of the profile it came from,
//! by `(loop_index, segment)`; here that index becomes the curve's key, so
//! the name survives a re-ordered or re-drawn profile. A boolean keeps
//! its operands' names on what it leaves alone, and extends them with a
//! `mod` or `gen` step on what it splits or makes.

use std::collections::{BTreeMap, BTreeSet};

use arris::topo::provenance::SweepPart;
use arris::topo::{Body, EntityId, Model, Origin, Provenance, Role};
use arrix_core::{
    CurveKey, FeatureId, NameRoot, NameStep, PersistentName, Profile, SweepPartName, TopoKind,
};

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

    /// Every name with its entity, in the names' order.
    pub(crate) fn pairs(&self) -> impl Iterator<Item = (&PersistentName, EntityId)> {
        self.by_name.iter().map(|(name, &id)| (name, id))
    }

    pub(crate) fn from_map(by_name: BTreeMap<PersistentName, EntityId>) -> Self {
        Self { by_name }
    }
}

pub(crate) fn kind_of(id: EntityId) -> Option<TopoKind> {
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

/// Which sweep made a body: its names are rooted at `sweep.` or
/// `revolve.` and read the matching role of Arris's provenance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Sweep {
    Extrude,
    Revolve,
}

impl Sweep {
    fn root(self, feature: FeatureId, index: u32, part: SweepPartName) -> NameRoot {
        match self {
            Sweep::Extrude => NameRoot::Sweep {
                feature,
                index,
                part,
            },
            Sweep::Revolve => NameRoot::Revolve {
                feature,
                index,
                part,
            },
        }
    }
}

/// The names of a sweep's output. `Err` says what could not be placed:
/// an origin that is not the sweep's own role, an index the profile
/// lacks, an entity named twice or not at all, a name given twice.
pub(crate) fn sweep_names(
    m: &Model,
    body: Body,
    sweep: Sweep,
    (feature, index): (FeatureId, u32),
    profile: &Profile,
    provenance: &Provenance,
) -> Result<BodyNames, String> {
    let keys: Vec<Vec<CurveKey>> = profile.loops().map(|l| l.keys()).collect();
    let mut by_name = BTreeMap::new();
    let mut by_entity = BTreeMap::new();
    for origin in provenance.origins_recorded() {
        let part = match origin {
            Origin::Role(Role::Extrude(part)) if sweep == Sweep::Extrude => part,
            Origin::Role(Role::Revolve(part)) if sweep == Sweep::Revolve => part,
            // A file entity has no name root until C2's import gives it one.
            Origin::Role(Role::File(_)) => {
                return Err(format!("{origin} is a file entity, not yet nameable"));
            }
            // A consumer key is ours to give, and a sweep never does.
            Origin::Role(Role::Consumer(_)) => {
                return Err(format!("{origin} is a consumer entity, not a sweep's"));
            }
            _ => return Err(format!("{origin} is not {sweep:?}'s role")),
        };
        let generated = provenance.generated_from(origin);
        let Some(part) = sweep_part(part, &keys) else {
            if matches!(part, SweepPart::Body | SweepPart::Shell) {
                continue;
            }
            return Err(format!("{origin} names a curve the profile lacks"));
        };
        for shape in generated {
            // A void's shell (`Cavity`) is made by the role but has no name:
            // only its faces, edges and vertices do, from their own roles.
            if matches!(shape.id, EntityId::Shell(_) | EntityId::Body(_)) {
                continue;
            }
            let Some(kind) = kind_of(shape.id) else {
                return Err(format!("{origin} generated {shape}, which is not topology"));
            };
            let name = PersistentName {
                kind,
                root: sweep.root(feature, index, part),
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
    let entities = entities(m, body)?;
    for id in &entities {
        if !by_entity.contains_key(id) {
            return Err(format!("{id} has no name"));
        }
    }
    if by_entity.len() != entities.len() {
        return Err("a name was given to an entity outside the body".into());
    }
    Ok(BodyNames { by_name })
}

/// Every face, edge and vertex of `body`, each once.
pub(crate) fn entities(m: &Model, body: Body) -> Result<Vec<EntityId>, String> {
    let closure = m.closure(body).map_err(|e| e.to_string())?;
    Ok(closure
        .faces
        .iter()
        .map(|&f| EntityId::Face(f))
        .chain(closure.edges.iter().map(|&e| EntityId::Edge(e)))
        .chain(closure.vertices.iter().map(|&v| EntityId::Vertex(v)))
        .collect())
}

/// The names of a boolean's or a blend's output, from its operands'
/// names (one for a blend) and its provenance. An entity the boolean left alone keeps its name. Piece `0`
/// of an origin's split order (ADR-0009 in Arris) keeps the origin's
/// name (ADR-0010); a piece `k > 0` is that name with
/// `mod.<feature>.<boolean>.<k>` (the number not written when 0, ADR-0014). The target's origin comes first where
/// both operands give one (the flush case). An entity the boolean made
/// is `gen.<feature>.<boolean>[…]` of its origins' names, sorted, after the root
/// and chain of the first of them, since a name has a root. `Err` says
/// what could not be placed: an entity with no nameable origin, or two
/// entities with one name.
pub(crate) fn boolean_names(
    m: &Model,
    body: Body,
    (feature, boolean): (FeatureId, u32),
    operands: &[&BodyNames],
    provenance: &Provenance,
) -> Result<BodyNames, String> {
    // Every operand entity's name, the target's first.
    let mut old: BTreeMap<EntityId, (usize, &PersistentName)> = BTreeMap::new();
    for (i, names) in operands.iter().enumerate() {
        for (name, &id) in &names.by_name {
            if old.insert(id, (i, name)).is_some() {
                return Err(format!("{id} is in both operands"));
            }
        }
    }
    let mut pieces: BTreeMap<EntityId, Vec<(usize, PersistentName)>> = BTreeMap::new();
    let mut made: BTreeMap<EntityId, Vec<PersistentName>> = BTreeMap::new();
    for origin in provenance.origins_recorded() {
        // A body's and a shell's records have no name to extend.
        let Origin::Entity(from) = origin else {
            return Err(format!("{origin} is a role, not an operand's entity"));
        };
        let Some(&(operand, from_name)) = old.get(&from.id) else {
            continue;
        };
        for (k, shape) in provenance.modified_from(origin).iter().enumerate() {
            let mut name = from_name.clone();
            if k > 0 {
                name.chain.push(NameStep::Modified {
                    feature,
                    boolean,
                    split: u32::try_from(k).expect("under 2³² pieces"),
                });
            }
            pieces.entry(shape.id).or_default().push((operand, name));
        }
        for shape in provenance.generated_from(origin) {
            made.entry(shape.id).or_default().push(from_name.clone());
        }
    }
    let all = entities(m, body)?;
    let mut named: BTreeMap<EntityId, PersistentName> = BTreeMap::new();
    for &id in &all {
        let kind = kind_of(id).expect("a closure's faces, edges and vertices");
        let mut name = if let Some(list) = pieces.get(&id) {
            let first = list.iter().map(|(operand, _)| *operand).min();
            let mut from = list.iter().filter(|(operand, _)| Some(*operand) == first);
            match (from.next(), from.next()) {
                (Some((_, name)), None) => name.clone(),
                _ => return Err(format!("{id} is a piece of more than one entity")),
            }
        } else if let Some(from) = made.get(&id) {
            generated((feature, boolean), from.clone())
        } else if let Some((_, name)) = old.get(&id) {
            (*name).clone()
        } else {
            return Err(format!("{id} has no nameable origin"));
        };
        name.kind = kind;
        named.insert(id, name);
    }
    // Entities one operation made from the same origins (a blend's two
    // contact edges, its end vertices) share a name so far. Each takes the
    // names of what it touches, faces for an edge and edges for a vertex,
    // which the operation did not make from the same origins.
    let mut tally: BTreeMap<&PersistentName, usize> = BTreeMap::new();
    for name in named.values() {
        *tally.entry(name).or_default() += 1;
    }
    let clashing: BTreeSet<PersistentName> = tally
        .into_iter()
        .filter(|&(_, n)| n > 1)
        .map(|(name, _)| name.clone())
        .collect();
    if !clashing.is_empty() {
        let closure = m.closure(body).map_err(|e| e.to_string())?;
        for &id in &all {
            if !clashing.contains(&named[&id]) {
                continue;
            }
            let Some(origins) = made.get(&id) else {
                return Err(format!("{} names both {id} and another", named[&id]));
            };
            let touched: Vec<EntityId> = match id {
                EntityId::Edge(e) => m
                    .edge_uses(e)
                    .map_err(|e| e.to_string())?
                    .iter()
                    .filter(|u| closure.faces.contains(&u.face))
                    .map(|u| EntityId::Face(u.face))
                    .collect(),
                EntityId::Vertex(v) => m
                    .vertex_edges(v)
                    .map_err(|e| e.to_string())?
                    .iter()
                    .filter(|e| closure.edges.contains(e))
                    .map(|&e| EntityId::Edge(e))
                    .collect(),
                _ => Vec::new(),
            };
            let mut from = origins.clone();
            from.extend(touched.iter().map(|t| named[t].clone()));
            let mut name = generated((feature, boolean), from);
            name.kind = named[&id].kind;
            named.insert(id, name);
        }
    }
    let mut by_name = BTreeMap::new();
    for (id, name) in named {
        if let Some(other) = by_name.insert(name.clone(), id) {
            return Err(format!("{name} names both {other} and {id}"));
        }
    }
    Ok(BodyNames { by_name })
}

/// The names of a copy's or a mirror image's output: each entity is
/// `Modified` one-to-one from its operand's, and takes that name with
/// `inst.<feature>.<index>` (ADR-0010). `Err` says what could not be
/// placed: an origin that is not an entity of the operand, a split or a
/// merge, which a copy never makes, or two entities with one name.
pub(crate) fn instance_names(
    m: &Model,
    body: Body,
    (feature, index): (FeatureId, u32),
    operand: &BodyNames,
    provenance: &Provenance,
) -> Result<BodyNames, String> {
    let old: BTreeMap<EntityId, &PersistentName> = operand.pairs().map(|(n, id)| (id, n)).collect();
    let mut named: BTreeMap<EntityId, PersistentName> = BTreeMap::new();
    for origin in provenance.origins_recorded() {
        let Origin::Entity(from) = origin else {
            return Err(format!("{origin} is a role, not an operand's entity"));
        };
        let Some(&from_name) = old.get(&from.id) else {
            continue;
        };
        let to = provenance.modified_from(origin);
        let ([shape], true) = (to, provenance.generated_from(origin).is_empty()) else {
            return Err(format!("{origin} is not copied one-to-one"));
        };
        let mut name = from_name.clone();
        name.chain.push(NameStep::Instance { feature, index });
        if let Some(other) = named.insert(shape.id, name) {
            return Err(format!("{other} names {} twice", shape.id));
        }
    }
    let mut by_name = BTreeMap::new();
    for id in entities(m, body)? {
        let Some(name) = named.remove(&id) else {
            return Err(format!("{id} is no copy of an operand entity"));
        };
        if let Some(other) = by_name.insert(name.clone(), id) {
            return Err(format!("{name} names both {other} and {id}"));
        }
    }
    Ok(BodyNames { by_name })
}

/// `gen.<feature>[from]` after the root and chain of the first of `from`
/// sorted, since a name has a root.
fn generated(
    (feature, boolean): (FeatureId, u32),
    mut from: Vec<PersistentName>,
) -> PersistentName {
    from.sort();
    from.dedup();
    let mut name = from[0].clone();
    name.chain.push(NameStep::Generated {
        feature,
        boolean,
        from,
    });
    name
}
