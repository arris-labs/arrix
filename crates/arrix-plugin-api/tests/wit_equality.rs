//! The WIT world and the Rust traits held equal (docs/PLUGINS.md §One
//! interface; the C1 risk "the WIT world and the Tier 0 traits drift apart
//! before C3 builds Tier 1", docs/ROADMAP.md).
//!
//! Three holds, each catching what the others cannot:
//!
//! 1. **Types.** Every type of the world converts to its Rust type and
//!    back, with exhaustive destructuring and matches on both sides, so a
//!    field or case added to either fails to compile; every case round
//!    trips at run time.
//! 2. **Functions.** `feature`'s `Guest` is implemented by forwarding to a
//!    Rust `Feature`, and a Rust `Feature` by forwarding to the `Guest`;
//!    the same for `exchange`'s `Guest` and a Rust `Exporter`, and for
//!    `commands`' `Guest` and a Rust `Commands`; the Rust
//!    `Kernel` and `Snapshot` are implemented over the world's imports. A
//!    method or function added on either side fails to compile.
//! 3. **Inventory.** The world as parsed is exactly the types and functions
//!    this file covers, so a WIT addition the bindings would silently
//!    absorb (an unused record, a new import) fails here. The generated
//!    wire (`wire/`, ADR-0022) must hold the same inventory, so a generator
//!    that skips an item fails here too.

use std::collections::BTreeSet;
use std::path::Path;

use arrix_plugin_api as api;
use arrix_plugin_api::guest::bindings::exports::arrix::plugin::{
    commands as wc, exchange as wx, feature as wf,
};
use arrix_plugin_api::guest::{
    CommandSource, Exported, ExporterSource, FeatureSource, Mirror, NoCommands, NoExporters,
    NoFeatures, Plugin, WitKernel, WitSnapshot, WithCommands, WithExporters, WithFeatures,
    input_to_wit, output_from_wit,
};
use arrix_plugin_api::{
    AssemblyId, Axis, ComponentId, CurveKey, DVec2, DVec3, Diagnostic, FeatureId, Frame, Id,
    JointId, Motion, ParamId, PartId, PersistentName, PluginId, Profile, ProfileLoop,
    ProfileSegment, Quantity, QuantityKind, RecordId, Ref, RegionKey, Severity, SketchEntityId,
    SlotName,
};

fn round_trip<T: Mirror + PartialEq + std::fmt::Debug>(v: &T) {
    assert_eq!(&T::from_wit(v.to_wit()), v);
}
/// A test plugin: one feature type, a plane offset from its input, so it
/// evaluates without the kernel and runs natively through both adapters.
struct Offset;

fn offset_type() -> api::FeatureTypeSpec {
    api::FeatureTypeSpec {
        id: "test.offset".into(),
        version: 1,
        title: "Offset plane".into(),
        params: vec![api::ParamSpec {
            name: "distance".into(),
            title: "Distance".into(),
            kind: QuantityKind::Length,
            default: "10 mm".into(),
            shown_when: Some(api::ShownWhen {
                choice: "side".into(),
                values: vec!["above".into()],
            }),
        }],
        choices: vec![api::ChoiceSpec {
            name: "side".into(),
            title: "Side".into(),
            options: vec![
                api::ChoiceOption {
                    name: "above".into(),
                    title: "Above".into(),
                },
                api::ChoiceOption {
                    name: "below".into(),
                    title: "Below".into(),
                },
            ],
            default: Some("above".into()),
        }],
        inputs: vec![api::InputSpec {
            name: "plane".into(),
            title: "Plane".into(),
            kind: api::InputKind::Plane,
            list: false,
            shown_when: None,
        }],
        outputs: vec![api::SlotSpec {
            name: SlotName::new("plane").unwrap(),
            kind: api::SlotKind::Plane,
            modifies: None,
        }],
    }
}

impl api::Feature for Offset {
    fn describe(&self) -> Vec<api::FeatureTypeSpec> {
        vec![offset_type()]
    }

    fn evaluate(
        &self,
        _kernel: &mut dyn api::Kernel,
        type_id: &str,
        params: &[api::ParamValue],
        choices: &[api::ChoiceValue],
        inputs: &[api::ResolvedInput],
    ) -> Result<api::FeatureOutput, Diagnostic> {
        let refuse = |what: &str| {
            Err(Diagnostic::new(Severity::Error, "test.offset.input", what.to_owned()).unwrap())
        };
        if type_id != "test.offset" {
            return refuse("unknown type");
        }
        let below = choices
            .iter()
            .any(|c| c.name == "side" && c.value == "below");
        let (Some(distance), Some(api::InputValue::Plane(plane))) = (
            params.iter().find(|p| p.name == "distance"),
            inputs.iter().find(|i| i.name == "plane").map(|i| &i.value),
        ) else {
            return refuse("missing distance or plane");
        };
        Ok(api::FeatureOutput {
            slots: vec![api::SlotOutput {
                name: SlotName::new("plane").unwrap(),
                value: api::OutputValue::Plane(plane.offset(if below {
                    -distance.value.si
                } else {
                    distance.value.si
                })),
            }],
            tool: None,
        })
    }
}

impl FeatureSource for Offset {
    fn feature() -> &'static dyn api::Feature {
        &Offset
    }
}

/// The world's `feature` export over `Offset`, as a Tier 1 guest built by
/// `export_plugin!` is.
struct OffsetPlugin;

impl Plugin for OffsetPlugin {
    type Features = WithFeatures<Offset>;
    type Exporters = NoExporters;
    type Commands = NoCommands;
}

type OffsetExported = Exported<OffsetPlugin>;

/// A test exporter: a text file naming the subject, and a second file that
/// is its bytes' length, so a result is checked file by file.
struct Dump;

fn dump_spec() -> api::ExporterSpec {
    api::ExporterSpec {
        id: "test.dump".into(),
        title: "Dump".into(),
        extension: "txt".into(),
        subject: api::SubjectKind::Assembly,
        params: vec![api::ParamSpec {
            name: "scale".into(),
            title: "Scale".into(),
            kind: QuantityKind::Ratio,
            default: "1".into(),
            shown_when: None,
        }],
        choices: vec![api::ChoiceSpec {
            name: "unit".into(),
            title: "Unit".into(),
            options: vec![api::ChoiceOption {
                name: "metre".into(),
                title: "Metre".into(),
            }],
            default: Some("metre".into()),
        }],
    }
}

impl api::Exporter for Dump {
    fn exporters(&self) -> Vec<api::ExporterSpec> {
        vec![dump_spec()]
    }

    fn export(
        &self,
        _snapshot: &mut dyn api::Snapshot,
        exporter: &str,
        subject: &api::Subject,
        params: &[api::ParamValue],
        choices: &[api::ChoiceValue],
    ) -> Result<Vec<api::ExportFile>, Diagnostic> {
        if exporter != "test.dump" {
            return Err(Diagnostic::new(Severity::Error, "test.dump.unknown", exporter).unwrap());
        }
        let text = format!("{subject:?} {params:?} {choices:?}").into_bytes();
        Ok(vec![
            api::ExportFile {
                name: "dump.txt".into(),
                bytes: text.clone(),
            },
            api::ExportFile {
                name: "meshes/len.bin".into(),
                bytes: (text.len() as u64).to_le_bytes().to_vec(),
            },
        ])
    }
}

impl ExporterSource for Dump {
    fn exporter() -> &'static dyn api::Exporter {
        &Dump
    }
}

struct DumpPlugin;

impl Plugin for DumpPlugin {
    type Features = NoFeatures;
    type Exporters = WithExporters<Dump>;
    type Commands = NoCommands;
}

type DumpExported = Exported<DumpPlugin>;

/// A Rust `Exporter` implemented by forwarding to the world's export, as
/// the host sees a Tier 1 plugin. The snapshot is the guest's own import,
/// which natively cannot be reached, so `Dump` never reads it.
struct ImportedDump;

impl api::Exporter for ImportedDump {
    fn exporters(&self) -> Vec<api::ExporterSpec> {
        Vec::from_wit(<DumpExported as wx::Guest>::exporters())
    }

    fn export(
        &self,
        _snapshot: &mut dyn api::Snapshot,
        exporter: &str,
        subject: &api::Subject,
        params: &[api::ParamValue],
        choices: &[api::ChoiceValue],
    ) -> Result<Vec<api::ExportFile>, Diagnostic> {
        <DumpExported as wx::Guest>::run(
            exporter.to_owned(),
            subject.to_wit(),
            params.to_vec().to_wit(),
            choices.to_vec().to_wit(),
        )
        .map(Vec::from_wit)
        .map_err(Diagnostic::from_wit)
    }
}

/// A test command: a block part on each picked edge, one component each.
struct Place;

fn place_spec() -> api::CommandSpec {
    api::CommandSpec {
        id: "test.place".into(),
        title: "Place".into(),
        group: "Test".into(),
        params: vec![api::ParamSpec {
            name: "size".into(),
            title: "Size".into(),
            kind: QuantityKind::Length,
            default: "5 mm".into(),
            shown_when: None,
        }],
        choices: vec![],
        inputs: vec![api::InputSpec {
            name: "edges".into(),
            title: "Edges".into(),
            kind: api::InputKind::Edge,
            list: true,
            shown_when: None,
        }],
        max_new: Some(8),
    }
}

impl api::Commands for Place {
    fn commands(&self) -> Vec<api::CommandSpec> {
        vec![place_spec()]
    }

    fn expand(
        &self,
        _snapshot: &mut dyn api::Snapshot,
        command: &str,
        context: &api::Context,
        params: &[api::ParamValue],
        _choices: &[api::ChoiceValue],
        inputs: &[api::CommandInput],
        ids: &[Id],
    ) -> Result<api::CommandGroup, Diagnostic> {
        let refuse = |code: &str, what: &str| {
            Err(Diagnostic::new(Severity::Error, code, what.to_owned()).unwrap())
        };
        if command != "test.place" {
            return refuse("test.place.unknown", command);
        }
        let api::Context::Assembly(assembly) = *context else {
            return refuse("test.place.context", "an assembly is needed");
        };
        let mut ids = ids.iter().copied();
        let mut commands = Vec::new();
        for input in inputs {
            let api::CommandInputValue::Edge(edge) = &input.value;
            let (Some(part), Some(component)) = (ids.next(), ids.next()) else {
                return refuse("test.place.ids", "out of ids");
            };
            commands.push(api::PluginCommand::AddPart(api::PartDraft {
                id: PartId(part),
                name: "block".into(),
                features: vec![],
                material: None,
            }));
            commands.push(api::PluginCommand::AddComponent(api::ComponentChange {
                assembly,
                component: api::ComponentDraft {
                    id: ComponentId(component),
                    name: format!("block {}", edge.name),
                    of: api::ComponentOf::Part(PartId(part)),
                    pose: None,
                },
            }));
        }
        let _ = params;
        Ok(api::CommandGroup {
            label: "Place".into(),
            commands,
        })
    }
}

impl CommandSource for Place {
    fn commands() -> &'static dyn api::Commands {
        &Place
    }
}

struct PlacePlugin;

impl Plugin for PlacePlugin {
    type Features = NoFeatures;
    type Exporters = NoExporters;
    type Commands = WithCommands<Place>;
}

type PlaceExported = Exported<PlacePlugin>;

/// A Rust `Commands` implemented by forwarding to the world's export, as
/// the host sees a Tier 1 plugin.
struct ImportedPlace;

impl api::Commands for ImportedPlace {
    fn commands(&self) -> Vec<api::CommandSpec> {
        Vec::from_wit(<PlaceExported as wc::Guest>::commands())
    }

    fn expand(
        &self,
        _snapshot: &mut dyn api::Snapshot,
        command: &str,
        context: &api::Context,
        params: &[api::ParamValue],
        choices: &[api::ChoiceValue],
        inputs: &[api::CommandInput],
        ids: &[Id],
    ) -> Result<api::CommandGroup, Diagnostic> {
        <PlaceExported as wc::Guest>::expand(
            command.to_owned(),
            context.to_wit(),
            params.to_vec().to_wit(),
            choices.to_vec().to_wit(),
            inputs.to_vec().to_wit(),
            ids.iter().map(|i| i.0).collect(),
        )
        .map(api::CommandGroup::from_wit)
        .map_err(Diagnostic::from_wit)
    }
}

/// The snapshot imported by the world, as a Rust `Snapshot`: held at
/// compile time, since natively the imports are unreachable.
#[allow(dead_code)]
fn the_snapshot_is_the_worlds_import() -> impl api::Snapshot {
    WitSnapshot
}

/// A Rust `Feature` implemented by forwarding to the world's export, as
/// the host sees a Tier 1 plugin.
struct Imported;

impl api::Feature for Imported {
    fn describe(&self) -> Vec<api::FeatureTypeSpec> {
        Vec::from_wit(<OffsetExported as wf::Guest>::describe())
    }

    fn evaluate(
        &self,
        _kernel: &mut dyn api::Kernel,
        type_id: &str,
        params: &[api::ParamValue],
        choices: &[api::ChoiceValue],
        inputs: &[api::ResolvedInput],
    ) -> Result<api::FeatureOutput, Diagnostic> {
        // The host's handles would enter the plugin's table here; natively
        // no body exists to hand over, so this arm is held at compile time.
        let mut bodies = WitKernel::default();
        let inputs = inputs.iter().map(|i| input_to_wit(i, &mut bodies));
        let (params, choices) = (params.to_vec().to_wit(), choices.to_vec().to_wit());
        <OffsetExported as wf::Guest>::evaluate(
            type_id.to_owned(),
            params,
            choices,
            inputs.collect(),
        )
        .map(output_from_wit)
        .map_err(Diagnostic::from_wit)
    }
}

// ---- tests ------------------------------------------------------------------

fn key(n: u64) -> CurveKey {
    CurveKey(Id(n))
}

fn tilted() -> Frame {
    Frame::new(
        DVec3::new(0.01, -0.02, 0.5),
        DVec3::new(0.0, 0.6, 0.8),
        DVec3::new(1.0, 0.0, 0.0),
    )
    .unwrap()
}

#[test]
fn every_type_round_trips_through_the_world() {
    for v in [DVec3::ZERO, DVec3::new(1e-6, -2.5, 3e3)] {
        round_trip(&v);
    }
    for f in [Frame::WORLD_XY, Frame::WORLD_YZ, Frame::WORLD_ZX, tilted()] {
        round_trip(&f);
    }
    for kind in [
        QuantityKind::Length,
        QuantityKind::Angle,
        QuantityKind::Count,
        QuantityKind::Ratio,
        QuantityKind::Mass,
        QuantityKind::Density,
    ] {
        round_trip(&Quantity::si(kind, 0.012));
    }

    let face: PersistentName = format!("face:sweep.{}.side.{}", Id(7), Id(9))
        .parse()
        .unwrap();
    let refs = vec![
        Ref::Param(ParamId(Id(1))),
        Ref::Feature(FeatureId(Id(2))),
        Ref::Slot {
            feature: FeatureId(Id(3)),
            slot: SlotName::new("body").unwrap(),
        },
        Ref::Topo(face.clone()),
        Ref::Sketch {
            feature: FeatureId(Id(4)),
            entity: SketchEntityId(Id(5)),
        },
        Ref::Region {
            feature: FeatureId(Id(4)),
            key: RegionKey::new(
                [SketchEntityId(Id(6)), SketchEntityId(Id(5))],
                [-0.02, 0.015],
            ),
        },
        Ref::Plugin {
            plugin: PluginId::new("gears").unwrap(),
            record: RecordId(Id(u64::MAX)),
        },
    ];
    round_trip(&face);
    round_trip(&refs);
    for severity in [Severity::Info, Severity::Warning, Severity::Error] {
        round_trip(&Diagnostic::new(severity, "ref.lost", "gone").unwrap());
    }
    round_trip(
        &Diagnostic::new(Severity::Error, "kernel.degenerate.not-positive", "no")
            .unwrap()
            .with_refs(refs.clone())
            .with_candidates(refs.into_iter().rev()),
    );

    let path = ProfileLoop::Path {
        start: DVec2::new(0.0, 0.0),
        segments: vec![
            ProfileSegment::Line {
                key: key(1),
                to: DVec2::new(0.04, 0.0),
            },
            ProfileSegment::Arc {
                key: key(2),
                to: DVec2::new(0.0, 0.0),
                via: DVec2::new(0.02, 0.01),
            },
        ],
    };
    let hole = ProfileLoop::Circle {
        key: key(3),
        center: DVec2::new(0.02, 0.003),
        radius: 0.001,
    };
    round_trip(&Profile::new(tilted(), path, vec![hole]).unwrap());
    round_trip(&api::MassProperties {
        volume: 1e-6,
        area: 6e-4,
        centroid: DVec3::new(0.0, 0.0, 0.005),
    });

    round_trip(&offset_type());
    round_trip(&api::SlotSpec {
        name: SlotName::new("body").unwrap(),
        kind: api::SlotKind::Body,
        modifies: Some("target".into()),
    });
    for kind in [
        api::InputKind::Plane,
        api::InputKind::Region,
        api::InputKind::Body,
        api::InputKind::Edge,
        api::InputKind::Point,
        api::InputKind::Axis,
        api::InputKind::Tool,
    ] {
        round_trip(&kind);
        round_trip(&api::InputSpec {
            name: "in".into(),
            title: "In".into(),
            kind,
            list: kind == api::InputKind::Edge,
            shown_when: (kind == api::InputKind::Body).then(|| api::ShownWhen {
                choice: "mode".into(),
                values: vec!["join".into(), "cut".into()],
            }),
        });
    }
    round_trip(&Axis {
        origin: DVec3::new(0.01, 0.0, -0.002),
        direction: DVec3::new(0.0, 0.6, 0.8),
    });
    round_trip(&Motion {
        translation: DVec3::new(0.008, 0.0, 0.0),
        axis: DVec3::Z,
        angle: 0.5,
    });
    for normal in [None, Some(DVec3::Z)] {
        round_trip(&api::Point {
            at: DVec3::new(0.01, 0.02, 0.005),
            normal,
        });
    }
    round_trip(&api::ToolOp::Fuse);
    round_trip(&api::ToolOp::Cut);
    document_types_round_trip();
}

fn document_types_round_trip() {
    let part = PartId(Id(11));
    let assembly = AssemblyId(Id(12));
    let body = api::BodyRef {
        feature: FeatureId(Id(13)),
        slot: SlotName::new("body").unwrap(),
    };
    round_trip(&body);
    let material = api::MaterialInfo {
        name: "Steel".into(),
        color: api::Rgb {
            red: 0.5,
            green: 0.25,
            blue: 1.0,
        },
    };
    round_trip(&material);
    round_trip(&api::PartInfo {
        id: part,
        name: "Bracket".into(),
        features: vec![api::FeatureInfo {
            id: FeatureId(Id(18)),
            type_id: "core.extrude".into(),
            name: "Extrude 1".into(),
            choices: vec![api::ChoiceValue {
                name: "mode".into(),
                value: "join".into(),
            }],
            params: vec![api::ParamValue {
                name: "depth".into(),
                value: Quantity::si(QuantityKind::Length, 0.01),
            }],
        }],
        bodies: vec![
            api::BodyInfo {
                body: body.clone(),
                material: Some(material),
            },
            api::BodyInfo {
                body,
                material: None,
            },
        ],
    });
    round_trip(&api::Triangles {
        positions: vec![0.0, 0.0, 0.0, 1e-3, 0.0, 0.0, 0.0, 1e-3, 0.0],
        normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        indices: vec![0, 1, 2],
    });
    round_trip(&api::Inertial {
        mass: 0.125,
        centroid: DVec3::new(0.01, 0.0, -0.002),
        inertia: api::InertiaTensor {
            xx: 1e-6,
            xy: -1e-8,
            xz: 2e-8,
            yy: 2e-6,
            yz: -3e-8,
            zz: 3e-6,
        },
    });
    for kind in [
        api::JointKind::Fixed,
        api::JointKind::Revolute,
        api::JointKind::Prismatic,
    ] {
        round_trip(&kind);
    }
    let component = ComponentId(Id(14));
    let unplaced = Diagnostic::new(Severity::Warning, "assembly.unplaced", "no joint").unwrap();
    let components = vec![
        api::ComponentInfo {
            path: vec![component],
            name: "arm".into(),
            of: api::ComponentOf::Part(part),
            pose: tilted(),
            diagnostics: vec![],
        },
        api::ComponentInfo {
            path: vec![component, ComponentId(Id(15))],
            name: "hand".into(),
            of: api::ComponentOf::Assembly(assembly),
            pose: Frame::WORLD_XY,
            diagnostics: vec![unplaced.clone()],
        },
    ];
    let placed = api::JointPlacement {
        parent_anchor: tilted(),
        child_anchor: Frame::WORLD_ZX,
        value: 0.5,
        diagnostics: vec![unplaced.clone()],
    };
    let joints = vec![
        api::JointInfo {
            id: JointId(Id(16)),
            name: "hinge".into(),
            kind: api::JointKind::Revolute,
            parent: vec![component],
            child: vec![component, ComponentId(Id(15))],
            limits: Some([-1.5, 1.5]),
            state: api::JointState::Placed(placed),
        },
        api::JointInfo {
            id: JointId(Id(17)),
            name: "slide".into(),
            kind: api::JointKind::Prismatic,
            parent: vec![],
            child: vec![component],
            limits: None,
            state: api::JointState::Failed(unplaced),
        },
    ];
    round_trip(&api::AssemblyInfo {
        id: assembly,
        name: "Arm".into(),
        components,
        joints,
    });

    for kind in [api::SubjectKind::Part, api::SubjectKind::Assembly] {
        round_trip(&kind);
    }
    round_trip(&api::Subject::Part(part));
    round_trip(&api::Subject::Assembly(assembly));
    round_trip(&dump_spec());
    round_trip(&api::ExportFile {
        name: "meshes/link.stl".into(),
        bytes: vec![0, 1, 254, 255],
    });
    command_types_round_trip();
}

fn command_types_round_trip() {
    round_trip(&place_spec());
    round_trip(&api::Context::Part(PartId(Id(1))));
    round_trip(&api::Context::Assembly(AssemblyId(Id(2))));
    let edge: PersistentName = format!("edge:revolve.{}.side.{}", Id(7), Id(9))
        .parse()
        .unwrap();
    for curve in [
        api::EdgeCurve::Circle(api::CircleEdge {
            centre: DVec3::new(0.01, 0.02, 0.003),
            axis: DVec3::new(0.0, 0.0, -1.0),
            radius: 0.0015,
        }),
        api::EdgeCurve::Line(api::LineEdge {
            start: DVec3::ZERO,
            end: DVec3::new(0.0, 0.1, 0.0),
        }),
        api::EdgeCurve::Other,
    ] {
        round_trip(&api::CommandInput {
            name: "edges".into(),
            value: api::CommandInputValue::Edge(api::ResolvedEdge {
                path: vec![ComponentId(Id(3)), ComponentId(Id(4))],
                name: edge.clone(),
                curve,
                face_normals: vec![DVec3::Z, DVec3::new(0.0, 0.6, 0.8)],
            }),
        });
    }

    let si = api::Expression::Si(Quantity::si(QuantityKind::Length, 0.004));
    let text = api::Expression::Text("width / 2".into());
    let feature = api::FeatureDraft {
        id: FeatureId(Id(21)),
        type_id: "core.extrude".into(),
        name: "Head".into(),
        params: vec![
            api::NamedExpression {
                name: "depth".into(),
                expression: si.clone(),
            },
            api::NamedExpression {
                name: "offset".into(),
                expression: text.clone(),
            },
        ],
        choices: vec![api::ChoiceValue {
            name: "mode".into(),
            value: "new".into(),
        }],
        inputs: vec![
            api::NamedInput {
                name: "profile".into(),
                refs: vec![Ref::Feature(FeatureId(Id(22)))],
                many: false,
            },
            api::NamedInput {
                name: "edges".into(),
                refs: vec![Ref::Topo(edge.clone()), Ref::Topo(edge.clone())],
                many: true,
            },
        ],
    };
    let component = api::ComponentDraft {
        id: ComponentId(Id(23)),
        name: "M3x10 1".into(),
        of: api::ComponentOf::Part(PartId(Id(24))),
        pose: Some(api::Pose {
            position: [si.clone(), text.clone(), si.clone()],
            rpy: [text.clone(), si.clone(), text.clone()],
        }),
    };
    let anchor = |path: Vec<ComponentId>, origin| api::Anchor {
        path,
        axis: edge.clone(),
        origin,
    };
    let joint = api::JointDraft {
        id: JointId(Id(25)),
        name: "M3x10 1 fixed".into(),
        kind: api::JointKind::Fixed,
        parent: anchor(vec![ComponentId(Id(3))], None),
        child: anchor(vec![ComponentId(Id(23))], Some(edge.clone())),
        value: si.clone(),
        offset: text.clone(),
        limits: Some([si.clone(), text.clone()]),
        flip: true,
    };
    let group = api::CommandGroup {
        label: "Place screws".into(),
        commands: vec![
            api::PluginCommand::AddPart(api::PartDraft {
                id: PartId(Id(24)),
                name: "M3x10".into(),
                features: vec![feature.clone()],
                material: Some(api::MaterialId(Id(26))),
            }),
            api::PluginCommand::AddFeature(api::AddFeature {
                part: PartId(Id(24)),
                at: Some(1),
                feature,
            }),
            api::PluginCommand::AddComponent(api::ComponentChange {
                assembly: AssemblyId(Id(27)),
                component: component.clone(),
            }),
            api::PluginCommand::EditComponent(api::ComponentChange {
                assembly: AssemblyId(Id(27)),
                component: api::ComponentDraft {
                    pose: None,
                    ..component
                },
            }),
            api::PluginCommand::AddJoint(api::JointChange {
                assembly: AssemblyId(Id(27)),
                joint: joint.clone(),
            }),
            api::PluginCommand::EditJoint(api::JointChange {
                assembly: AssemblyId(Id(27)),
                joint: api::JointDraft {
                    limits: None,
                    flip: false,
                    ..joint
                },
            }),
            api::PluginCommand::AddMaterial(api::MaterialDraft {
                id: api::MaterialId(Id(26)),
                name: "Steel".into(),
                density: Some(text),
                color: api::Rgb {
                    red: 0.5,
                    green: 0.5,
                    blue: 0.5,
                },
            }),
            api::PluginCommand::AssignMaterial(api::AssignMaterial {
                part: PartId(Id(24)),
                body: Some(api::BodyRef {
                    feature: FeatureId(Id(21)),
                    slot: SlotName::new("body").unwrap(),
                }),
                material: None,
            }),
        ],
    };
    round_trip(&group);
}

#[test]
fn a_feature_behaves_the_same_through_the_world() {
    use api::Feature;

    assert_eq!(Imported.describe(), Offset.describe());

    let params = [api::ParamValue {
        name: "distance".into(),
        value: Quantity::si(QuantityKind::Length, 0.002),
    }];
    let inputs = [api::ResolvedInput {
        name: "plane".into(),
        value: api::InputValue::Plane(tilted()),
    }];
    let mut kernel = WitKernel::default();
    for side in ["above", "below"] {
        let choices = [api::ChoiceValue {
            name: "side".into(),
            value: side.into(),
        }];
        for type_id in ["test.offset", "test.other"] {
            let direct = Offset.evaluate(&mut kernel, type_id, &params, &choices, &inputs);
            let through = Imported.evaluate(&mut kernel, type_id, &params, &choices, &inputs);
            assert_eq!(through, direct, "{type_id} {side}");
        }
    }
}

#[test]
fn an_exporter_behaves_the_same_through_the_world() {
    use api::Exporter;

    assert_eq!(ImportedDump.exporters(), Dump.exporters());
    // A plugin that names no feature or exporter is empty, and refuses a call.
    assert!(<DumpExported as wf::Guest>::describe().is_empty());
    assert!(<OffsetExported as wx::Guest>::exporters().is_empty());

    let params = [api::ParamValue {
        name: "scale".into(),
        value: Quantity::si(QuantityKind::Ratio, 2.0),
    }];
    let choices = [api::ChoiceValue {
        name: "unit".into(),
        value: "metre".into(),
    }];
    let subject = api::Subject::Assembly(AssemblyId(Id(3)));
    // `Dump` reads no snapshot, so any will do for the direct call.
    struct Nothing;
    impl api::Snapshot for Nothing {
        fn parts(&mut self) -> Vec<api::PartInfo> {
            Vec::new()
        }
        fn mesh(&mut self, _: PartId, _: &api::BodyRef) -> Result<api::Triangles, Diagnostic> {
            unreachable!()
        }
        fn mass(&mut self, _: PartId) -> Result<api::Inertial, Diagnostic> {
            unreachable!()
        }
        fn assemblies(&mut self) -> Vec<api::AssemblyInfo> {
            Vec::new()
        }
    }
    for id in ["test.dump", "test.other"] {
        let direct = Dump.export(&mut Nothing, id, &subject, &params, &choices);
        let through = ImportedDump.export(&mut Nothing, id, &subject, &params, &choices);
        assert_eq!(through, direct, "{id}");
    }
    let refused = <OffsetExported as wx::Guest>::run(
        "test.dump".into(),
        subject.to_wit(),
        params.to_vec().to_wit(),
        choices.to_vec().to_wit(),
    );
    assert_eq!(refused.unwrap_err().code, "plugin.no-exporter");
}

#[test]
fn a_command_behaves_the_same_through_the_world() {
    use api::Commands;

    assert_eq!(ImportedPlace.commands(), Place.commands());
    // A plugin that names no commands is empty, and refuses a call.
    assert!(<PlaceExported as wf::Guest>::describe().is_empty());
    assert!(<OffsetExported as wc::Guest>::commands().is_empty());

    struct Nothing;
    impl api::Snapshot for Nothing {
        fn parts(&mut self) -> Vec<api::PartInfo> {
            Vec::new()
        }
        fn mesh(&mut self, _: PartId, _: &api::BodyRef) -> Result<api::Triangles, Diagnostic> {
            unreachable!()
        }
        fn mass(&mut self, _: PartId) -> Result<api::Inertial, Diagnostic> {
            unreachable!()
        }
        fn assemblies(&mut self) -> Vec<api::AssemblyInfo> {
            Vec::new()
        }
    }
    let edge = |n: u64| api::CommandInput {
        name: "edges".into(),
        value: api::CommandInputValue::Edge(api::ResolvedEdge {
            path: vec![ComponentId(Id(n))],
            name: format!("edge:revolve.{}.side.{}", Id(7), Id(n))
                .parse()
                .unwrap(),
            curve: api::EdgeCurve::Other,
            face_normals: vec![DVec3::Z],
        }),
    };
    let params = [api::ParamValue {
        name: "size".into(),
        value: Quantity::si(QuantityKind::Length, 0.003),
    }];
    let ids: Vec<Id> = (100..108).map(Id).collect();
    let assembly = api::Context::Assembly(AssemblyId(Id(5)));
    let cases = [
        ("test.place", assembly, vec![edge(1), edge(2)]),
        ("test.place", api::Context::Part(PartId(Id(5))), vec![]),
        ("test.place", assembly, vec![edge(1); 5]),
        ("test.other", assembly, vec![]),
    ];
    for (command, context, inputs) in &cases {
        let direct = Place.expand(&mut Nothing, command, context, &params, &[], inputs, &ids);
        let through =
            ImportedPlace.expand(&mut Nothing, command, context, &params, &[], inputs, &ids);
        assert_eq!(through, direct, "{command} {context:?}");
    }
    let refused = <OffsetExported as wc::Guest>::expand(
        "test.place".into(),
        assembly.to_wit(),
        vec![],
        vec![],
        vec![],
        vec![],
    );
    assert_eq!(refused.unwrap_err().code, "plugin.no-command");
}

/// Every type and function of the world, by interface. Each is covered
/// above: a type by a `Mirror` impl (or by `output_*_wit` for outputs,
/// `WitKernel` for `body`), a function by `WitKernel` or `Exported`.
const COVERED: &[(&str, &[&str], &[&str])] = &[
    (
        "types",
        &[
            "id",
            "vec2",
            "vec3",
            "frame",
            "quantity-kind",
            "quantity",
            "persistent-ref",
            "slot-ref",
            "sketch-ref",
            "region-ref",
            "plugin-ref",
            "reference",
            "severity",
            "diagnostic",
            "line-segment",
            "arc-segment",
            "profile-segment",
            "circle-loop",
            "path-loop",
            "profile-loop",
            "profile",
            "mass-properties",
            "axis",
            "point",
            "motion",
            "rgb",
            "body-ref",
            "joint-kind",
            "component-of",
            "param-value",
            "choice-value",
        ],
        &[],
    ),
    (
        "kernel",
        &["body"],
        &[
            "extrude",
            "revolve",
            "names",
            "face-frame",
            "measure",
            "fuse",
            "cut",
            "common",
            "fillet",
            "chamfer",
            "copy",
            "mirror",
        ],
    ),
    (
        "feature",
        &[
            "shown-when",
            "param-spec",
            "choice-option",
            "choice-spec",
            "input-kind",
            "input-spec",
            "slot-kind",
            "slot-spec",
            "feature-type-spec",
            "input-value",
            "resolved-input",
            "tool-op",
            "tool",
            "output-value",
            "slot-output",
            "feature-output",
        ],
        &["describe", "evaluate"],
    ),
    (
        "document",
        &[
            "material-info",
            "body-info",
            "feature-info",
            "part-info",
            "triangles",
            "inertia-tensor",
            "inertial",
            "component-info",
            "joint-placement",
            "joint-state",
            "joint-info",
            "assembly-info",
        ],
        &["parts", "mesh", "mass", "assemblies"],
    ),
    (
        "exchange",
        &["subject-kind", "subject", "exporter-spec", "file"],
        &["exporters", "run"],
    ),
    (
        "commands",
        &[
            "command-spec",
            "context",
            "circle-edge",
            "line-edge",
            "edge-curve",
            "resolved-edge",
            "command-input-value",
            "command-input",
            "expression",
            "named-expression",
            "named-input",
            "feature-draft",
            "part-draft",
            "add-feature",
            "pose",
            "component-draft",
            "component-change",
            "anchor",
            "joint-draft",
            "joint-change",
            "material-draft",
            "assign-material",
            "plugin-command",
            "command-group",
        ],
        &["commands", "expand"],
    ),
];

#[test]
fn the_world_is_exactly_what_is_covered() {
    let mut resolve = wit_parser::Resolve::default();
    let wit = Path::new(env!("CARGO_MANIFEST_DIR")).join("wit");
    let (package, _) = resolve.push_dir(&wit).unwrap();
    let pkg = &resolve.packages[package];
    assert_eq!(pkg.name.namespace, "arrix");
    assert_eq!(pkg.name.name, "plugin");
    assert_eq!(
        pkg.name
            .version
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
        Some(api::API_VERSION),
        "the WIT package's version is the crate's"
    );
    assert_eq!(
        std::fs::read_to_string(wit.join("arrix-plugin.wit")).unwrap(),
        api::WIT
    );

    let world = &resolve.worlds[pkg.worlds["plugin"]];
    let names = |items: &wit_parser::IndexMap<_, wit_parser::WorldItem>| {
        items
            .keys()
            .map(|k| resolve.name_world_key(k))
            .collect::<BTreeSet<_>>()
    };
    // `kernel` and `document` are imported for their own sake; `types` only
    // through the `use`s.
    let v = api::API_VERSION;
    assert_eq!(
        names(&world.imports),
        BTreeSet::from(["kernel", "document", "types"].map(|i| format!("arrix:plugin/{i}@{v}")))
    );
    assert_eq!(
        names(&world.exports),
        BTreeSet::from(
            ["feature", "exchange", "commands"].map(|i| format!("arrix:plugin/{i}@{v}"))
        )
    );

    let mut found = BTreeSet::new();
    for (_, iface) in pkg.interfaces.iter() {
        let iface = &resolve.interfaces[*iface];
        let name = iface.name.clone().unwrap();
        // A `use` makes an alias of the used type; it is covered where it
        // is defined.
        for (ty, id) in &iface.types {
            let used = matches!(
                resolve.types[*id].kind,
                wit_parser::TypeDefKind::Type(wit_parser::Type::Id(_))
            );
            if !used {
                found.insert((name.clone(), "type", ty.clone()));
            }
        }
        for f in iface.functions.keys() {
            found.insert((name.clone(), "func", f.clone()));
        }
    }
    let covered = COVERED
        .iter()
        .flat_map(|(iface, types, funcs)| {
            let t = types
                .iter()
                .map(move |t| (iface.to_string(), "type", t.to_string()));
            let f = funcs
                .iter()
                .map(move |f| (iface.to_string(), "func", f.to_string()));
            t.chain(f)
        })
        .collect::<BTreeSet<_>>();
    let uncovered: Vec<_> = found.difference(&covered).collect();
    let gone: Vec<_> = covered.difference(&found).collect();
    assert!(
        uncovered.is_empty() && gone.is_empty(),
        "in the world but not covered here: {uncovered:?}; covered here but not in the world: {gone:?}"
    );

    // The wire carries every item the world has, and nothing else.
    let wire = Path::new(env!("CARGO_MANIFEST_DIR")).join("wire");
    let read = |file: &str| -> serde_json::Value {
        serde_json::from_str(&std::fs::read_to_string(wire.join(file)).unwrap()).unwrap()
    };
    let schema_types: BTreeSet<String> = read("schema.json")["$defs"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    let world_types: BTreeSet<String> = found
        .iter()
        .filter(|(_, kind, _)| *kind == "type")
        .map(|(_, _, name)| name.clone())
        .collect();
    assert_eq!(schema_types, world_types, "wire/schema.json and the world");
    let wire_methods: BTreeSet<String> = read("methods.json")["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["method"].as_str().unwrap().to_owned())
        .collect();
    let world_methods: BTreeSet<String> = found
        .iter()
        .filter(|(_, kind, _)| *kind == "func")
        .map(|(iface, _, name)| format!("{iface}.{name}"))
        .collect();
    assert_eq!(
        wire_methods, world_methods,
        "wire/methods.json and the world"
    );
}
