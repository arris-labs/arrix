//! What a tab looks at (ADR-0030 §1): one part, drawn at its own origin, or
//! one assembly, drawn as its components are placed. Session state, never
//! the document's.

use arrix_core::{AssemblyId, ComponentId, PartId};
use arrix_doc::{ComponentOf, Document, Replica};
use arrix_viewport::{BodyKey, Instance};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Part(PartId),
    Assembly(AssemblyId),
}

impl Context {
    /// Where a document opens: its first root assembly (one no assembly
    /// contains, by id), else its first part.
    pub fn opening(doc: &Document) -> Option<Self> {
        let nested: Vec<AssemblyId> = doc
            .assemblies()
            .values()
            .flat_map(|a| a.components.values())
            .filter_map(|c| match c.of {
                ComponentOf::Assembly(a) => Some(a),
                ComponentOf::Part(_) => None,
            })
            .collect();
        doc.assemblies()
            .keys()
            .find(|id| !nested.contains(id))
            .map(|id| Self::Assembly(*id))
            .or_else(|| doc.parts().keys().next().map(|id| Self::Part(*id)))
    }

    /// Whether the document still holds what this looks at.
    pub fn exists(self, doc: &Document) -> bool {
        match self {
            Self::Part(id) => doc.parts().contains_key(&id),
            Self::Assembly(id) => doc.assemblies().contains_key(&id),
        }
    }

    /// The name and the id the user and a render's `--context` call it by.
    pub fn names(self, doc: &Document) -> Option<(String, String)> {
        match self {
            Self::Part(id) => doc
                .parts()
                .get(&id)
                .map(|p| (p.name.clone(), id.0.to_string())),
            Self::Assembly(id) => doc
                .assemblies()
                .get(&id)
                .map(|a| (a.name.clone(), id.0.to_string())),
        }
    }

    /// The part or assembly `spec` names, by id or by name; parts first.
    pub fn parse(doc: &Document, spec: &str) -> Result<Self, String> {
        let all = doc
            .parts()
            .keys()
            .map(|id| Self::Part(*id))
            .chain(doc.assemblies().keys().map(|id| Self::Assembly(*id)));
        let mut found = all.filter(|c| {
            c.names(doc)
                .is_some_and(|(name, id)| name == spec || id == spec)
        });
        match (found.next(), found.next()) {
            (Some(one), None) => Ok(one),
            (None, _) => Err(format!("no part or assembly is `{spec}`")),
            (Some(_), Some(_)) => Err(format!("more than one part or assembly is `{spec}`")),
        }
    }
}

/// The part the component at `path` places, following nested assemblies
/// from `root`; `None` where the path leads to an assembly or nowhere.
fn leaf_part(doc: &Document, root: AssemblyId, path: &[ComponentId]) -> Option<PartId> {
    let mut assembly = root;
    let mut leaf = None;
    for id in path {
        let component = doc.assemblies().get(&assembly)?.components.get(id)?;
        match component.of {
            ComponentOf::Part(p) => leaf = Some(p),
            ComponentOf::Assembly(a) => {
                assembly = a;
                leaf = None;
            }
        }
    }
    leaf
}

/// The names of the components along `path`, following nested assemblies
/// from `root`; it stops where the path leads nowhere.
pub fn component_names(doc: &Document, root: AssemblyId, path: &[ComponentId]) -> Vec<String> {
    let mut assembly = root;
    let mut names = Vec::new();
    for id in path {
        let Some(component) = doc
            .assemblies()
            .get(&assembly)
            .and_then(|a| a.components.get(id))
        else {
            break;
        };
        names.push(component.name.clone());
        if let ComponentOf::Assembly(a) = component.of {
            assembly = a;
        }
    }
    names
}

/// Every placed component of `root` that places a part, once per body of
/// that part the scene holds, at the pose the last `Assembled` gave it
/// (nested ones composed, `assembly.unplaced` ones at the identity).
pub fn instances(replica: &Replica, root: AssemblyId, keys: &[BodyKey]) -> Vec<Instance> {
    let Some((_, outcome)) = replica.assembly(root) else {
        return Vec::new();
    };
    let doc = replica.document();
    let mut out = Vec::new();
    for placed in &outcome.components {
        let Some(part) = leaf_part(doc, root, &placed.path) else {
            continue;
        };
        out.extend(keys.iter().filter(|k| k.part == part).map(|key| Instance {
            path: placed.path.clone(),
            key: key.clone(),
            pose: placed.pose,
        }));
    }
    out
}
