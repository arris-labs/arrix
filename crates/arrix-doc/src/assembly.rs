//! Assemblies: components that place a part or a nested assembly, grounded
//! by a pose or placed by a joint (docs/DATA-MODEL.md §Assemblies,
//! ADR-0029). The types here are stored data; solving them is the
//! evaluator's.

use std::collections::BTreeMap;

use arrix_core::{AssemblyId, ComponentId, JointId, PartId, PersistentName};
use serde::{Deserialize, Serialize};

use crate::expr::Expr;

/// An assembly: components, and the joints that place them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Assembly {
    pub id: AssemblyId,
    pub name: String,
    pub components: BTreeMap<ComponentId, Component>,
    pub joints: BTreeMap<JointId, Joint>,
}

/// One use of a part or of a nested assembly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: ComponentId,
    /// Unique within the assembly.
    pub name: String,
    pub of: ComponentOf,
    /// A pose grounds the component; a joint places it otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pose: Option<Pose>,
}

/// What a component places.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ComponentOf {
    Part(PartId),
    Assembly(AssemblyId),
}

/// A position (three lengths) and URDF's fixed-axis roll, pitch and yaw
/// (three angles), each an expression.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Pose {
    pub position: [Expr; 3],
    pub rpy: [Expr; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum JointKind {
    Fixed,
    Revolute,
    Prismatic,
}

/// A joint between two anchors: it places the child's component.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Joint {
    pub id: JointId,
    /// Unique within the assembly.
    pub name: String,
    pub kind: JointKind,
    pub parent: Anchor,
    pub child: Anchor,
    /// An angle for a revolute joint, a length for a prismatic one.
    pub value: Expr,
    pub offset: Expr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limits: Option<[Expr; 2]>,
    /// Turns the child's frame half a turn about its x.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// Where a joint attaches: a face or an edge of a part, by persistent name,
/// reached through the components from the joint's assembly down.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct Anchor {
    pub path: Vec<ComponentId>,
    pub axis: PersistentName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<PersistentName>,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use arrix_core::{FeatureId, Id};

    use super::*;
    use crate::document::{Document, FeatureRecord, FeatureTypeId, ImportedFile, Invalid, Part};
    use crate::frozen::BlobRef;
    use crate::open::{OpenError, open};
    use crate::{from_zip, save, to_zip};

    fn id<T: From<Id>>(n: u64) -> T {
        T::from(Id(n))
    }

    fn e(text: &str) -> Expr {
        Expr::parse(text).unwrap()
    }

    fn name(text: &str) -> PersistentName {
        text.parse().unwrap()
    }

    fn part(n: u64, file: Option<ImportedFile>) -> Part {
        let feature: FeatureId = id(n + 100);
        let record = FeatureRecord {
            id: feature,
            type_id: FeatureTypeId::new("core.import-step").unwrap(),
            type_version: 1,
            name: "servo".into(),
            params: BTreeMap::new(),
            choices: BTreeMap::new(),
            inputs: BTreeMap::new(),
            suppressed: false,
            sketch: None,
            frozen: None,
            file,
        };
        Part {
            id: id(n),
            name: format!("part {n}"),
            history: vec![feature],
            features: BTreeMap::from([(feature, record)]),
            rollback: None,
            material: None,
            body_materials: Default::default(),
        }
    }

    fn joint(n: u64, name_: &str, parent: u64, child: u64) -> Joint {
        let anchor = |c: u64| Anchor {
            path: vec![id(c)],
            axis: name("face:import.0000000000065.1"),
            origin: None,
        };
        Joint {
            id: id(n),
            name: name_.into(),
            kind: JointKind::Revolute,
            parent: anchor(parent),
            child: anchor(child),
            value: e("30 deg"),
            offset: e("0 deg"),
            limits: Some([e("-90 deg"), e("90 deg")]),
            flip: true,
        }
    }

    /// Two parts, an assembly of them grounded and jointed, and a second
    /// assembly nesting the first.
    fn document(blob: &BlobRef) -> Document {
        let file = ImportedFile {
            name: "servo.step".into(),
            blob: blob.clone(),
        };
        let component = |n: u64, of, pose| Component {
            id: id(n),
            name: format!("c{n}"),
            of,
            pose,
        };
        let pose = Pose {
            position: [e("1 mm"), e("2 mm"), e("3 mm")],
            rpy: [e("0 deg"), e("0 deg"), e("90 deg")],
        };
        let inner = Assembly {
            id: id(10),
            name: "inner".into(),
            components: BTreeMap::from([
                (id(1), component(1, ComponentOf::Part(id(1)), Some(pose))),
                (id(2), component(2, ComponentOf::Part(id(2)), None)),
            ]),
            joints: BTreeMap::from([(id(5), joint(5, "hinge", 1, 2))]),
        };
        let outer = Assembly {
            id: id(11),
            name: "outer".into(),
            components: BTreeMap::from([(
                id(3),
                component(3, ComponentOf::Assembly(id(10)), None),
            )]),
            joints: BTreeMap::new(),
        };
        Document {
            parts: BTreeMap::from([(id(1), part(1, None)), (id(2), part(2, Some(file)))]),
            assemblies: BTreeMap::from([(id(10), inner), (id(11), outer)]),
            blobs: BTreeMap::from([(blob.clone(), b"ISO-10303-21;".to_vec())]),
            ..Document::default()
        }
    }

    #[test]
    fn assemblies_and_an_imported_file_save_open_and_save_to_the_same_bytes() {
        let blob = BlobRef::of(b"ISO-10303-21;", "step");
        let doc = document(&blob);
        doc.validate().unwrap();
        let saved = save(&doc);
        assert!(saved.0.contains_key("assemblies/000000000000A.json"));
        assert_eq!(saved.0[&blob.path()], b"ISO-10303-21;");
        let opened = open(&saved).unwrap();
        assert_eq!(opened, doc);
        assert_eq!(save(&opened).0, saved.0);
        let zipped = to_zip(&saved).unwrap();
        assert_eq!(save(&open(&from_zip(&zipped).unwrap()).unwrap()).0, saved.0);
    }

    #[test]
    fn a_file_no_record_names_is_dropped_and_a_missing_one_is_named() {
        let blob = BlobRef::of(b"ISO-10303-21;", "step");
        let mut doc = document(&blob);
        doc.parts
            .get_mut(&id(2))
            .unwrap()
            .features
            .values_mut()
            .for_each(|r| r.file = None);
        assert!(!save(&doc).0.contains_key(&blob.path()));
        let mut saved = save(&document(&blob));
        saved.0.remove(&blob.path());
        let err = open(&saved).unwrap_err();
        assert_eq!(err.code(), Some("blob.missing"), "{err}");
    }

    #[test]
    fn validation_refuses_what_cannot_be_solved_or_named() {
        let blob = BlobRef::of(b"x", "step");
        let invalid = |edit: &dyn Fn(&mut Document)| {
            let mut doc = document(&blob);
            edit(&mut doc);
            doc.validate().unwrap_err()
        };
        let err = invalid(&|d| {
            d.assemblies.get_mut(&id(11)).unwrap().name = "inner".into();
        });
        assert!(matches!(err, Invalid::DuplicateAssembly(_)), "{err}");
        let err = invalid(&|d| {
            d.assemblies
                .get_mut(&id(10))
                .unwrap()
                .components
                .get_mut(&id(2))
                .unwrap()
                .name = "c1".into();
        });
        assert!(
            matches!(err, Invalid::DuplicateAssemblyName { .. }),
            "{err}"
        );
        let err = invalid(&|d| {
            d.parts.remove(&id(1));
        });
        assert!(matches!(err, Invalid::MissingTarget { .. }), "{err}");
        let err = invalid(&|d| {
            d.assemblies.remove(&id(10));
        });
        assert!(matches!(err, Invalid::MissingTarget { .. }), "{err}");
        let err = invalid(&|d| {
            d.assemblies
                .get_mut(&id(10))
                .unwrap()
                .joints
                .get_mut(&id(5))
                .unwrap()
                .child
                .path = vec![id(9)];
        });
        assert!(matches!(err, Invalid::MissingComponent { .. }), "{err}");
        let err = invalid(&|d| {
            d.assemblies.get_mut(&id(10)).unwrap().id = id(12);
        });
        assert!(matches!(err, Invalid::AssemblyIdMismatch { .. }), "{err}");
    }

    #[test]
    fn a_stray_assembly_file_is_named() {
        let mut saved = save(&Document::default());
        saved.0.insert("assemblies/notes.txt".into(), Vec::new());
        assert!(matches!(
            open(&saved).unwrap_err(),
            OpenError::Unexpected { path, .. } if path == "assemblies/notes.txt"
        ));
        let mut doc = document(&BlobRef::of(b"x", "step"));
        doc.blobs.clear();
        doc.parts
            .get_mut(&id(2))
            .unwrap()
            .features
            .values_mut()
            .for_each(|r| r.file = None);
        let mut saved = save(&doc);
        let text = saved.0.remove("assemblies/000000000000A.json").unwrap();
        saved.0.insert("assemblies/000000000000B.json".into(), text);
        assert!(matches!(
            open(&saved).unwrap_err(),
            OpenError::Unexpected { message, .. } if message.contains("000000000000A")
        ));
    }
}
