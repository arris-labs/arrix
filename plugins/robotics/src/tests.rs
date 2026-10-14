//! `robotics.urdf` on hand-built snapshots: no kernel, no document.

use crate::{Robotics, URDF};
use arrix_plugin_api::{
    AssemblyId, AssemblyInfo, BodyInfo, BodyRef, ChoiceValue, ComponentId, ComponentInfo,
    ComponentOf, DVec3, Diagnostic, ExportFile, Exporter, FeatureId, Frame, Id, InertiaTensor,
    Inertial, JointId, JointInfo, JointKind, JointPlacement, JointState, MaterialInfo, PartId,
    PartInfo, Rgb, Severity, SlotName, Snapshot, Subject, Triangles,
};

fn part(n: u64) -> PartId {
    PartId::from(Id(n))
}

fn comp(n: u64) -> ComponentId {
    ComponentId::from(Id(n))
}

fn frame(origin: [f64; 3], angle: f64) -> Frame {
    let (s, c) = arrix_plugin_api::math::sincos(angle);
    Frame::new(
        DVec3::new(origin[0], origin[1], origin[2]),
        DVec3::new(c, s, 0.0),
        DVec3::Z,
    )
    .unwrap()
}

struct World {
    parts: Vec<PartInfo>,
    massless: Vec<PartId>,
    assemblies: Vec<AssemblyInfo>,
}

impl Snapshot for World {
    fn parts(&mut self) -> Vec<PartInfo> {
        self.parts.clone()
    }

    fn mesh(&mut self, _: PartId, _: &BodyRef) -> Result<Triangles, Diagnostic> {
        Ok(Triangles {
            positions: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
            normals: [0.0, 0.0, 1.0].repeat(3),
            indices: vec![0, 1, 2],
        })
    }

    fn mass(&mut self, part: PartId) -> Result<Inertial, Diagnostic> {
        if self.massless.contains(&part) {
            return Err(Diagnostic::new(Severity::Error, "mass.no-density", "no density").unwrap());
        }
        Ok(Inertial {
            mass: 2.0,
            centroid: DVec3::new(0.0, 0.0, 0.05),
            inertia: InertiaTensor {
                xx: 0.1,
                xy: 0.0,
                xz: 0.0,
                yy: 0.2,
                yz: 0.0,
                zz: 0.3,
            },
        })
    }

    fn assemblies(&mut self) -> Vec<AssemblyInfo> {
        self.assemblies.clone()
    }
}

fn part_info(n: u64, name: &str) -> PartInfo {
    PartInfo {
        id: part(n),
        name: name.into(),
        bodies: vec![BodyInfo {
            body: BodyRef {
                feature: FeatureId::from(Id(n)),
                slot: SlotName::new("body").unwrap(),
            },
            material: Some(MaterialInfo {
                name: "Steel".into(),
                color: Rgb {
                    red: 0.5,
                    green: 0.5,
                    blue: 0.5,
                },
            }),
        }],
        features: vec![],
    }
}

fn component(path: &[u64], name: &str, part_n: u64, pose: Frame) -> ComponentInfo {
    ComponentInfo {
        path: path.iter().map(|n| comp(*n)).collect(),
        name: name.into(),
        of: ComponentOf::Part(part(part_n)),
        pose,
        diagnostics: vec![],
    }
}

fn joint(
    n: u64,
    name: &str,
    kind: JointKind,
    ends: (&[u64], &[u64]),
    limits: Option<[f64; 2]>,
    anchors: (Frame, Frame),
    value: f64,
) -> JointInfo {
    JointInfo {
        id: JointId::from(Id(n)),
        name: name.into(),
        kind,
        parent: ends.0.iter().map(|n| comp(*n)).collect(),
        child: ends.1.iter().map(|n| comp(*n)).collect(),
        limits,
        state: JointState::Placed(JointPlacement {
            parent_anchor: anchors.0,
            child_anchor: anchors.1,
            value,
            diagnostics: vec![],
        }),
    }
}

fn run(world: &mut World, choices: &[ChoiceValue]) -> Vec<ExportFile> {
    let subject = Subject::Assembly(world.assemblies[0].id);
    Robotics
        .export(world, URDF, &subject, &[], choices)
        .unwrap()
}

fn main_text(files: &[ExportFile]) -> String {
    String::from_utf8(files[0].bytes.clone()).unwrap()
}

fn assembly(id: u64, components: Vec<ComponentInfo>, joints: Vec<JointInfo>) -> AssemblyInfo {
    AssemblyInfo {
        id: AssemblyId::from(Id(id)),
        name: format!("Arm {id}"),
        components,
        joints,
    }
}

/// A grounded base and an arm on a revolute joint at (0, 0, 0.1), turned by
/// `q`: the child's anchor at its own origin.
fn chain(q: f64) -> World {
    let up = frame([0.0, 0.0, 0.1], 0.0);
    let here = frame([0.0, 0.0, 0.0], 0.0);
    World {
        parts: vec![part_info(1, "Base"), part_info(2, "Link")],
        massless: vec![],
        assemblies: vec![assembly(
            10,
            vec![
                component(&[1], "base", 1, frame([0.0; 3], 0.0)),
                component(&[2], "arm", 2, frame([0.0, 0.0, 0.1], q)),
            ],
            vec![joint(
                1,
                "hinge",
                JointKind::Revolute,
                (&[1], &[2]),
                Some([-1.0, 1.0]),
                (up, here),
                q,
            )],
        )],
    }
}

#[test]
fn a_chain_has_a_base_joint_a_revolute_joint_and_one_stl_per_part() {
    let files = run(&mut chain(0.5), &[]);
    let names: Vec<_> = files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Arm_10.urdf", "meshes/Base.stl", "meshes/Link.stl"]);
    let x = main_text(&files);
    assert!(x.contains("<link name=\"base_link\"/>"), "{x}");
    assert!(
        x.contains("<joint name=\"base_to_base\" type=\"fixed\">"),
        "{x}"
    );
    assert!(
        x.contains("<joint name=\"hinge\" type=\"revolute\">"),
        "{x}"
    );
    assert!(x.contains("<parent link=\"base\"/>"), "{x}");
    assert!(x.contains("<child link=\"arm\"/>"), "{x}");
    assert!(x.contains("<origin xyz=\"0 0 0.1\" rpy=\"0 0 0\"/>"), "{x}");
    assert!(x.contains("<axis xyz=\"0 0 1\"/>"), "{x}");
    assert!(
        x.contains("<limit lower=\"-1\" upper=\"1\" effort=\"100\" velocity=\"1\"/>"),
        "{x}"
    );
    assert!(x.contains("<mass value=\"2\"/>"), "{x}");
    assert!(x.contains("<mesh filename=\"meshes/Link.stl\"/>"), "{x}");
    assert!(
        x.contains("<material name=\"Steel\"><color rgba=\"0.5 0.5 0.5 1\"/></material>"),
        "{x}"
    );
    // 80 bytes of header, a count, and one 50-byte triangle.
    assert_eq!(files[1].bytes.len(), 134);
}

#[test]
fn the_joint_origin_does_not_depend_on_the_joint_value() {
    let joint_origin = |q| {
        let x = main_text(&run(&mut chain(q), &[]));
        let at = x.find("name=\"hinge\"").unwrap();
        x[at..]
            .lines()
            .find(|l| l.contains("<origin"))
            .unwrap()
            .to_owned()
    };
    assert_eq!(joint_origin(0.0), joint_origin(0.9));
}

#[test]
fn an_anchor_off_the_link_origin_moves_the_link_frame() {
    // The child's anchor sits 0.2 m along its x: the link frame is there,
    // and its part is 0.2 m behind it.
    let up = frame([0.0, 0.0, 0.1], 0.0);
    let anchor = frame([0.2, 0.0, 0.0], 0.0);
    let mut w = chain(0.0);
    w.assemblies[0].components[1].pose = frame([-0.2, 0.0, 0.1], 0.0);
    w.assemblies[0].joints[0] = joint(
        1,
        "hinge",
        JointKind::Revolute,
        (&[1], &[2]),
        Some([-1.0, 1.0]),
        (up, anchor),
        0.0,
    );
    let x = main_text(&run(&mut w, &[]));
    assert!(
        x.contains("<origin xyz=\"-0.2 0 0\" rpy=\"0 0 0\"/>"),
        "the visual: {x}"
    );
}

#[test]
fn a_joint_that_does_not_reproduce_the_pose_is_left_out_with_a_warning() {
    let mut w = chain(0.5);
    w.assemblies[0].components[1].pose = frame([0.3, 0.0, 0.1], 0.5);
    let x = main_text(&run(&mut w, &[]));
    assert!(!x.contains("<joint name=\"hinge\""), "{x}");
    assert!(
        x.contains("warning: joint hinge does not reproduce the pose of arm"),
        "{x}"
    );
    assert!(
        x.contains("<joint name=\"base_to_arm\" type=\"fixed\">"),
        "{x}"
    );
}

#[test]
fn each_joint_kind_and_a_joint_without_limits() {
    let up = frame([0.0, 0.0, 0.1], 0.0);
    let here = frame([0.0; 3], 0.0);
    let mut w = chain(0.0);
    w.parts.push(part_info(3, "Slider"));
    w.parts.push(part_info(4, "Cap"));
    w.parts.push(part_info(5, "Wheel"));
    let a = &mut w.assemblies[0];
    a.components[1].pose = frame([0.0, 0.0, 0.1], 0.0);
    a.components
        .push(component(&[3], "slider", 3, frame([0.0, 0.0, 0.3], 0.0)));
    a.components
        .push(component(&[4], "cap", 4, frame([0.0, 0.0, 0.1], 0.0)));
    a.components
        .push(component(&[5], "wheel", 5, frame([0.0, 0.0, 0.1], 0.25)));
    a.joints = vec![
        joint(
            1,
            "free",
            JointKind::Revolute,
            (&[1], &[5]),
            None,
            (up, here),
            0.25,
        ),
        joint(
            2,
            "slide",
            JointKind::Prismatic,
            (&[1], &[3]),
            Some([0.0, 0.5]),
            (up, here),
            0.2,
        ),
        joint(
            3,
            "glue",
            JointKind::Fixed,
            (&[1], &[4]),
            None,
            (up, here),
            0.0,
        ),
        joint(
            4,
            "arm",
            JointKind::Revolute,
            (&[1], &[2]),
            Some([-1.0, 1.0]),
            (up, here),
            0.0,
        ),
    ];
    let x = main_text(&run(&mut w, &[]));
    assert!(
        x.contains("<joint name=\"free\" type=\"continuous\">"),
        "{x}"
    );
    assert!(
        x.contains("<joint name=\"slide\" type=\"prismatic\">"),
        "{x}"
    );
    assert!(x.contains("<joint name=\"glue\" type=\"fixed\">"), "{x}");
    assert!(x.contains("<limit lower=\"0\" upper=\"0.5\""), "{x}");
    // A branch: three joints hang from the one parent.
    assert_eq!(x.matches("<parent link=\"base\"/>").count(), 4, "{x}");
}

#[test]
fn a_nested_assembly_hangs_its_parts_by_the_outer_poses() {
    let up = frame([0.0, 0.0, 0.1], 0.0);
    let here = frame([0.0; 3], 0.0);
    let inner = AssemblyId::from(Id(20));
    let mut w = chain(0.0);
    w.assemblies[0].joints.clear();
    w.assemblies[0].components.truncate(1);
    w.assemblies[0].components.push(ComponentInfo {
        path: vec![comp(7)],
        name: "wrist".into(),
        of: ComponentOf::Assembly(inner),
        pose: frame([1.0, 0.0, 0.0], 0.0),
        diagnostics: vec![],
    });
    // Inside the wrist: a part at its origin and one turned on a joint.
    w.assemblies[0]
        .components
        .push(component(&[7, 1], "palm", 1, frame([1.0, 0.0, 0.0], 0.0)));
    w.assemblies[0]
        .components
        .push(component(&[7, 2], "finger", 2, frame([1.0, 0.0, 0.1], 0.0)));
    w.assemblies.push(assembly(
        20,
        vec![],
        vec![joint(
            5,
            "knuckle",
            JointKind::Revolute,
            (&[1], &[2]),
            Some([0.0, 1.0]),
            (up, here),
            0.0,
        )],
    ));
    let x = main_text(&run(&mut w, &[]));
    assert!(x.contains("<link name=\"wrist.palm\">"), "{x}");
    assert!(
        x.contains("<joint name=\"knuckle\" type=\"revolute\">"),
        "{x}"
    );
    assert!(x.contains("<parent link=\"wrist.palm\"/>"), "{x}");
    assert!(
        x.contains("<joint name=\"base_to_wrist.palm\" type=\"fixed\">"),
        "{x}"
    );
    assert!(x.contains("<origin xyz=\"1 0 0\" rpy=\"0 0 0\"/>"), "{x}");
}

#[test]
fn an_unplaced_component_hangs_from_the_base_with_a_warning() {
    let mut w = chain(0.0);
    w.assemblies[0].joints.clear();
    w.assemblies[0].components[1].diagnostics =
        vec![Diagnostic::new(Severity::Warning, "assembly.unplaced", "no pose").unwrap()];
    let x = main_text(&run(&mut w, &[]));
    assert!(
        x.contains("warning: arm is not placed (assembly.unplaced)"),
        "{x}"
    );
    assert!(
        x.contains("<joint name=\"base_to_arm\" type=\"fixed\">"),
        "{x}"
    );
}

#[test]
fn a_part_without_mass_has_no_inertial_and_a_part_used_twice_one_stl() {
    let mut w = chain(0.0);
    w.assemblies[0].joints.clear();
    w.massless = vec![part(2)];
    w.assemblies[0]
        .components
        .push(component(&[3], "arm 2", 2, frame([0.0, 1.0, 0.0], 0.0)));
    let files = run(&mut w, &[]);
    let x = main_text(&files);
    assert_eq!(files.len(), 3, "the base's STL and the arm's, once");
    assert_eq!(
        x.matches("<mesh filename=\"meshes/Link.stl\"/>").count(),
        4,
        "{x}"
    );
    assert_eq!(
        x.matches("<inertial>").count(),
        1,
        "only the base weighs: {x}"
    );
    assert!(
        x.contains("warning: Link has no mass (mass.no-density)"),
        "{x}"
    );
}

#[test]
fn the_package_choice_prefixes_mesh_paths_and_an_export_is_repeatable() {
    let package = [ChoiceValue {
        name: "mesh-prefix".into(),
        value: "package".into(),
    }];
    let files = run(&mut chain(0.5), &package);
    assert!(main_text(&files).contains("filename=\"package://Arm_10/meshes/Base.stl\""));
    assert_eq!(files, run(&mut chain(0.5), &package));
}

#[test]
fn a_part_is_refused() {
    let mut w = chain(0.0);
    let e = Robotics
        .export(&mut w, URDF, &Subject::Part(part(1)), &[], &[])
        .unwrap_err();
    assert_eq!(e.code.as_str(), "robotics.subject");
}
