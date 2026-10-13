//! The generated codec against the generated schemas (ADR-0022): every
//! type of the world, every case of every variant and enum, encoded,
//! checked against its schema, decoded and compared; then the ways a value
//! can be wrong, each refused with where.

mod support;

use std::collections::BTreeSet;
use std::fmt::Debug;

use arrix_core::{
    AssemblyId, ComponentId, CurveKey, FeatureId, Frame, Id, JointId, ParamId, PartId,
    PersistentName, PluginId, Profile, ProfileLoop, ProfileSegment, RecordId, RegionKey,
    SketchEntityId, SlotName,
};
use arrix_plugin_api::{
    Axis, Body, ChoiceOption, ChoiceSpec, ChoiceValue, DVec2, DVec3, Diagnostic, FeatureOutput,
    FeatureTypeSpec, InputKind, InputSpec, InputValue, MassProperties, Motion, OutputValue,
    ParamSpec, ParamValue, Point, Quantity, QuantityKind, Ref, ResolvedInput, Severity, ShownWhen,
    SlotKind, SlotOutput, SlotSpec, Tool, ToolOp,
};
use arrix_plugin_host::wire::{self, Wire, WireError};
use serde_json::{Value, json};
use support::{Check, Schemas};

fn id(n: u64) -> Id {
    Id(n)
}

fn feature(n: u64) -> FeatureId {
    FeatureId(id(n))
}

fn key(n: u64) -> CurveKey {
    CurveKey(id(n))
}

fn v2(x: f64, y: f64) -> DVec2 {
    DVec2::new(x, y)
}

fn v3(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z)
}

fn slot(name: &str) -> SlotName {
    SlotName::new(name).unwrap()
}

fn name(text: &str) -> PersistentName {
    text.parse().unwrap()
}

fn tilted() -> Frame {
    Frame::new(v3(0.1, -0.2, 0.3), v3(0.0, 1.0, 0.0), v3(1.0, 0.0, 0.0)).unwrap()
}

fn path_loop() -> ProfileLoop {
    ProfileLoop::Path {
        start: v2(0.0, 0.0),
        segments: vec![
            ProfileSegment::Line {
                key: key(1),
                to: v2(0.04, 0.0),
            },
            ProfileSegment::Arc {
                key: key(2),
                to: v2(0.04, 0.02),
                via: v2(0.05, 0.01),
            },
            ProfileSegment::Line {
                key: key(3),
                to: v2(0.0, 0.0),
            },
        ],
    }
}

fn circle(n: u64) -> ProfileLoop {
    ProfileLoop::Circle {
        key: key(n),
        center: v2(0.01, 0.01),
        radius: 0.005,
    }
}

fn profile() -> Profile {
    Profile::new(tilted(), path_loop(), vec![circle(7)]).unwrap()
}

fn refs() -> Vec<Ref> {
    vec![
        Ref::Param(ParamId(id(1))),
        Ref::Feature(feature(2)),
        Ref::Slot {
            feature: feature(3),
            slot: slot("body"),
        },
        Ref::Topo(name("face:sweep.0000000000001.3.side.0000000000002")),
        Ref::Sketch {
            feature: feature(4),
            entity: SketchEntityId(id(5)),
        },
        Ref::Region {
            feature: feature(6),
            key: RegionKey {
                entities: vec![SketchEntityId(id(7)), SketchEntityId(id(8))],
                sample: [-1_500_000, 2_500_000],
            },
        },
        Ref::Plugin {
            plugin: PluginId::new("demo").unwrap(),
            record: RecordId(id(9)),
        },
    ]
}

fn diagnostic() -> Diagnostic {
    Diagnostic::new(Severity::Warning, "kernel.degenerate", "a sliver")
        .unwrap()
        .with_refs(refs())
        .with_candidates([Ref::Feature(feature(1))])
}

fn shown() -> Option<ShownWhen> {
    Some(ShownWhen {
        choice: "mode".into(),
        values: vec!["a".into(), "b".into()],
    })
}

fn spec() -> FeatureTypeSpec {
    FeatureTypeSpec {
        id: "demo.disc".into(),
        version: 2,
        title: "Disc".into(),
        params: vec![
            ParamSpec {
                name: "radius".into(),
                title: "Radius".into(),
                kind: QuantityKind::Length,
                default: "10 mm".into(),
                shown_when: None,
            },
            ParamSpec {
                name: "count".into(),
                title: "Count".into(),
                kind: QuantityKind::Count,
                default: "3".into(),
                shown_when: shown(),
            },
        ],
        choices: vec![
            ChoiceSpec {
                name: "mode".into(),
                title: "Mode".into(),
                options: vec![ChoiceOption {
                    name: "a".into(),
                    title: "A".into(),
                }],
                default: Some("a".into()),
            },
            ChoiceSpec {
                name: "free".into(),
                title: "Free".into(),
                options: vec![],
                default: None,
            },
        ],
        inputs: vec![InputSpec {
            name: "plane".into(),
            title: "Plane".into(),
            kind: InputKind::Plane,
            list: true,
            shown_when: shown(),
        }],
        outputs: vec![
            SlotSpec {
                name: slot("body"),
                kind: SlotKind::Body,
                modifies: Some("target".into()),
            },
            SlotSpec {
                name: slot("plane"),
                kind: SlotKind::Plane,
                modifies: None,
            },
        ],
    }
}

fn inputs() -> Vec<InputValue> {
    vec![
        InputValue::Plane(tilted()),
        InputValue::Region(profile()),
        InputValue::Body(Body::from_handle(4)),
        InputValue::Edge(name("edge:sweep.0000000000001.end-edge.0000000000002")),
        InputValue::Point(Point {
            at: v3(1.0, 2.0, 3.0),
            normal: Some(v3(0.0, 0.0, 1.0)),
        }),
        InputValue::Axis(Axis {
            origin: v3(0.0, 0.0, 0.0),
            direction: v3(0.0, 0.0, 1.0),
        }),
        InputValue::Tool(Tool {
            body: Body::from_handle(5),
            op: ToolOp::Cut,
        }),
    ]
}

/// One value of a type: encoded, checked against the schema, decoded.
struct Round<'a> {
    check: Check<'a>,
}

impl Round<'_> {
    fn go<T: PartialEq + Debug>(
        &mut self,
        ty: &str,
        value: &T,
        to: fn(&T) -> Wire<Value>,
        from: fn(&Value) -> Wire<T>,
    ) {
        let json = to(value).unwrap_or_else(|e| panic!("{ty}: {e}"));
        if let Err(e) = self.check.type_ok(ty, &json) {
            panic!("{ty} breaks its schema: {e}\n{json}");
        }
        let back = from(&json).unwrap_or_else(|e| panic!("{ty}: {e}\n{json}"));
        assert_eq!(&back, value, "{ty} round trip\n{json}");
        assert_eq!(to(&back).unwrap(), json, "{ty} encodes the same again");
    }
}

#[test]
fn every_type_and_case_round_trips_and_keeps_its_schema() {
    let schemas = Schemas::load();
    let mut r = Round {
        check: Check::new(&schemas),
    };

    r.go("id", &id(0x0123_4567_89ab_cdef), wire::id_to, wire::id_from);
    r.go("body", &Body::from_handle(9), wire::handle_to, |v| {
        wire::handle_from(v).map(Body::from_handle)
    });
    r.go(
        "vec2",
        &v2(1.5, -2.0),
        wire::vec2_to_json,
        wire::vec2_from_json,
    );
    r.go(
        "vec3",
        &v3(1.5, -2.0, 0.0),
        wire::vec3_to_json,
        wire::vec3_from_json,
    );
    r.go(
        "frame",
        &tilted(),
        wire::frame_to_json,
        wire::frame_from_json,
    );
    for k in [
        QuantityKind::Length,
        QuantityKind::Angle,
        QuantityKind::Count,
        QuantityKind::Ratio,
        QuantityKind::Mass,
        QuantityKind::Density,
    ] {
        r.go(
            "quantity-kind",
            &k,
            wire::quantity_kind_to_json,
            wire::quantity_kind_from_json,
        );
        r.go(
            "quantity",
            &Quantity::si(k, 0.012),
            wire::quantity_to_json,
            wire::quantity_from_json,
        );
    }
    for p in [
        name("face:sweep.0000000000001.3.side.0000000000002"),
        name("vertex:sweep.0000000000001.start-vertex.0000000000002/mod.0000000000003.1"),
    ] {
        r.go(
            "persistent-ref",
            &p,
            wire::persistent_ref_to_json,
            wire::persistent_ref_from_json,
        );
    }
    for reference in refs() {
        r.go(
            "reference",
            &reference,
            wire::reference_to_json,
            wire::reference_from_json,
        );
    }
    for s in [Severity::Info, Severity::Warning, Severity::Error] {
        r.go(
            "severity",
            &s,
            wire::severity_to_json,
            wire::severity_from_json,
        );
    }
    r.go(
        "diagnostic",
        &diagnostic(),
        wire::diagnostic_to_json,
        wire::diagnostic_from_json,
    );
    let bare = Diagnostic::new(Severity::Error, "plugin.crash", "gone").unwrap();
    r.go(
        "diagnostic",
        &bare,
        wire::diagnostic_to_json,
        wire::diagnostic_from_json,
    );

    for segment in match path_loop() {
        ProfileLoop::Path { segments, .. } => segments,
        _ => unreachable!(),
    } {
        r.go(
            "profile-segment",
            &segment,
            wire::profile_segment_to_json,
            wire::profile_segment_from_json,
        );
    }
    for l in [path_loop(), circle(1)] {
        r.go(
            "profile-loop",
            &l,
            wire::profile_loop_to_json,
            wire::profile_loop_from_json,
        );
    }
    r.go(
        "profile",
        &profile(),
        wire::profile_to_json,
        wire::profile_from_json,
    );
    let plain = Profile::new(Frame::WORLD_XY, circle(1), vec![]).unwrap();
    r.go(
        "profile",
        &plain,
        wire::profile_to_json,
        wire::profile_from_json,
    );

    r.go(
        "mass-properties",
        &MassProperties {
            volume: 1e-6,
            area: 6e-4,
            centroid: v3(0.0, 0.0, 0.005),
        },
        wire::mass_properties_to_json,
        wire::mass_properties_from_json,
    );
    r.go(
        "axis",
        &Axis {
            origin: v3(0.0, 1.0, 0.0),
            direction: v3(0.0, 0.0, 1.0),
        },
        wire::axis_to_json,
        wire::axis_from_json,
    );
    for normal in [Some(v3(0.0, 0.0, 1.0)), None] {
        r.go(
            "point",
            &Point {
                at: v3(1.0, 2.0, 3.0),
                normal,
            },
            wire::point_to_json,
            wire::point_from_json,
        );
    }
    r.go(
        "motion",
        &Motion {
            translation: v3(0.1, 0.0, 0.0),
            axis: v3(0.0, 0.0, 1.0),
            angle: 0.5,
        },
        wire::motion_to_json,
        wire::motion_from_json,
    );

    r.go(
        "shown-when",
        &shown().unwrap(),
        wire::shown_when_to_json,
        wire::shown_when_from_json,
    );
    r.go(
        "choice-option",
        &ChoiceOption {
            name: "a".into(),
            title: "A".into(),
        },
        wire::choice_option_to_json,
        wire::choice_option_from_json,
    );
    let s = spec();
    for p in &s.params {
        r.go(
            "param-spec",
            p,
            wire::param_spec_to_json,
            wire::param_spec_from_json,
        );
    }
    for c in &s.choices {
        r.go(
            "choice-spec",
            c,
            wire::choice_spec_to_json,
            wire::choice_spec_from_json,
        );
    }
    for i in &s.inputs {
        r.go(
            "input-spec",
            i,
            wire::input_spec_to_json,
            wire::input_spec_from_json,
        );
    }
    for o in &s.outputs {
        r.go(
            "slot-spec",
            o,
            wire::slot_spec_to_json,
            wire::slot_spec_from_json,
        );
    }
    for k in [
        InputKind::Plane,
        InputKind::Region,
        InputKind::Body,
        InputKind::Edge,
        InputKind::Point,
        InputKind::Axis,
        InputKind::Tool,
    ] {
        r.go(
            "input-kind",
            &k,
            wire::input_kind_to_json,
            wire::input_kind_from_json,
        );
    }
    for k in [SlotKind::Body, SlotKind::Plane] {
        r.go(
            "slot-kind",
            &k,
            wire::slot_kind_to_json,
            wire::slot_kind_from_json,
        );
    }
    r.go(
        "feature-type-spec",
        &s,
        wire::feature_type_spec_to_json,
        wire::feature_type_spec_from_json,
    );

    r.go(
        "param-value",
        &ParamValue {
            name: "radius".into(),
            value: Quantity::si(QuantityKind::Length, 0.01),
        },
        wire::param_value_to_json,
        wire::param_value_from_json,
    );
    r.go(
        "choice-value",
        &ChoiceValue {
            name: "mode".into(),
            value: "a".into(),
        },
        wire::choice_value_to_json,
        wire::choice_value_from_json,
    );
    for op in [ToolOp::Fuse, ToolOp::Cut] {
        r.go(
            "tool-op",
            &op,
            wire::tool_op_to_json,
            wire::tool_op_from_json,
        );
    }
    let tool = || Tool {
        body: Body::from_handle(5),
        op: ToolOp::Fuse,
    };
    r.go("tool", &tool(), wire::tool_to_json, wire::tool_from_json);
    for (i, value) in inputs().into_iter().enumerate() {
        r.go(
            "input-value",
            &value,
            wire::input_value_to_json,
            wire::input_value_from_json,
        );
        let resolved = ResolvedInput {
            name: format!("in{i}"),
            value,
        };
        r.go(
            "resolved-input",
            &resolved,
            wire::resolved_input_to_json,
            wire::resolved_input_from_json,
        );
    }
    let outputs = [
        OutputValue::Body(Body::from_handle(1)),
        OutputValue::Plane(tilted()),
    ];
    for value in outputs {
        r.go(
            "output-value",
            &value,
            wire::output_value_to_json,
            wire::output_value_from_json,
        );
        let out = SlotOutput {
            name: slot("body"),
            value,
        };
        r.go(
            "slot-output",
            &out,
            wire::slot_output_to_json,
            wire::slot_output_from_json,
        );
    }
    r.go(
        "feature-output",
        &FeatureOutput {
            slots: vec![SlotOutput {
                name: slot("body"),
                value: OutputValue::Body(Body::from_handle(2)),
            }],
            tool: Some(tool()),
        },
        wire::feature_output_to_json,
        wire::feature_output_from_json,
    );
    r.go(
        "feature-output",
        &FeatureOutput::default(),
        wire::feature_output_to_json,
        wire::feature_output_from_json,
    );

    document_samples(&mut r);

    let missing: BTreeSet<_> = schemas
        .expected_coverage()
        .difference(&r.check.seen)
        .cloned()
        .collect();
    assert!(
        missing.is_empty(),
        "the samples never reach these types or cases of the world: {missing:?}"
    );
}

/// The `document` and `exchange` types: runs of `f32`, `u32` and `u8`
/// among them.
fn document_samples(r: &mut Round) {
    use arrix_plugin_api as api;
    let body = api::BodyRef {
        feature: feature(3),
        slot: slot("body"),
    };
    r.go(
        "body-ref",
        &body,
        wire::body_ref_to_json,
        wire::body_ref_from_json,
    );
    let material = api::MaterialInfo {
        name: "Steel".into(),
        color: api::Rgb {
            red: 0.5,
            green: 0.25,
            blue: 1.0,
        },
    };
    r.go(
        "rgb",
        &material.color,
        wire::rgb_to_json,
        wire::rgb_from_json,
    );
    r.go(
        "material-info",
        &material,
        wire::material_info_to_json,
        wire::material_info_from_json,
    );
    for material in [Some(material), None] {
        r.go(
            "body-info",
            &api::BodyInfo {
                body: body.clone(),
                material,
            },
            wire::body_info_to_json,
            wire::body_info_from_json,
        );
    }
    r.go(
        "part-info",
        &api::PartInfo {
            id: PartId(id(7)),
            name: "Bracket".into(),
            bodies: vec![],
        },
        wire::part_info_to_json,
        wire::part_info_from_json,
    );
    for mesh in [
        api::Triangles::default(),
        api::Triangles {
            positions: vec![0.0, 0.0, 0.0, 1e-3, 0.0, 0.0, 0.0, 1e-3, -2.5],
            normals: vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
            indices: vec![0, 1, 2, u32::MAX],
        },
    ] {
        r.go(
            "triangles",
            &mesh,
            wire::triangles_to_json,
            wire::triangles_from_json,
        );
    }
    let tensor = api::InertiaTensor {
        xx: 1e-6,
        xy: -1e-8,
        xz: 2e-8,
        yy: 2e-6,
        yz: -3e-8,
        zz: 3e-6,
    };
    r.go(
        "inertia-tensor",
        &tensor,
        wire::inertia_tensor_to_json,
        wire::inertia_tensor_from_json,
    );
    r.go(
        "inertial",
        &api::Inertial {
            mass: 0.125,
            centroid: v3(0.01, 0.0, -0.002),
            inertia: tensor,
        },
        wire::inertial_to_json,
        wire::inertial_from_json,
    );
    for kind in [
        api::JointKind::Fixed,
        api::JointKind::Revolute,
        api::JointKind::Prismatic,
    ] {
        r.go(
            "joint-kind",
            &kind,
            wire::joint_kind_to_json,
            wire::joint_kind_from_json,
        );
    }
    let component = ComponentId(id(14));
    for of in [
        api::ComponentOf::Part(PartId(id(1))),
        api::ComponentOf::Assembly(AssemblyId(id(2))),
    ] {
        r.go(
            "component-of",
            &of,
            wire::component_of_to_json,
            wire::component_of_from_json,
        );
        r.go(
            "component-info",
            &api::ComponentInfo {
                path: vec![component, ComponentId(id(15))],
                name: "arm".into(),
                of,
                pose: tilted(),
                diagnostics: vec![diagnostic()],
            },
            wire::component_info_to_json,
            wire::component_info_from_json,
        );
    }
    let placement = api::JointPlacement {
        parent_anchor: tilted(),
        child_anchor: Frame::WORLD_ZX,
        value: 0.5,
        diagnostics: vec![],
    };
    r.go(
        "joint-placement",
        &placement,
        wire::joint_placement_to_json,
        wire::joint_placement_from_json,
    );
    let states = [
        api::JointState::Placed(placement),
        api::JointState::Failed(diagnostic()),
    ];
    for state in &states {
        r.go(
            "joint-state",
            state,
            wire::joint_state_to_json,
            wire::joint_state_from_json,
        );
    }
    let joints: Vec<_> = states
        .into_iter()
        .enumerate()
        .map(|(i, state)| api::JointInfo {
            id: JointId(id(16 + i as u64)),
            name: "hinge".into(),
            kind: api::JointKind::Revolute,
            parent: vec![],
            child: vec![component],
            limits: (i == 0).then_some([-1.5, 1.5]),
            state,
        })
        .collect();
    for joint in &joints {
        r.go(
            "joint-info",
            joint,
            wire::joint_info_to_json,
            wire::joint_info_from_json,
        );
    }
    r.go(
        "assembly-info",
        &api::AssemblyInfo {
            id: AssemblyId(id(2)),
            name: "Arm".into(),
            components: vec![],
            joints,
        },
        wire::assembly_info_to_json,
        wire::assembly_info_from_json,
    );

    for kind in [api::SubjectKind::Part, api::SubjectKind::Assembly] {
        r.go(
            "subject-kind",
            &kind,
            wire::subject_kind_to_json,
            wire::subject_kind_from_json,
        );
    }
    for subject in [
        api::Subject::Part(PartId(id(1))),
        api::Subject::Assembly(AssemblyId(id(2))),
    ] {
        r.go(
            "subject",
            &subject,
            wire::subject_to_json,
            wire::subject_from_json,
        );
    }
    r.go(
        "exporter-spec",
        &api::ExporterSpec {
            id: "robotics.urdf".into(),
            title: "URDF".into(),
            extension: "urdf".into(),
            subject: api::SubjectKind::Assembly,
            params: vec![],
            choices: vec![],
        },
        wire::exporter_spec_to_json,
        wire::exporter_spec_from_json,
    );
    for bytes in [vec![], vec![0, 1, 254, 255]] {
        r.go(
            "file",
            &api::ExportFile {
                name: "meshes/a.stl".into(),
                bytes,
            },
            wire::file_to_json,
            wire::file_from_json,
        );
    }
}

#[test]
fn the_schemas_refuse_what_the_codec_would() {
    let schemas = Schemas::load();
    let mut c = Check::new(&schemas);
    for (ty, bad) in [
        ("vec2", json!({"x": 1.0})),
        ("vec2", json!({"x": 1.0, "y": 2.0, "z": 3.0})),
        ("vec2", json!([1.0, 2.0])),
        ("id", json!("not an id")),
        ("id", json!("ZZZZZZZZZZZZZ")),
        ("severity", json!("fatal")),
        (
            "reference",
            json!({"feature": "0000000000001", "param": "0000000000001"}),
        ),
        ("reference", json!({"nothing": null})),
        ("body", json!(-1)),
        ("body", json!(4294967296u64)),
        (
            "input-spec",
            json!({"name": "a", "title": "A", "kind": "plane", "list": "yes"}),
        ),
    ] {
        assert!(c.type_ok(ty, &bad).is_err(), "{ty} takes {bad}");
    }
    // An option field may be left out.
    assert!(
        c.type_ok("point", &json!({"at": {"x": 0, "y": 0, "z": 0}}))
            .is_ok()
    );
}

fn refused<T: Debug>(result: Wire<T>) -> WireError {
    result.expect_err("the codec refuses this")
}

#[test]
fn a_value_that_is_wrong_is_refused_with_where() {
    let e = refused(wire::vec2_from_json(&json!({"x": 1.0})));
    assert_eq!(
        (e.path().as_str(), e.message.as_str()),
        ("", "missing field `y`")
    );

    let e = refused(wire::vec2_from_json(&json!({"x": 1.0, "y": "two"})));
    assert_eq!(e.path(), "y");
    assert!(e.message.contains("expected a number"), "{e}");

    let e = refused(wire::vec2_from_json(&json!({"x": 1.0, "y": 2.0, "w": 0})));
    assert!(e.message.contains("`w` is no field"), "{e}");

    let e = refused(wire::vec2_from_json(&json!(3)));
    assert!(e.message.contains("expected an object"), "{e}");

    let e = refused(wire::severity_from_json(&json!("fatal")));
    assert!(e.message.contains("no case of severity"), "{e}");

    let e = refused(wire::reference_from_json(
        &json!({"feature": "0", "param": "0"}),
    ));
    assert!(e.message.contains("exactly one key"), "{e}");

    let e = refused(wire::reference_from_json(&json!({"nothing": null})));
    assert!(e.message.contains("no case of reference"), "{e}");

    // Where, through a list of loops in a profile.
    let mut p = wire::profile_to_json(&profile()).unwrap();
    p["holes"][0]["circle"]["radius"] = json!("wide");
    let e = refused(wire::profile_from_json(&p));
    assert_eq!(e.path(), "holes[0].circle.radius");
    assert_eq!(
        e.to_string(),
        format!("holes[0].circle.radius: {}", e.message)
    );
}

#[test]
fn a_value_the_rust_type_refuses_is_refused_by_the_codec() {
    let zero = json!({"x": 0.0, "y": 0.0, "z": 0.0});
    let one_x = json!({"x": 1.0, "y": 0.0, "z": 0.0});
    let e = refused(wire::frame_from_json(
        &json!({"origin": zero, "x-axis": one_x, "z-axis": one_x}),
    ));
    assert_eq!(e.path(), "");
    assert!(e.message.contains("orthogonal"), "{e}");

    let e = refused(wire::frame_from_json(
        &json!({"origin": zero, "x-axis": zero, "z-axis": one_x}),
    ));
    assert!(e.message.contains("zero length"), "{e}");

    let e = refused(wire::slot_spec_from_json(
        &json!({"name": "Body", "kind": "body"}),
    ));
    assert_eq!(e.path(), "name");
    assert!(e.message.contains("not a slot name"), "{e}");

    let diagnostic = |code: &str| json!({"severity": "error", "code": code, "message": "m", "refs": [], "candidates": []});
    assert!(wire::diagnostic_from_json(&diagnostic("ok.code")).is_ok());
    let e = refused(wire::diagnostic_from_json(&diagnostic("Not A Code")));
    assert_eq!(e.path(), "code");
    assert!(e.message.contains("not a diagnostic code"), "{e}");

    let e = refused(wire::persistent_ref_from_json(
        &json!({"encoded": "face:nonsense"}),
    ));
    assert_eq!(e.path(), "encoded");

    let e = refused(wire::reference_from_json(
        &json!({"plugin": {"plugin": "core", "record": "0000000000001"}}),
    ));
    assert_eq!(e.path(), "plugin.plugin");
    assert!(e.message.contains("built-ins"), "{e}");

    // A circle with no radius is the profile's to refuse.
    let mut p = wire::profile_to_json(&profile()).unwrap();
    p["holes"][0]["circle"]["radius"] = json!(-1.0);
    let e = refused(wire::profile_from_json(&p));
    assert!(e.message.contains("radius"), "{e}");

    // Two curves with one key.
    let mut p = wire::profile_to_json(&profile()).unwrap();
    p["holes"][0]["circle"]["key"] = json!(Id(1).to_string());
    let e = refused(wire::profile_from_json(&p));
    assert!(e.message.to_lowercase().contains("key"), "{e}");
}

#[test]
fn numbers_keep_to_what_json_holds_exactly() {
    let region = |sample: Value| json!({"region": {"feature": "0000000000001", "entities": [], "sample": sample}});
    assert!(wire::reference_from_json(&region(json!([1, -1]))).is_ok());
    // A reader whose numbers are doubles writes 3.0 for 3.
    assert!(wire::reference_from_json(&region(json!([1.0, -1.0]))).is_ok());
    for bad in [
        json!([9_007_199_254_740_993i64, 0]),
        json!([0.5, 0]),
        json!([0]),
        json!([0, 0, 0]),
        json!(["0", 0]),
    ] {
        let e = refused(wire::reference_from_json(&region(bad.clone())));
        assert!(e.path().starts_with("region.sample"), "{bad}: {e}");
    }
    let too_big = Ref::Region {
        feature: feature(1),
        key: RegionKey {
            entities: vec![],
            sample: [i64::MAX, 0],
        },
    };
    assert!(
        wire::reference_to_json(&too_big)
            .unwrap_err()
            .message
            .contains("2^53")
    );

    // An option field may be left out.
    assert!(
        wire::param_spec_from_json(&json!({
            "name": "n", "title": "N", "kind": "count", "default": "1"
        }))
        .is_ok()
    );

    assert!(wire::vec2_to_json(&v2(f64::NAN, 0.0)).is_err());
    assert!(wire::vec2_to_json(&v2(f64::INFINITY, 0.0)).is_err());
    let e = wire::vec2_to_json(&v2(0.0, f64::NAN)).unwrap_err();
    assert_eq!(e.path(), "y");
}

#[test]
fn u32s_and_ids_are_checked() {
    assert!(wire::handle_from(&json!(4294967295u64)).is_ok());
    assert!(wire::handle_from(&json!(4294967296u64)).is_err());
    assert!(wire::handle_from(&json!(-1)).is_err());
    // An id reads as Crockford does, and is written canonically.
    let lower: FeatureId = wire::id_from(&json!("0000000000a1b")).unwrap();
    assert_eq!(wire::id_to(&lower).unwrap(), json!("0000000000A1B"));
    for bad in [json!(12), json!("short"), json!("0000000000000U")] {
        assert!(wire::id_from::<Id>(&bad).is_err(), "{bad}");
    }
}

#[test]
fn the_method_table_is_the_worlds() {
    use wire::Direction::{HostToPlugin, PluginToHost};
    let schemas = Schemas::load();
    let table: Vec<(String, &str)> = schemas.methods["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            (
                m["method"].as_str().unwrap().to_owned(),
                m["direction"].as_str().unwrap(),
            )
        })
        .collect();
    let coded: Vec<(String, &str)> = wire::METHODS
        .iter()
        .map(|m| {
            (
                m.name.to_owned(),
                match m.direction {
                    HostToPlugin => "host-to-plugin",
                    PluginToHost => "plugin-to-host",
                },
            )
        })
        .collect();
    assert_eq!(coded, table);

    assert_eq!(
        wire::method("kernel.extrude", PluginToHost).unwrap().name,
        "kernel.extrude"
    );
    assert_eq!(
        wire::method("feature.evaluate", HostToPlugin).unwrap().name,
        "feature.evaluate"
    );
    // The other side's method, and one that is nobody's.
    for (name, side) in [
        ("feature.evaluate", PluginToHost),
        ("kernel.extrude", HostToPlugin),
        ("kernel.nothing", PluginToHost),
    ] {
        let e = wire::method(name, side).unwrap_err();
        assert_eq!(e.code, wire::rpc::METHOD_NOT_FOUND, "{name}");
    }
}
