//! The test command `place.block` (plans/c3-m3 steps 5 and 7): in an
//! assembly, a block part's component on each picked edge. It finds the
//! block part by a feature of its own in the snapshot, the way `fasteners`
//! finds a size's part (ADR-0038 §4), and makes it, with a material, only
//! when no part has one. A `place.refuse` command answers a diagnostic,
//! `place.rogue` an id it was not given, and `place.panic` panics. Included
//! by `#[path]` into the Tier 0 test and built as the Tier 1 component, so
//! both tiers run the same code.

use arrix_plugin_api::{
    AssignMaterial, CommandGroup, CommandInput, CommandInputValue, CommandSpec, Commands,
    ComponentChange, ComponentDraft, ComponentId, ComponentOf, Context, Diagnostic, EdgeCurve,
    Expression, FeatureDraft, Id, InputKind, InputSpec, MaterialDraft, MaterialId, ParamSpec,
    ParamValue, PartDraft, PartId, PluginCommand, Pose, QuantityKind, Rgb, Severity, Snapshot,
};

/// The name of the feature that marks the block part.
const MARKER: &str = "block marker";

pub struct Place;

fn spec(name: &str) -> CommandSpec {
    CommandSpec {
        id: format!("place.{name}"),
        title: name.into(),
        group: "Place".into(),
        params: vec![ParamSpec {
            name: "size".into(),
            title: "Size".into(),
            kind: QuantityKind::Length,
            default: "5 mm".into(),
            shown_when: None,
        }],
        choices: vec![],
        inputs: vec![InputSpec {
            name: "edges".into(),
            title: "Edges".into(),
            kind: InputKind::Edge,
            list: true,
            shown_when: None,
        }],
        max_new: Some(8),
    }
}

fn refuse(code: &str, message: String) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

impl Commands for Place {
    fn commands(&self) -> Vec<CommandSpec> {
        ["block", "refuse", "rogue", "panic"].map(spec).to_vec()
    }

    fn expand(
        &self,
        snapshot: &mut dyn Snapshot,
        command: &str,
        context: &Context,
        params: &[ParamValue],
        _choices: &[arrix_plugin_api::ChoiceValue],
        inputs: &[CommandInput],
        ids: &[Id],
    ) -> Result<CommandGroup, Diagnostic> {
        match command {
            "place.refuse" => return Err(refuse("place.refused", "this command refuses".into())),
            "place.panic" => panic!("place.panic was asked to"),
            _ => {}
        }
        let Context::Assembly(assembly) = *context else {
            return Err(refuse("place.context", "an assembly is needed".into()));
        };
        let size = params
            .iter()
            .find(|p| p.name == "size")
            .map_or(0.005, |p| p.value.si);
        let mut ids = ids.iter().copied();
        let mut next = || {
            ids.next()
                .ok_or_else(|| refuse("place.ids", "out of ids".into()))
        };
        let mut commands = Vec::new();

        // The block part, found by its marker or made once.
        let found = snapshot
            .parts()
            .into_iter()
            .find(|p| p.features.iter().any(|f| f.name == MARKER))
            .map(|p| p.id);
        let part = match found {
            Some(part) => part,
            None if inputs.is_empty() => PartId(Id(0)),
            None => {
                let (part, feature, material) = (PartId(next()?), next()?, MaterialId(next()?));
                let marker = FeatureDraft {
                    id: arrix_plugin_api::FeatureId(feature),
                    type_id: "core.datum-plane".into(),
                    name: MARKER.into(),
                    params: vec![],
                    choices: vec![arrix_plugin_api::ChoiceValue {
                        name: "world".into(),
                        value: "xy".into(),
                    }],
                    inputs: vec![],
                };
                commands.push(PluginCommand::AddMaterial(MaterialDraft {
                    id: material,
                    name: "Placed steel".into(),
                    density: Some(Expression::Text("7850 kg/m3".into())),
                    color: Rgb {
                        red: 0.5,
                        green: 0.5,
                        blue: 0.55,
                    },
                }));
                commands.push(PluginCommand::AddPart(PartDraft {
                    id: part,
                    name: "Block".into(),
                    features: vec![marker],
                    material: None,
                }));
                commands.push(PluginCommand::AssignMaterial(AssignMaterial {
                    part,
                    body: None,
                    material: Some(material),
                }));
                part
            }
        };

        for (n, input) in inputs.iter().enumerate() {
            let CommandInputValue::Edge(edge) = &input.value;
            let at = match edge.curve {
                EdgeCurve::Circle(c) => c.centre,
                EdgeCurve::Line(l) => (l.start + l.end) * 0.5,
                EdgeCurve::Other => arrix_plugin_api::DVec3::ZERO,
            };
            let id = if command == "place.rogue" {
                Id(1)
            } else {
                next()?
            };
            let si = |kind, v| Expression::Si(arrix_plugin_api::Quantity::si(kind, v));
            let length = |v| si(QuantityKind::Length, v);
            let zero = si(QuantityKind::Angle, 0.0);
            commands.push(PluginCommand::AddComponent(ComponentChange {
                assembly,
                component: ComponentDraft {
                    id: ComponentId(id),
                    name: format!("block {}", n + 1),
                    of: ComponentOf::Part(part),
                    pose: Some(Pose {
                        position: [length(at.x), length(at.y), length(at.z + size)],
                        rpy: [zero.clone(), zero.clone(), zero],
                    }),
                },
            }));
        }
        Ok(CommandGroup {
            label: "Place blocks".into(),
            commands,
        })
    }
}
