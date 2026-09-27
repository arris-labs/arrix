//! Feature types by id (docs/DATA-MODEL.md §Features). Built-ins register
//! under `core.` through the same shape a plugin's feature is adapted onto,
//! so the evaluator has one path for both. A record whose type is not here
//! is not an error: it evaluates frozen.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_core::{Diagnostic, FeatureId, SlotName};
use arrix_plugin_api::{
    FeatureOutput, FeatureTypeSpec, InputKind, Kernel, ParamValue, ResolvedInput, SlotKind,
};
use arrix_sketch::Sketch;

use crate::core_types::{CoreSketch, DatumPlane};
use crate::document::{FeatureTypeId, InvalidFeatureTypeId};
use crate::eval::SketchView;

/// A feature type as the evaluator sees it, built-in or plugin: the plugin
/// API's `Feature` for one type, so a Tier 0 plugin is adapted onto it
/// without translation.
pub trait FeatureType: Send + Sync {
    /// Its form, inputs and output slots; `spec().id` is its id.
    fn spec(&self) -> &FeatureTypeSpec;

    /// The outputs of one feature of this type. Deterministic: a pure
    /// function of `args`, the kernel's results and the type's version.
    /// Everything built through `kernel` is named as the evaluating
    /// feature's.
    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        args: &FeatureArgs,
    ) -> Result<TypeOutput, Diagnostic>;

    /// Slots of a kind the plugin API has no word for, which only a
    /// built-in declares and fills: a sketch's. Its spec's `outputs` are
    /// the rest.
    fn sketch_slots(&self) -> &[SlotName] {
        &[]
    }

    /// The contributing plugin's own version, `None` for a built-in. It is
    /// part of every input hash, so upgrading a plugin re-evaluates its
    /// features (docs/PLUGINS.md §Versioning).
    fn plugin_version(&self) -> Option<&str> {
        None
    }
}

/// What the evaluator resolved for one feature: every parameter of the
/// type's form in SI (the record's expression or the form's default), its
/// plain choices as written, the inputs the record gives, in the form's
/// order, and a `core.sketch` record's sketch.
#[derive(Debug, Default, PartialEq)]
pub struct FeatureArgs {
    pub params: Vec<ParamValue>,
    pub choices: BTreeMap<String, String>,
    pub inputs: Vec<ResolvedInput>,
    pub sketch: Option<SketchArgs>,
}

/// A record's sketch with every driving dimension's expression resolved
/// into its value in SI, as the solve reads it, and the feature it
/// belongs to, which what it names is named under.
#[derive(Clone, Debug, PartialEq)]
pub struct SketchArgs {
    pub feature: FeatureId,
    pub sketch: Sketch,
}

/// What a feature type's `evaluate` returns: the plugin API's output, and
/// the slots only a built-in fills, by the name `sketch_slots` declares.
#[derive(Debug, Default, PartialEq)]
pub struct TypeOutput {
    pub output: FeatureOutput,
    pub sketches: Vec<(SlotName, SketchView)>,
}

impl From<FeatureOutput> for TypeOutput {
    fn from(output: FeatureOutput) -> Self {
        Self {
            output,
            sketches: Vec::new(),
        }
    }
}

impl FeatureArgs {
    /// The parameter `name` in SI. The evaluator gives every one the form
    /// lists, so a type asks only for its own.
    pub fn param(&self, name: &str) -> Option<f64> {
        self.params
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.value.si)
    }

    pub fn input(&self, name: &str) -> Option<&ResolvedInput> {
        self.inputs.iter().find(|i| i.name == name)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RegistryError {
    #[error(transparent)]
    Id(#[from] InvalidFeatureTypeId),
    #[error("feature type {0} is registered twice")]
    Duplicate(FeatureTypeId),
    /// A slot that `modifies` something other than one of the type's body
    /// inputs, or is not a body slot itself (ADR-0007).
    #[error(
        "feature type {id}'s slot `{slot}` modifies `{input}`, which is not a body input of it, or the slot is not a body"
    )]
    Modifies {
        id: FeatureTypeId,
        slot: SlotName,
        input: String,
    },
}

#[derive(Clone, Default)]
pub struct Registry {
    types: BTreeMap<FeatureTypeId, Arc<dyn FeatureType>>,
}

impl Registry {
    /// The built-in types, under `core.`.
    pub fn with_core_types() -> Self {
        let mut r = Registry::default();
        for ty in [
            Arc::new(DatumPlane::new()) as Arc<dyn FeatureType>,
            Arc::new(CoreSketch::new()),
        ] {
            r.register(ty)
                .expect("the core types have distinct, well-formed ids");
        }
        r
    }

    pub fn register(&mut self, ty: Arc<dyn FeatureType>) -> Result<FeatureTypeId, RegistryError> {
        let id = FeatureTypeId::new(ty.spec().id.clone())?;
        if self.types.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        let spec = ty.spec();
        for slot in &spec.outputs {
            let Some(input) = &slot.modifies else {
                continue;
            };
            let body_input = spec
                .inputs
                .iter()
                .any(|i| &i.name == input && i.kind == InputKind::Body);
            if !body_input || slot.kind != SlotKind::Body {
                return Err(RegistryError::Modifies {
                    id,
                    slot: slot.name.clone(),
                    input: input.clone(),
                });
            }
        }
        self.types.insert(id.clone(), ty);
        Ok(id)
    }

    pub fn get(&self, id: &FeatureTypeId) -> Option<&Arc<dyn FeatureType>> {
        self.types.get(id)
    }

    /// The registered ids, sorted.
    pub fn ids(&self) -> impl Iterator<Item = &FeatureTypeId> {
        self.types.keys()
    }
}

impl std::fmt::Debug for Registry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.types.keys()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ty(FeatureTypeSpec);

    impl FeatureType for Ty {
        fn spec(&self) -> &FeatureTypeSpec {
            &self.0
        }

        fn evaluate(&self, _: &mut dyn Kernel, _: &FeatureArgs) -> Result<TypeOutput, Diagnostic> {
            Ok(TypeOutput::default())
        }
    }

    fn ty(id: &str) -> Arc<dyn FeatureType> {
        Arc::new(Ty(FeatureTypeSpec {
            id: id.into(),
            version: 1,
            title: id.into(),
            params: vec![],
            inputs: vec![],
            outputs: vec![],
        }))
    }

    #[test]
    fn registers_built_ins_and_plugin_types_alike() {
        let mut r = Registry::default();
        r.register(ty("gears.spur")).unwrap();
        let plane = r.register(ty("core.datum-plane")).unwrap();
        assert_eq!(plane.plugin(), None);
        assert_eq!(r.get(&plane).unwrap().spec().version, 1);
        let ids: Vec<_> = r.ids().map(FeatureTypeId::as_str).collect();
        assert_eq!(ids, ["core.datum-plane", "gears.spur"]);
        assert_eq!(
            FeatureTypeId::new("gears.spur")
                .unwrap()
                .plugin()
                .unwrap()
                .as_str(),
            "gears"
        );
    }

    #[test]
    fn the_core_types_are_registered_under_core() {
        let r = Registry::with_core_types();
        let ids: Vec<_> = r.ids().map(FeatureTypeId::as_str).collect();
        assert_eq!(ids, ["core.datum-plane", "core.sketch"]);
    }

    #[test]
    fn refuses_a_duplicate_or_a_malformed_id() {
        let mut r = Registry::default();
        r.register(ty("core.datum-plane")).unwrap();
        assert!(matches!(
            r.register(ty("core.datum-plane")),
            Err(RegistryError::Duplicate(_))
        ));
        for bad in [
            "datum-plane",
            "core.",
            "Core.x",
            "gears.Spur",
            "gears.spur.x",
            ".x",
        ] {
            assert!(
                matches!(r.register(ty(bad)), Err(RegistryError::Id(_))),
                "{bad}"
            );
        }
        assert!(serde_json::from_str::<FeatureTypeId>("\"gears\"").is_err());
    }

    #[test]
    fn a_modifying_slot_names_one_of_its_body_inputs() {
        use arrix_plugin_api::{InputSpec, SlotSpec};
        let spec = |input: InputKind, slot: SlotKind, modifies: &str| {
            Arc::new(Ty(FeatureTypeSpec {
                id: "test.join".into(),
                version: 1,
                title: "Join".into(),
                params: vec![],
                inputs: vec![InputSpec {
                    name: "target".into(),
                    title: "Target".into(),
                    kind: input,
                }],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").unwrap(),
                    kind: slot,
                    modifies: Some(modifies.into()),
                }],
            })) as Arc<dyn FeatureType>
        };
        let mut r = Registry::default();
        for bad in [
            spec(InputKind::Body, SlotKind::Body, "tool"),
            spec(InputKind::Plane, SlotKind::Body, "target"),
            spec(InputKind::Body, SlotKind::Plane, "target"),
        ] {
            assert!(matches!(
                r.register(bad),
                Err(RegistryError::Modifies { .. })
            ));
        }
        r.register(spec(InputKind::Body, SlotKind::Body, "target"))
            .unwrap();
    }
}
