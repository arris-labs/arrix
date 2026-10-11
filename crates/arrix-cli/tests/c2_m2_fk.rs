//! c2-m2 step 3: `arrix_doc::place`, the forward kinematics the app's joint
//! drag shares with the evaluator (ADR-0030 §3), equals evaluating the
//! document with the value overridden. On `tests/docs/assembly` (the servo
//! on the bracket by a revolute joint) with the joint's kind, flip and value
//! edited by commands, and the assembly nested two deep.

mod common;

use std::collections::BTreeMap;

use arrix_core::{AssemblyId, ComponentId, Frame, IdMinter, JointId};
use arrix_doc::{
    Assembly, AssemblyOutcome, AuthorId, Command, Component, ComponentOf, JointKind, JointOutcome,
    MemorySource, PlacedComponent, Pose, expr::Expr, open, place,
};
use common::{Client, read_tree, registry, workspace};
use proptest::prelude::*;

fn e(text: &str) -> Expr {
    Expr::parse(text).unwrap()
}

fn scenario() -> Client {
    let mut files = BTreeMap::new();
    read_tree(&workspace().join("tests/docs/assembly"), "", &mut files);
    let doc = open(&MemorySource(files)).expect("the scenario opens");
    let author = AuthorId(IdMinter::new(7).next_id());
    Client::of_document(author, doc, registry())
}

fn outcomes(c: &Client) -> BTreeMap<AssemblyId, AssemblyOutcome> {
    let mut out = BTreeMap::new();
    for id in c.replica.document().assemblies().keys() {
        let (g, o) = c.replica.assembly(*id).expect("solved");
        assert_eq!(g, c.replica.generation());
        for joint in o.joints.values() {
            assert!(matches!(joint, JointOutcome::Ok { .. }), "{joint:?}");
        }
        out.insert(*id, o.clone());
    }
    out
}

fn near(a: Frame, b: Frame) -> bool {
    (a.origin() - b.origin()).length() < 1e-9
        && (a.x_axis() - b.x_axis()).length() < 1e-9
        && (a.z_axis() - b.z_axis()).length() < 1e-9
}

#[track_caller]
fn same(placed: &[PlacedComponent], evaluated: &AssemblyOutcome) {
    assert_eq!(placed.len(), evaluated.components.len());
    for (p, w) in placed.iter().zip(&evaluated.components) {
        assert_eq!(p.path, w.path);
        assert!(
            near(p.pose, w.pose),
            "{:?}: {:?} vs {:?}",
            p.path,
            p.pose,
            w.pose
        );
    }
}

fn coordinate(kind: JointKind, q: f64) -> String {
    match kind {
        JointKind::Prismatic => format!("{} mm", q * 1000.0),
        _ => format!("{q} rad"),
    }
}

/// Edits the scenario's joint to `kind`, `flip` and the coordinate `q`.
fn edit(c: &mut Client, kind: JointKind, flip: bool, q: f64) -> (AssemblyId, JointId) {
    let (assembly, a) = c.replica.document().assemblies().iter().next().unwrap();
    let mut joint = a.joints.values().next().unwrap().clone();
    let ids = (*assembly, joint.id);
    joint.kind = kind;
    joint.flip = flip;
    joint.value = e(&coordinate(kind, q));
    joint.offset = e(if kind == JointKind::Prismatic {
        "0 mm"
    } else {
        "0 deg"
    });
    c.submit(Command::EditJoint {
        assembly: ids.0,
        joint: Box::new(joint),
    });
    ids
}

#[test]
fn overriding_a_value_equals_evaluating_it_for_each_kind() {
    for kind in [JointKind::Revolute, JointKind::Prismatic, JointKind::Fixed] {
        for flip in [false, true] {
            let mut c = scenario();
            let (assembly, joint) = edit(&mut c, kind, flip, 0.0);
            let base = outcomes(&c);
            let doc = c.replica.document().clone();
            let unchanged = place(&doc, assembly, &base[&assembly], &base, &[]).unwrap();
            same(&unchanged, &base[&assembly]);
            for q in [0.3, -1.2, 2.5] {
                let placed = place(&doc, assembly, &base[&assembly], &base, &[(joint, q)]).unwrap();
                edit(&mut c, kind, flip, q);
                same(&placed, &outcomes(&c)[&assembly]);
            }
        }
    }
}

/// `inner` as the only component of a grounded wrapper, and that of another.
fn nest(c: &mut Client, inner: AssemblyId) -> [AssemblyId; 2] {
    let mut minter = IdMinter::new(4711);
    let mut of = inner;
    [10.0, -7.0].map(|x| {
        let (id, component): (AssemblyId, ComponentId) = (minter.mint(), minter.mint());
        let pose = Pose {
            position: [e(&format!("{x} mm")), e("5 mm"), e("0 mm")],
            rpy: [e("0 deg"), e("0 deg"), e("30 deg")],
        };
        c.submit(Command::AddAssembly {
            assembly: Assembly {
                id,
                name: format!("wrapper {x}"),
                components: [(
                    component,
                    Component {
                        id: component,
                        name: "inner".into(),
                        of: ComponentOf::Assembly(of),
                        pose: Some(pose),
                    },
                )]
                .into(),
                joints: BTreeMap::new(),
            },
        });
        of = id;
        id
    })
}

#[test]
fn a_nested_assembly_follows_the_value_two_deep() {
    let mut c = scenario();
    let (inner, joint) = edit(&mut c, JointKind::Revolute, true, 0.0);
    let wrappers = nest(&mut c, inner);
    let base = outcomes(&c);
    let doc = c.replica.document().clone();
    let q = 0.9;

    let mut now = base.clone();
    let again = place(&doc, inner, &base[&inner], &base, &[(joint, q)]).unwrap();
    now.get_mut(&inner).unwrap().components = again.clone();
    for w in wrappers {
        let placed = place(&doc, w, &base[&w], &now, &[]).unwrap();
        now.get_mut(&w).unwrap().components = placed;
    }

    edit(&mut c, JointKind::Revolute, true, q);
    let truth = outcomes(&c);
    for id in [inner, wrappers[0], wrappers[1]] {
        same(&now[&id].components, &truth[&id]);
    }
    assert_eq!(truth[&wrappers[1]].components.len(), 4);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(10))]

    #[test]
    fn any_value_placed_equals_any_value_evaluated(q in -6.0f64..6.0, flip in any::<bool>()) {
        let mut c = scenario();
        let (assembly, joint) = edit(&mut c, JointKind::Revolute, flip, 0.0);
        let base = outcomes(&c);
        let doc = c.replica.document().clone();
        let placed = place(&doc, assembly, &base[&assembly], &base, &[(joint, q)]).unwrap();
        edit(&mut c, JointKind::Revolute, flip, q);
        same(&placed, &outcomes(&c)[&assembly]);
    }
}
