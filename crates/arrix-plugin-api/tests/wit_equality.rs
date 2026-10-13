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
//!    the Rust `Kernel` is implemented over the world's imports. A method
//!    or function added on either side fails to compile.
//! 3. **Inventory.** The world as parsed is exactly the types and functions
//!    this file covers, so a WIT addition the bindings would silently
//!    absorb (an unused record, a new import) fails here. The generated
//!    wire (`wire/`, ADR-0022) must hold the same inventory, so a generator
//!    that skips an item fails here too.

use std::collections::BTreeSet;
use std::path::Path;

use arrix_plugin_api as api;
use arrix_plugin_api::guest::bindings::exports::arrix::plugin::feature as wf;
use arrix_plugin_api::guest::{Exported, Mirror, Plugin, WitKernel, input_to_wit, output_from_wit};
use arrix_plugin_api::{
    Axis, CurveKey, DVec2, DVec3, Diagnostic, FeatureId, Frame, Id, Motion, ParamId,
    PersistentName, PluginId, Profile, ProfileLoop, ProfileSegment, Quantity, QuantityKind,
    RecordId, Ref, RegionKey, Severity, SketchEntityId, SlotName,
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

impl Plugin for Offset {
    fn feature() -> &'static dyn api::Feature {
        &Offset
    }
}

/// The world's `feature` export over `Offset`, as a Tier 1 guest built by
/// `export_plugin!` is.
type OffsetExported = Exported<Offset>;

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
            "param-value",
            "choice-value",
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
    // `kernel` is imported for its own sake and by `feature`'s `use`;
    // `types` only through the `use`s.
    assert_eq!(
        names(&world.imports),
        BTreeSet::from(["arrix:plugin/kernel@0.6.0", "arrix:plugin/types@0.6.0"].map(String::from))
    );
    assert_eq!(
        names(&world.exports),
        BTreeSet::from(["arrix:plugin/feature@0.6.0".to_owned()])
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
