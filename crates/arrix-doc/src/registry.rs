//! Feature types by id (docs/DATA-MODEL.md §Features). Built-ins register
//! under `core.` through the same shape a plugin's feature is adapted onto,
//! so the evaluator has one path for both. A record whose type is not here
//! is not an error: it evaluates frozen.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_core::{Diagnostic, FeatureId, SlotName};
use arrix_kernel::Interrupt;
use arrix_plugin_api::{
    ChoiceValue, ExportFile, ExporterSpec, FeatureOutput, FeatureTypeSpec, InputKind, Kernel,
    ParamValue, ResolvedInput, SlotKind, Snapshot, Subject,
};
use arrix_sketch::Sketch;

use crate::core_blend::CoreBlend;
use crate::core_boolean::CoreBoolean;
use crate::core_extrude::CoreExtrude;
use crate::core_hole::CoreHole;
use crate::core_import::CoreImportStep;
use crate::core_mirror::CoreMirror;
use crate::core_pattern::CorePattern;
use crate::core_revolve::CoreRevolve;
use crate::core_types::{CoreSketch, DatumPlane};
use crate::document::{FeatureTypeId, InvalidFeatureTypeId};
use crate::eval::SketchView;

/// An exporter as the evaluator sees it, built-in or plugin: the plugin
/// API's `Exporter` for one id (ADR-0037). It reads the snapshot and
/// returns files, and does nothing else.
pub trait ExporterType: Send + Sync {
    fn spec(&self) -> &ExporterSpec;

    /// Deterministic: a pure function of the snapshot and the options.
    fn export(
        &self,
        snapshot: &mut dyn Snapshot,
        subject: &Subject,
        params: &[ParamValue],
        choices: &[ChoiceValue],
    ) -> Result<Vec<ExportFile>, Diagnostic>;
}

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

    /// Whether the feature reads its record's file: the evaluator then
    /// calls [`evaluate_file`](Self::evaluate_file) in place of `evaluate`,
    /// and its input hash holds the file (ADR-0029). Only a built-in does.
    fn reads_file(&self) -> bool {
        false
    }

    /// The outputs of a feature that reads the file its record keeps, which
    /// is `None` when the record has none. `kernel` is the plugin API's
    /// with the one call a file needs, which is the core's alone.
    fn evaluate_file(
        &self,
        _kernel: &mut dyn FileKernel,
        _args: &FeatureArgs,
        _file: Option<FileArg<'_>>,
    ) -> Result<TypeOutput, Diagnostic> {
        Err(Diagnostic::new(
            arrix_core::Severity::Error,
            "feature.output",
            format!("{} reads no file", self.spec().id),
        )
        .expect("a literal code is well-formed"))
    }

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

/// A record's file as the feature reads it: the base name for messages
/// and the bytes.
#[derive(Clone, Copy, Debug)]
pub struct FileArg<'a> {
    pub name: &'a str,
    pub bytes: &'a [u8],
}

/// The plugin API's `Kernel` with the call that reads a file in.
pub trait FileKernel: Kernel {
    /// Solid number `solid` (from 1) of the STEP file `file`, in metres,
    /// its entities named from the file's (`Kernel::import_step`); in its
    /// product's own frame when `product_frame` (`Kernel::import_step_in`).
    fn import_step(
        &mut self,
        file: &[u8],
        solid: u32,
        product_frame: bool,
    ) -> Result<arrix_plugin_api::Body, Diagnostic>;
}

/// What the evaluator resolved for one feature: every parameter of the
/// type's form in SI (the record's expression or the form's default), its
/// plain choices as written, the inputs the record gives, in the form's
/// order, and a `core.sketch` record's sketch. `interrupt` is the
/// evaluator's flag, for a host that runs a plugin's code outside the
/// kernel to stop it by (docs/CONCURRENCY-WASM.md §Cancellation); it is
/// never handed to a plugin.
#[derive(Debug, Default)]
pub struct FeatureArgs {
    pub params: Vec<ParamValue>,
    pub choices: BTreeMap<String, String>,
    pub inputs: Vec<ResolvedInput>,
    pub sketch: Option<SketchArgs>,
    pub interrupt: Interrupt,
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

    /// The first input called `name`: the whole of a single input.
    pub fn input(&self, name: &str) -> Option<&ResolvedInput> {
        self.inputs.iter().find(|i| i.name == name)
    }

    /// Every input called `name`, in the record's order: a list input's
    /// elements (ADR-0012); empty when it is unset.
    pub fn inputs_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a ResolvedInput> {
        self.inputs.iter().filter(move |i| i.name == name)
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
    /// A choice whose default is not one of its options, or a field shown
    /// under a choice or a value the type does not declare (ADR-0018).
    #[error("feature type {id}'s form is inconsistent: {what}")]
    Form { id: FeatureTypeId, what: String },
}

#[derive(Clone, Default)]
pub struct Registry {
    types: BTreeMap<FeatureTypeId, Arc<dyn FeatureType>>,
    exporters: BTreeMap<FeatureTypeId, Arc<dyn ExporterType>>,
}

impl Registry {
    /// The built-in types, under `core.`.
    pub fn with_core_types() -> Self {
        let mut r = Registry::default();
        for ty in [
            Arc::new(CoreBoolean::new()) as Arc<dyn FeatureType>,
            Arc::new(CoreBlend::chamfer()),
            Arc::new(DatumPlane::new()),
            Arc::new(CoreExtrude::new()),
            Arc::new(CoreBlend::fillet()),
            Arc::new(CoreHole::new()),
            Arc::new(CoreImportStep::new()),
            Arc::new(CoreMirror::new()),
            Arc::new(CorePattern::linear()),
            Arc::new(CorePattern::circular()),
            Arc::new(CoreRevolve::new()),
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
        if let Some(what) = form_fault(spec) {
            return Err(RegistryError::Form { id, what });
        }
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

    /// Registers an exporter (ADR-0037) under its `<plugin>.<name>` id.
    pub fn register_exporter(
        &mut self,
        ty: Arc<dyn ExporterType>,
    ) -> Result<FeatureTypeId, RegistryError> {
        let id = FeatureTypeId::new(ty.spec().id.clone())?;
        if self.exporters.contains_key(&id) {
            return Err(RegistryError::Duplicate(id));
        }
        self.exporters.insert(id.clone(), ty);
        Ok(id)
    }

    /// Leaves out every feature type and exporter `plugin` contributes, as
    /// a build without it would. `false` when it contributed none.
    pub fn remove_plugin(&mut self, plugin: &arrix_core::PluginId) -> bool {
        let before = self.types.len() + self.exporters.len();
        self.types
            .retain(|id, _| id.plugin().as_ref() != Some(plugin));
        self.exporters
            .retain(|id, _| id.plugin().as_ref() != Some(plugin));
        self.types.len() + self.exporters.len() != before
    }

    /// The exporter `id` (`<plugin>.<name>`).
    pub fn exporter(&self, id: &str) -> Option<&Arc<dyn ExporterType>> {
        self.exporters.get(&FeatureTypeId::new(id).ok()?)
    }

    /// The specs of every exporter, in id order.
    pub fn exporter_specs(&self) -> Vec<ExporterSpec> {
        self.exporters.values().map(|e| e.spec().clone()).collect()
    }

    pub fn get(&self, id: &FeatureTypeId) -> Option<&Arc<dyn FeatureType>> {
        self.types.get(id)
    }

    /// The specs of the types a create command makes, in id order: every
    /// one but those that read a record's file (`FeatureType::reads_file`),
    /// which only an import creates, with its file (M2's, ADR-0029).
    pub fn creatable_specs(&self) -> Vec<FeatureTypeSpec> {
        self.types
            .values()
            .filter(|ty| !ty.reads_file())
            .map(|ty| ty.spec().clone())
            .collect()
    }

    /// The registered ids, sorted.
    pub fn ids(&self) -> impl Iterator<Item = &FeatureTypeId> {
        self.types.keys()
    }
}

/// What is wrong with a type's choices and the fields shown under them,
/// if anything.
fn form_fault(spec: &FeatureTypeSpec) -> Option<String> {
    for (i, c) in spec.choices.iter().enumerate() {
        if spec.choices[..i].iter().any(|o| o.name == c.name) {
            return Some(format!("choice `{}` is declared twice", c.name));
        }
        if c.options.is_empty() {
            return Some(format!("choice `{}` has no options", c.name));
        }
        if let Some(d) = &c.default
            && !c.options.iter().any(|o| &o.name == d)
        {
            return Some(format!(
                "choice `{}`'s default `{d}` is not an option",
                c.name
            ));
        }
    }
    let shown = spec
        .params
        .iter()
        .map(|p| (&p.name, &p.shown_when))
        .chain(spec.inputs.iter().map(|i| (&i.name, &i.shown_when)));
    for (field, when) in shown {
        let Some(when) = when else { continue };
        let Some(choice) = spec.choices.iter().find(|c| c.name == when.choice) else {
            return Some(format!(
                "`{field}` is shown under `{}`, which is not a choice",
                when.choice
            ));
        };
        if let Some(v) = when
            .values
            .iter()
            .find(|v| !choice.options.iter().any(|o| &o.name == *v))
        {
            return Some(format!(
                "`{field}` is shown when `{}` is `{v}`, which is not an option",
                when.choice
            ));
        }
    }
    None
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
            choices: vec![],
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
        assert_eq!(
            ids,
            [
                "core.boolean",
                "core.chamfer",
                "core.datum-plane",
                "core.extrude",
                "core.fillet",
                "core.hole",
                "core.import-step",
                "core.mirror",
                "core.pattern-circular",
                "core.pattern-linear",
                "core.revolve",
                "core.sketch"
            ]
        );
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
    fn a_form_shows_fields_only_under_the_choices_it_declares() {
        use arrix_core::QuantityKind;
        use arrix_plugin_api::{ChoiceOption, ChoiceSpec, ParamSpec, ShownWhen};
        let form = |default: Option<&str>, choice: &str, value: &str| {
            let spec = Ty(FeatureTypeSpec {
                id: "test.form".into(),
                version: 1,
                title: "Form".into(),
                params: vec![ParamSpec {
                    name: "depth".into(),
                    title: "Depth".into(),
                    kind: QuantityKind::Length,
                    default: "1 mm".into(),
                    shown_when: Some(ShownWhen {
                        choice: choice.into(),
                        values: vec![value.into()],
                    }),
                }],
                choices: vec![ChoiceSpec {
                    name: "extent".into(),
                    title: "Extent".into(),
                    options: ["blind", "through"]
                        .map(|name| ChoiceOption {
                            name: name.into(),
                            title: name.into(),
                        })
                        .to_vec(),
                    default: default.map(Into::into),
                }],
                inputs: vec![],
                outputs: vec![],
            });
            Arc::new(spec) as Arc<dyn FeatureType>
        };
        let mut r = Registry::default();
        for bad in [
            form(Some("sideways"), "extent", "blind"),
            form(Some("blind"), "kind", "blind"),
            form(Some("blind"), "extent", "deep"),
        ] {
            assert!(matches!(r.register(bad), Err(RegistryError::Form { .. })));
        }
        r.register(form(None, "extent", "blind")).unwrap();
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
                    list: false,
                    shown_when: None,
                }],
                outputs: vec![SlotSpec {
                    name: SlotName::new("body").unwrap(),
                    kind: slot,
                    modifies: Some(modifies.into()),
                }],
                choices: vec![],
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
