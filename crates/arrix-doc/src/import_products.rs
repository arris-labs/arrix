//! A STEP file's product tree as one command (ADR-0032 §2): a part per
//! product, a `core.import-step` feature per solid in the product's own
//! frame, an assembly per assembly product and a component per occurrence,
//! grounded at the file's placement. A product placed twice is one part
//! used twice; a colour becomes a material with no density. One `Group`,
//! so one undo removes it all.

use std::collections::{BTreeMap, BTreeSet};

use arrix_core::{
    AssemblyId, ComponentId, Diagnostic, FeatureId, Frame, IdMinter, MaterialId, PartId, Severity,
};
use arrix_kernel::{StepOccurrence, StepStructure};

use crate::assembly::{Assembly, Component, ComponentOf, Pose};
use crate::command::Command;
use crate::core_import::CoreImportStep;
use crate::document::{Document, FeatureRecord, FeatureTypeId, ImportedFile, Part};
use crate::expr::Expr;
use crate::frozen::BlobRef;
use crate::material::{Material, Rgb};

/// Why a file's tree cannot be imported.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    /// A placement the reader does not read, so a component cannot be put
    /// where the file puts it (`import.placement`).
    #[error("{0}: {1}")]
    Placement(String, String),
    /// Nothing in the file could be imported (`import.empty`).
    #[error("the file holds no solid ArriX can import")]
    Empty,
}

impl ImportError {
    /// The stable code of the failure.
    pub fn code(&self) -> &'static str {
        match self {
            ImportError::Placement(..) => "import.placement",
            ImportError::Empty => "import.empty",
        }
    }
}

/// An import ready to submit.
#[derive(Clone, Debug, PartialEq)]
pub struct Imported {
    /// The one `Group`.
    pub command: Command,
    /// What the file's roots became, in the file's order.
    pub roots: Vec<ComponentOf>,
    /// What was left out and why (`import.refused`), none of it silent.
    pub notes: Vec<Diagnostic>,
}

/// The `Group` that adds the file `bytes` named `file_name` and the
/// structure `structure` read from it, with ids from `ids`. Names are made
/// unique against `doc` and each other.
pub fn import_products(
    doc: &Document,
    file_name: &str,
    bytes: Vec<u8>,
    structure: &StepStructure,
    ids: &mut IdMinter,
) -> Result<Imported, ImportError> {
    let file = ImportedFile {
        name: file_name.to_owned(),
        blob: BlobRef::of(&bytes, "step"),
    };
    let mut b = Builder {
        doc,
        structure,
        file,
        ids,
        commands: Vec::new(),
        parts: BTreeMap::new(),
        assemblies: BTreeMap::new(),
        colours: BTreeMap::new(),
        used: Used::of(doc),
        notes: Vec::new(),
    };
    let mut roots = Vec::new();
    for root in &structure.roots {
        roots.extend(b.build(root)?);
    }
    if roots.is_empty() {
        return Err(ImportError::Empty);
    }
    let mut commands = vec![Command::AddFile { bytes }];
    commands.append(&mut b.commands);
    Ok(Imported {
        command: Command::Group {
            label: format!("Import {file_name}"),
            commands,
        },
        roots,
        notes: b.notes,
    })
}

/// The names taken, per kind.
struct Used {
    parts: BTreeSet<String>,
    assemblies: BTreeSet<String>,
    materials: BTreeSet<String>,
}

impl Used {
    fn of(doc: &Document) -> Self {
        Used {
            parts: doc.parts().values().map(|p| p.name.clone()).collect(),
            assemblies: doc.assemblies().values().map(|a| a.name.clone()).collect(),
            materials: doc.materials().values().map(|m| m.name.clone()).collect(),
        }
    }
}

/// `name`, or `name 2`, `name 3`… the first not in `taken`, which then
/// holds it.
fn unique(taken: &mut BTreeSet<String>, name: &str) -> String {
    let name = if name.is_empty() { "Part" } else { name };
    let mut candidate = name.to_owned();
    let mut n = 1;
    while !taken.insert(candidate.clone()) {
        n += 1;
        candidate = format!("{name} {n}");
    }
    candidate
}

struct Builder<'a> {
    doc: &'a Document,
    structure: &'a StepStructure,
    file: ImportedFile,
    ids: &'a mut IdMinter,
    commands: Vec<Command>,
    /// What each product became, so a second occurrence reuses it.
    parts: BTreeMap<u64, Option<ComponentOf>>,
    assemblies: BTreeMap<u64, Option<ComponentOf>>,
    colours: BTreeMap<[u64; 3], MaterialId>,
    used: Used,
    notes: Vec<Diagnostic>,
}

impl Builder<'_> {
    /// What `occ` becomes: an assembly when it holds children, a part when
    /// it holds solids, nothing when none of them can be imported.
    fn build(&mut self, occ: &StepOccurrence) -> Result<Option<ComponentOf>, ImportError> {
        if occ.children.is_empty() {
            return Ok(self.part_of(occ));
        }
        if let Some(done) = occ.product.and_then(|p| self.assemblies.get(&p)) {
            return Ok(*done);
        }
        let mut components = BTreeMap::new();
        let mut names = BTreeSet::new();
        // The occurrence's own solids sit in the assembly's origin.
        let own = StepOccurrence {
            children: Vec::new(),
            product: None,
            ..occ.clone()
        };
        if let Some(of) = self.part_of(&own) {
            let id: ComponentId = self.ids.mint();
            let name = unique(&mut names, &occ.name);
            let pose = pose_of(&Frame::WORLD_XY);
            components.insert(id, component(id, name, of, pose));
        }
        for child in &occ.children {
            let Some(of) = self.build(child)? else {
                continue;
            };
            let frame = child
                .placement
                .as_ref()
                .map_err(|why| ImportError::Placement(child.name.clone(), why.clone()))?;
            let id: ComponentId = self.ids.mint();
            let name = unique(&mut names, &child.name);
            components.insert(id, component(id, name, of, pose_of(frame)));
        }
        let result = if components.is_empty() {
            None
        } else {
            let id: AssemblyId = self.ids.mint();
            let name = unique(&mut self.used.assemblies, &occ.name);
            self.commands.push(Command::AddAssembly {
                assembly: Assembly {
                    id,
                    name,
                    components,
                    joints: BTreeMap::new(),
                },
            });
            Some(ComponentOf::Assembly(id))
        };
        if let Some(p) = occ.product {
            self.assemblies.insert(p, result);
        }
        Ok(result)
    }

    /// The part `occ`'s solids make, or none when it holds no solid the
    /// reader accepted.
    fn part_of(&mut self, occ: &StepOccurrence) -> Option<ComponentOf> {
        if let Some(done) = occ.product.and_then(|p| self.parts.get(&p)) {
            return *done;
        }
        let mut solids = Vec::new();
        for &number in &occ.solids {
            match &self.structure.solids[number as usize - 1].refused {
                None => solids.push(number),
                Some(why) => self.notes.push(
                    Diagnostic::new(
                        Severity::Warning,
                        "import.refused",
                        format!("{}: solid {number} left out: {why}", occ.name),
                    )
                    .expect("a literal code is well-formed"),
                ),
            }
        }
        let result = (!solids.is_empty()).then(|| self.add_part(occ, &solids));
        if let Some(p) = occ.product {
            self.parts.insert(p, result);
        }
        result
    }

    fn add_part(&mut self, occ: &StepOccurrence, solids: &[u32]) -> ComponentOf {
        let id: PartId = self.ids.mint();
        let name = unique(&mut self.used.parts, &occ.name);
        let mut part = Part {
            id,
            name: name.clone(),
            history: Vec::new(),
            features: BTreeMap::new(),
            rollback: None,
            material: occ.colour.map(|c| self.material_of(c)),
            body_materials: BTreeMap::new(),
        };
        for (i, &number) in solids.iter().enumerate() {
            let feature: FeatureId = self.ids.mint();
            let title = match solids.len() {
                1 => name.clone(),
                _ => format!("{name} {}", i + 1),
            };
            part.history.push(feature);
            part.features
                .insert(feature, self.record(feature, title, number));
        }
        self.commands.push(Command::AddPart { part });
        ComponentOf::Part(id)
    }

    fn record(&self, id: FeatureId, name: String, solid: u32) -> FeatureRecord {
        FeatureRecord {
            id,
            type_id: FeatureTypeId::new("core.import-step").expect("a literal type id"),
            type_version: CoreImportStep::VERSION,
            name,
            params: BTreeMap::from([(
                "solid".to_owned(),
                Expr::parse(&solid.to_string()).expect("a count parses"),
            )]),
            choices: BTreeMap::from([("frame".to_owned(), "product".to_owned())]),
            inputs: BTreeMap::new(),
            suppressed: false,
            sketch: None,
            frozen: None,
            file: Some(self.file.clone()),
        }
    }

    /// The material a colour is: one per distinct colour, named by its hex
    /// code, reusing an equal one the document already holds.
    fn material_of(&mut self, colour: [f64; 3]) -> MaterialId {
        let key = colour.map(f64::to_bits);
        if let Some(id) = self.colours.get(&key) {
            return *id;
        }
        let hex: String = colour
            .iter()
            .map(|c| format!("{:02x}", (c * 255.0).round().clamp(0.0, 255.0) as u8))
            .collect();
        let name = format!("Colour #{hex}");
        let existing = self
            .doc
            .materials()
            .values()
            .find(|m| m.name == name && m.appearance == Rgb(colour));
        let id = match existing {
            Some(m) => m.id,
            None => {
                let id: MaterialId = self.ids.mint();
                let name = unique(&mut self.used.materials, &name);
                self.commands.insert(
                    0,
                    Command::AddMaterial {
                        material: Material {
                            id,
                            name,
                            density: None,
                            appearance: Rgb(colour),
                        },
                    },
                );
                id
            }
        };
        self.colours.insert(key, id);
        id
    }
}

fn component(id: ComponentId, name: String, of: ComponentOf, pose: Pose) -> Component {
    Component {
        id,
        name,
        of,
        pose: Some(pose),
    }
}

/// A frame as a pose: the origin in metres and URDF's fixed-axis roll,
/// pitch and yaw in radians, `R = Rz(yaw) Ry(pitch) Rx(roll)`.
fn pose_of(frame: &Frame) -> Pose {
    let o = frame.origin();
    let (x, y, z) = (frame.x_axis(), frame.y_axis(), frame.z_axis());
    // R's columns are the frame's axes: R[row][col] = axis[col][row].
    let pitch = (-x.z).clamp(-1.0, 1.0).asin();
    let (roll, yaw) = if x.z.abs() < 1.0 - 1e-12 {
        (y.z.atan2(z.z), x.y.atan2(x.x))
    } else {
        // Gimbal lock: roll and yaw turn the same axis; put it all in yaw.
        (0.0, (-y.x).atan2(y.y))
    };
    let length = |v: f64| Expr::parse(&format!("{v:?} m")).expect("a number with a unit parses");
    let angle = |v: f64| Expr::parse(&format!("{v:?} rad")).expect("a number with a unit parses");
    Pose {
        position: [length(o.x), length(o.y), length(o.z)],
        rpy: [angle(roll), angle(pitch), angle(yaw)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrix_core::DVec3;

    fn rpy(pose: &Pose) -> [f64; 3] {
        let v = |e: &Expr| {
            let text = e.to_string();
            text.split_whitespace().next().unwrap().parse().unwrap()
        };
        [v(&pose.rpy[0]), v(&pose.rpy[1]), v(&pose.rpy[2])]
    }

    /// The pose a frame gives, turned back into a frame the way forward
    /// kinematics does, is that frame.
    #[test]
    fn a_frame_round_trips_through_roll_pitch_and_yaw() {
        let turns = [
            (DVec3::X, DVec3::Z),
            (DVec3::Y, DVec3::Z),
            (DVec3::new(1.0, 1.0, 0.0), DVec3::new(-1.0, 1.0, 1.0)),
            (DVec3::new(0.3, -0.5, 0.8), DVec3::new(0.5, 0.7, 0.1)),
            // Pitch at a quarter turn: gimbal lock.
            (DVec3::Z, DVec3::X),
            (DVec3::Z, DVec3::new(0.0, 1.0, 0.0)),
            (DVec3::new(0.0, 0.0, -1.0), DVec3::new(1.0, 0.0, 0.0)),
        ];
        for (x, z) in turns {
            // Make z orthogonal to x by Gram-Schmidt.
            let z = z - x.normalize() * z.dot(x.normalize());
            let frame = Frame::new(DVec3::new(0.1, -0.2, 0.3), x, z).unwrap();
            let [roll, pitch, yaw] = rpy(&pose_of(&frame));
            let r = arrix_core::DMat3::from_rotation_z(yaw)
                * arrix_core::DMat3::from_rotation_y(pitch)
                * arrix_core::DMat3::from_rotation_x(roll);
            for (got, want) in [
                (r.col(0), frame.x_axis()),
                (r.col(1), frame.y_axis()),
                (r.col(2), frame.z_axis()),
            ] {
                assert!(
                    (got - want).length() < 1e-12,
                    "{frame:?}: {got:?} != {want:?}"
                );
            }
        }
    }
}
