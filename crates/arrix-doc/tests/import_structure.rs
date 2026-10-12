//! Import with structure (plans/c2-m3 step 7, ADR-0032): the probe's
//! `arm.step` as one `Group`: a part per product, a component per
//! occurrence at the file's placement, the screw placed twice one part
//! used twice, colours as materials, names the same on both instances so
//! a joint anchors on the second one's face; an undo removes it all.

use std::path::Path;

use arrix_core::{ComponentId, DVec3, FeatureId, Frame, IdMinter, PersistentName, TopoKind};
use arrix_doc::{
    Anchor, Assembly, AssemblyOutcome, Command, ComponentOf, Document, EvalStatus, Evaluation,
    Evaluator, Imported, Joint, JointKind, JointOutcome, Registry, Rgb, apply, expr::Expr,
    import_products,
};
use arrix_kernel::{ImportFrame, Kernel};

const MM: f64 = 1e-3;

fn bytes() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/step/arm.step");
    std::fs::read(path).expect("the arm fixture")
}

fn e(text: &str) -> Expr {
    Expr::parse(text).unwrap()
}

fn import_into(doc: &Document, seed: u64) -> (Imported, Document) {
    let bytes = bytes();
    let structure = Kernel::new().read_step_structure(&bytes).unwrap();
    let imported =
        import_products(doc, "arm.step", bytes, &structure, &mut IdMinter::new(seed)).unwrap();
    let applied = apply(doc, &imported.command).unwrap();
    (imported, applied.document)
}

fn eval(doc: &Document) -> Evaluation {
    Evaluator::new(Registry::with_core_types()).evaluate(doc)
}

fn near(a: DVec3, b: DVec3) -> bool {
    (a - b).length() < 1e-12
}

fn mm(x: f64, y: f64, z: f64) -> DVec3 {
    DVec3::new(x, y, z) * MM
}

fn assembly<'d>(doc: &'d Document, name: &str) -> &'d Assembly {
    doc.assemblies().values().find(|a| a.name == name).unwrap()
}

fn component(assembly: &Assembly, name: &str) -> ComponentId {
    assembly
        .components
        .values()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no component {name}: {assembly:?}"))
        .id
}

#[test]
fn the_tree_becomes_parts_assemblies_and_components_at_the_file_s_placements() {
    let (imported, doc) = import_into(&Document::default(), 1);
    assert!(imported.notes.is_empty(), "{:?}", imported.notes);
    let [ComponentOf::Assembly(root)] = imported.roots[..] else {
        panic!("one root assembly: {:?}", imported.roots)
    };
    assert_eq!(doc.assemblies()[&root].name, "arm");

    // bracket, servo and screw: a part per product, the screw once.
    let mut parts: Vec<&str> = doc.parts().values().map(|p| p.name.as_str()).collect();
    parts.sort();
    assert_eq!(parts, ["bracket", "screw", "servo"]);
    assert_eq!(doc.assemblies().len(), 2, "arm and mount");

    let mount = assembly(&doc, "mount");
    assert_eq!(mount.components.len(), 3);
    let screws: Vec<_> = mount
        .components
        .values()
        .filter(|c| c.name.starts_with("screw"))
        .map(|c| c.of)
        .collect();
    assert_eq!(screws.len(), 2);
    assert_eq!(screws[0], screws[1], "one part, used twice");

    // Colours became three materials with no density, one per part.
    let materials: Vec<_> = doc.materials().values().collect();
    assert_eq!(materials.len(), 3);
    assert!(materials.iter().all(|m| m.density.is_none()));
    let steel = materials
        .iter()
        .find(|m| m.appearance == Rgb([0.8, 0.8, 0.85]))
        .unwrap();
    assert_eq!(steel.name, "Colour #ccccd9");
    let screw = doc.parts().values().find(|p| p.name == "screw").unwrap();
    assert_eq!(screw.material, Some(steel.id));
    assert_eq!(screw.features.len(), 1);

    // The whole file evaluates, and the poses are the composed placements.
    let arm = assembly(&doc, "arm");
    let ev = eval(&doc);
    let line = ev.line("arm", &doc);
    assert_eq!(line.status, EvalStatus::Ok, "{line:?}");
    assert_eq!(line.bodies.len(), 3);
    let (bracket, mount_c) = (component(arm, "bracket"), component(arm, "mount"));
    let (servo, screw_b) = (component(mount, "servo"), component(mount, "screw 2"));
    let pose = |path: &[ComponentId]| ev.assemblies[&arm.id].pose(path).unwrap();
    let at = pose(&[bracket]);
    assert!(near(at.origin(), mm(5.0, -3.0, 0.0)), "{at:?}");
    // The servo: Rz(90) Rx(30) from the mount, which sits at (50, 10, 5).
    let at = pose(&[mount_c, servo]);
    assert!(near(at.origin(), mm(48.0, 10.0, 5.0)), "{at:?}");
    assert!(near(at.x_axis(), DVec3::Y), "{at:?}");
    assert!(near(at.z_axis(), DVec3::new(0.5, 0.0, 3f64.sqrt() / 2.0)));
    let at = pose(&[mount_c, screw_b]);
    assert!(near(at.origin(), mm(34.0, 46.0, 41.0)), "{at:?}");
    assert!(near(at.z_axis(), DVec3::Z));
}

#[test]
fn one_undo_removes_the_whole_import() {
    let (imported, doc) = import_into(&Document::default(), 1);
    let applied = apply(&Document::default(), &imported.command).unwrap();
    assert_eq!(applied.document, doc);
    let back = apply(&doc, &applied.inverse).unwrap().document;
    assert_eq!(back, Document::default());
}

#[test]
fn a_second_import_keeps_its_names_unique() {
    let (_, once) = import_into(&Document::default(), 1);
    let (_, twice) = import_into(&once, 2);
    assert_eq!(twice.parts().len(), 6);
    assert_eq!(twice.assemblies().len(), 4);
    // The same colours are the same three materials.
    assert_eq!(twice.materials().len(), 3);
    let mut names: Vec<_> = twice.assemblies().values().map(|a| &a.name).collect();
    names.sort();
    assert_eq!(names, ["arm", "arm 2", "mount", "mount 2"]);
}

/// A face of the part's body, by name, whose frame `pick` accepts.
fn face_where(
    k: &mut Kernel,
    feature: FeatureId,
    solid: u32,
    bytes: &[u8],
    pick: impl Fn(&Frame) -> bool,
) -> PersistentName {
    let body = k
        .import_step_in(feature, bytes, solid, ImportFrame::Product)
        .unwrap();
    let faces: Vec<PersistentName> = k
        .names(body)
        .unwrap()
        .of_kind(TopoKind::Face)
        .cloned()
        .collect();
    faces
        .into_iter()
        .find(|n| k.face_frame(body, n).is_ok_and(|f| pick(&f)))
        .expect("a face of the asked geometry")
}

#[test]
fn a_joint_anchors_on_a_face_of_the_second_instance() {
    let (_, doc) = import_into(&Document::default(), 1);
    let mount = assembly(&doc, "mount").clone();
    let servo_part = doc.parts().values().find(|p| p.name == "servo").unwrap();
    let screw_part = doc.parts().values().find(|p| p.name == "screw").unwrap();
    let feature = |p: &arrix_doc::Part| p.history[0];
    let bytes = bytes();
    let structure = Kernel::new().read_step_structure(&bytes).unwrap();
    // The servo's top, z = 36 mm facing up; the screw's top, z = 10 mm.
    let solid_of = |name: &str| {
        fn find(o: &arrix_kernel::StepOccurrence, name: &str) -> Option<u32> {
            if o.name == name && !o.solids.is_empty() {
                return Some(o.solids[0]);
            }
            o.children.iter().find_map(|c| find(c, name))
        }
        structure.roots.iter().find_map(|r| find(r, name)).unwrap()
    };
    let mut k = Kernel::new();
    let top = face_where(
        &mut k,
        feature(servo_part),
        solid_of("servo"),
        &bytes,
        |f| near(f.z_axis(), DVec3::Z) && (f.origin().z - 36.0 * MM).abs() < 1e-12,
    );
    let screw_top = face_where(
        &mut k,
        feature(screw_part),
        solid_of("screw"),
        &bytes,
        |f| near(f.z_axis(), DVec3::Z) && (f.origin().z - 10.0 * MM).abs() < 1e-12,
    );
    let body = k
        .import_step_in(
            feature(screw_part),
            &bytes,
            solid_of("screw"),
            ImportFrame::Product,
        )
        .unwrap();
    let faces: Vec<PersistentName> = k
        .names(body)
        .unwrap()
        .of_kind(TopoKind::Face)
        .cloned()
        .collect();
    let side = faces
        .into_iter()
        .find(|n| k.anchor_frame(body, n, None).is_ok() && k.face_frame(body, n).is_err())
        .unwrap();

    let (servo, screw_b) = (component(&mount, "servo"), component(&mount, "screw 2"));
    let mut second = mount.components[&screw_b].clone();
    second.pose = None;
    let joint = Joint {
        id: arrix_core::JointId(arrix_core::Id(900)),
        name: "seat".into(),
        kind: JointKind::Fixed,
        parent: Anchor {
            path: vec![servo],
            axis: top,
            origin: None,
        },
        child: Anchor {
            path: vec![screw_b],
            axis: side,
            origin: Some(screw_top),
        },
        value: e("0 deg"),
        offset: e("0 deg"),
        limits: None,
        flip: false,
    };
    let edit = Command::EditComponent {
        assembly: mount.id,
        component: Box::new(second),
    };
    let add = Command::AddJoint {
        assembly: mount.id,
        joint: Box::new(joint.clone()),
    };
    let doc = apply(&doc, &edit).unwrap().document;
    let doc = apply(&doc, &add).unwrap().document;
    let ev = eval(&doc);
    let line = ev.line("arm", &doc);
    assert_eq!(line.status, EvalStatus::Ok, "{:?}", line.assemblies);
    let outcome: &AssemblyOutcome = &ev.assemblies[&mount.id];
    let JointOutcome::Ok {
        parent,
        child,
        diagnostics,
        ..
    } = &outcome.joints[&joint.id]
    else {
        panic!("{:?}", outcome.joints)
    };
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(
        near(parent.origin(), child.origin()),
        "{parent:?} {child:?}"
    );
    // The screw is on the servo's top: its own z is the servo's.
    let servo_z = outcome.pose(&[servo]).unwrap().z_axis();
    assert!(near(outcome.pose(&[screw_b]).unwrap().z_axis(), servo_z));
}

#[test]
fn an_unreadable_file_has_no_structure_to_import() {
    let err = Kernel::new().read_step_structure(b"nothing").unwrap_err();
    assert_eq!(err.code().as_str(), "kernel.import.parse");
}
