//! Mass, centroid and inertia (plans/c2-m3 step 5, ADR-0031 §4) against
//! closed forms: a steel box, a cylinder about its axis, two boxes in an
//! assembly (one turned a quarter, so the parallel-axis theorem and the
//! pose both count), a density read from a parameter, and a body with no
//! material left out of the sums, flagged.

use std::collections::BTreeMap;

use arrix_core::{
    AssemblyId, ComponentId, FeatureId, Id, MaterialId, ParamId, PartId, QuantityKind, Ref,
    RegionKey, SlotName,
};
use arrix_doc::{
    Assembly, Command, Component, ComponentOf, Document, Evaluation, Evaluator, FeatureRecord,
    FeatureTypeId, Inertial, Material, Param, Part, Pose, Registry, Rgb, SlotRef, SlotView, apply,
    expr::Expr,
};
use arrix_sketch::{Constraint, Draft, Entity, Point, Sketch};

const STEEL: MaterialId = MaterialId(Id(1));
const ALU: MaterialId = MaterialId(Id(2));
const RHO: ParamId = ParamId(Id(3));
const BOX: PartId = PartId(Id(10));
const CYL: PartId = PartId(Id(11));
const SKETCH: u64 = 1;
const EXTRUDE: u64 = 2;
const ASM: AssemblyId = AssemblyId(Id(20));
const A: ComponentId = ComponentId(Id(30));
const B: ComponentId = ComponentId(Id(31));

fn fid(n: u64) -> FeatureId {
    FeatureId(Id(n))
}

fn e(text: &str) -> Expr {
    Expr::parse(text).unwrap()
}

fn with(doc: &Document, command: Command) -> Document {
    apply(doc, &command)
        .unwrap_or_else(|err| panic!("{command:?}: {err}"))
        .document
}

fn rect(w: f64, h: f64) -> Sketch {
    let mut d = Draft::seeded(7);
    let p = [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].map(|(x, y)| d.add_point(Point::new(x, y)));
    for i in 0..4 {
        let line = d.add_entity(Entity::Line {
            start: p[i],
            end: p[(i + 1) % 4],
        });
        d.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line }
        } else {
            Constraint::Vertical { line }
        });
    }
    d.sketch
}

fn circle(r: f64) -> Sketch {
    let mut d = Draft::seeded(7);
    let c = d.add_point(Point::new(0.0, 0.0));
    let circle = d.add_entity(Entity::Circle {
        center: c,
        radius: r,
    });
    d.add_constraint(Constraint::Radius {
        target: circle,
        value: r,
    });
    d.sketch
}

fn record(id: u64, type_id: &str) -> FeatureRecord {
    FeatureRecord {
        id: fid(id),
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: format!("F{id}"),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

fn part_of(id: PartId, record: FeatureRecord) -> Part {
    Part {
        id,
        name: format!("P{}", id.0.0),
        history: vec![record.id],
        features: [(record.id, record)].into(),
        rollback: None,
        material: None,
        body_materials: Default::default(),
    }
}

fn region(sketch: &Sketch) -> RegionKey {
    let mut r = record(SKETCH, "core.sketch");
    r.sketch = Some(Box::new(sketch.clone()));
    let doc = with(
        &Document::default(),
        Command::AddPart {
            part: part_of(PartId(Id(99)), r),
        },
    );
    let ev = Evaluator::new(Registry::with_core_types()).evaluate(&doc);
    match ev.slot(fid(SKETCH), "sketch") {
        Some(SlotView::Sketch(v)) => v.regions[0].key.clone(),
        other => panic!("{other:?}"),
    }
}

/// A part of one sketch and one new-body extrude of `height` metres.
fn prism(id: PartId, sketch: Sketch, height: f64) -> Part {
    let key = region(&sketch);
    let mut s = record(SKETCH, "core.sketch");
    s.sketch = Some(Box::new(sketch));
    let mut x = record(EXTRUDE, "core.extrude");
    x.params
        .insert("distance".into(), e(&format!("{height} m")));
    x.choices.insert("mode".into(), "new".into());
    x.inputs.insert(
        "region".into(),
        Ref::Region {
            feature: fid(SKETCH),
            key,
        }
        .into(),
    );
    Part {
        id,
        name: format!("P{}", id.0.0),
        history: vec![fid(SKETCH), fid(EXTRUDE)],
        features: [(fid(SKETCH), s), (fid(EXTRUDE), x)].into(),
        rollback: None,
        material: None,
        body_materials: Default::default(),
    }
}

fn material(id: MaterialId, name: &str, density: Option<&str>) -> Material {
    Material {
        id,
        name: name.into(),
        density: density.map(e),
        appearance: Rgb([0.5, 0.5, 0.5]),
    }
}

fn add_material(doc: &Document, m: Material) -> Document {
    with(doc, Command::AddMaterial { material: m })
}

fn made_of(doc: &Document, part: PartId, m: MaterialId) -> Document {
    with(
        doc,
        Command::AssignMaterial {
            part,
            body: None,
            material: Some(m),
        },
    )
}

fn eval(doc: &Document) -> Evaluation {
    Evaluator::new(Registry::with_core_types()).evaluate(doc)
}

fn close(got: f64, want: f64, what: &str) {
    assert!(
        (got - want).abs() <= 1e-9 * want.abs().max(1e-3),
        "{what}: {got} != {want}"
    );
}

#[allow(clippy::needless_range_loop)]
fn inertia_is(i: &Inertial, want: [[f64; 3]; 3], what: &str) {
    for r in 0..3 {
        for c in 0..3 {
            close(
                i.inertia[r][c] + 1e-30,
                want[r][c] + 1e-30,
                &format!("{what}[{r}][{c}]"),
            );
        }
    }
}

const A_: f64 = 0.04;
const B_: f64 = 0.02;
const C_: f64 = 0.01;
const STEEL_RHO: f64 = 7850.0;

fn steel_box() -> Document {
    let doc = with(
        &Document::default(),
        Command::AddPart {
            part: prism(BOX, rect(A_, B_), C_),
        },
    );
    made_of(
        &add_material(&doc, material(STEEL, "Steel", Some("7.85 g/cm3"))),
        BOX,
        STEEL,
    )
}

#[test]
fn a_steel_box_has_its_closed_form_mass_centroid_and_inertia() {
    let ev = eval(&steel_box());
    let m = STEEL_RHO * A_ * B_ * C_;
    let body = &ev.masses.bodies[0];
    assert_eq!(body.material, Some(STEEL));
    close(body.volume, A_ * B_ * C_, "volume");
    let mass = body.mass.expect("a steel body has a mass");
    close(mass.mass, m, "mass");
    for (got, want) in mass.centroid.iter().zip([A_ / 2.0, B_ / 2.0, C_ / 2.0]) {
        close(*got, want, "centroid");
    }
    let diag = |x: f64, y: f64, z: f64| [[x, 0.0, 0.0], [0.0, y, 0.0], [0.0, 0.0, z]];
    inertia_is(
        &mass,
        diag(
            m * (B_ * B_ + C_ * C_) / 12.0,
            m * (A_ * A_ + C_ * C_) / 12.0,
            m * (A_ * A_ + B_ * B_) / 12.0,
        ),
        "inertia",
    );
    // The part's total is the body's, and nothing is left out.
    let part = &ev.masses.parts[&BOX];
    assert!(!part.partial && part.diagnostics.is_empty());
    assert_eq!(part.mass, Some(mass));
}

#[test]
fn a_cylinder_turns_about_its_axis_as_half_m_r_squared() {
    let (r, h) = (0.01, 0.03);
    let doc = with(
        &Document::default(),
        Command::AddPart {
            part: prism(CYL, circle(r), h),
        },
    );
    let doc = made_of(
        &add_material(&doc, material(ALU, "Alu", Some("2700 kg/m3"))),
        CYL,
        ALU,
    );
    let mass = eval(&doc).masses.parts[&CYL].mass.unwrap();
    // The region is a polygon-free circle, so the volume is exact to the
    // kernel's integration, not to a tessellation.
    let m = 2700.0 * std::f64::consts::PI * r * r * h;
    close(mass.mass, m, "mass");
    close(mass.inertia[2][2], 0.5 * m * r * r, "Izz = m r² / 2");
    close(mass.inertia[0][0], m * (3.0 * r * r + h * h) / 12.0, "Ixx");
    close(mass.centroid[2], h / 2.0, "centroid z");
}

fn box_assembly() -> Document {
    let pose = |x: &str, yaw: &str| Pose {
        position: [e(x), e("0 m"), e("0 m")],
        rpy: [e("0 deg"), e("0 deg"), e(yaw)],
    };
    let component = |id, p| Component {
        id,
        name: format!("c{}", id.0.0),
        of: ComponentOf::Part(BOX),
        pose: Some(p),
    };
    with(
        &steel_box(),
        Command::AddAssembly {
            assembly: Assembly {
                id: ASM,
                name: "Pair".into(),
                components: [
                    (A, component(A, pose("0 m", "0 deg"))),
                    (B, component(B, pose("0.1 m", "90 deg"))),
                ]
                .into(),
                joints: BTreeMap::new(),
            },
        },
    )
}

#[test]
#[allow(clippy::needless_range_loop)]
fn two_boxes_at_their_poses_sum_by_the_parallel_axis_theorem() {
    let l = 0.1;
    let ev = eval(&box_assembly());
    let total = ev.masses.assemblies[&ASM].mass.expect("two steel boxes");
    let m = STEEL_RHO * A_ * B_ * C_;
    close(total.mass, 2.0 * m, "mass");
    // B turned a quarter about z at x = L spans x in [L - b, L], y in [0, a].
    let ca = [A_ / 2.0, B_ / 2.0, C_ / 2.0];
    let cb = [l - B_ / 2.0, A_ / 2.0, C_ / 2.0];
    let c: Vec<f64> = (0..3).map(|i| (ca[i] + cb[i]) / 2.0).collect();
    for i in 0..3 {
        close(total.centroid[i], c[i], "centroid");
    }
    let own_a = [
        m * (B_ * B_ + C_ * C_) / 12.0,
        m * (A_ * A_ + C_ * C_) / 12.0,
        m * (A_ * A_ + B_ * B_) / 12.0,
    ];
    // The turned box swaps its x and y extents.
    let own_b = [
        m * (A_ * A_ + C_ * C_) / 12.0,
        m * (B_ * B_ + C_ * C_) / 12.0,
        m * (A_ * A_ + B_ * B_) / 12.0,
    ];
    let mut want = [[0.0; 3]; 3];
    for (own, centre) in [(own_a, ca), (own_b, cb)] {
        let r: Vec<f64> = (0..3).map(|i| centre[i] - c[i]).collect();
        let r2: f64 = r.iter().map(|x| x * x).sum();
        for i in 0..3 {
            for j in 0..3 {
                let delta = if i == j { r2 } else { 0.0 };
                want[i][j] += m * (delta - r[i] * r[j]) + if i == j { own[i] } else { 0.0 };
            }
        }
    }
    inertia_is(&total, want, "inertia");
    assert!(!ev.masses.assemblies[&ASM].partial);
}

#[test]
fn a_density_may_be_a_parameter() {
    let doc = with(
        &steel_box(),
        Command::AddParam {
            param: RHO,
            record: Param {
                name: "rho".into(),
                kind: QuantityKind::Density,
                expr: e("1000 kg/m3"),
            },
        },
    );
    let doc = with(
        &doc,
        Command::EditMaterial {
            material: material(STEEL, "Steel", Some("rho * 2")),
        },
    );
    let mass = eval(&doc).masses.parts[&BOX].mass.unwrap().mass;
    close(mass, 2000.0 * A_ * B_ * C_, "mass");
}

#[test]
fn a_body_without_a_material_keeps_its_volume_and_flags_the_totals() {
    let doc = with(
        &box_assembly(),
        Command::AssignMaterial {
            part: BOX,
            body: None,
            material: None,
        },
    );
    let ev = eval(&doc);
    let body = &ev.masses.bodies[0];
    close(body.volume, A_ * B_ * C_, "volume");
    close(body.centroid[0], A_ / 2.0, "centroid");
    assert!(body.mass.is_none());
    assert_eq!(
        body.diagnostic.as_ref().unwrap().code.as_str(),
        "mass.no-material"
    );
    let part = &ev.masses.parts[&BOX];
    assert!(part.partial && part.mass.is_none());
    close(part.volume, A_ * B_ * C_, "part volume");
    let asm = &ev.masses.assemblies[&ASM];
    assert!(asm.partial && asm.mass.is_none());
    close(asm.volume, 2.0 * A_ * B_ * C_, "assembly volume");
    assert_eq!(asm.diagnostics[0].code.as_str(), "mass.no-material");
    // A material with no density is the same, and a density that is not
    // one is a different code.
    let doc = made_of(&add_material(&doc, material(ALU, "Alu", None)), BOX, ALU);
    let ev = eval(&doc);
    assert_eq!(
        ev.masses.bodies[0]
            .diagnostic
            .as_ref()
            .unwrap()
            .code
            .as_str(),
        "mass.no-material"
    );
    let doc = with(
        &doc,
        Command::EditMaterial {
            material: material(ALU, "Alu", Some("5 mm")),
        },
    );
    let ev = eval(&doc);
    assert_eq!(
        ev.masses.bodies[0]
            .diagnostic
            .as_ref()
            .unwrap()
            .code
            .as_str(),
        "mass.density"
    );
}

#[test]
fn a_per_body_material_overrides_the_part_s() {
    let slot = SlotRef {
        feature: fid(EXTRUDE),
        slot: SlotName::new("body").unwrap(),
    };
    let doc = add_material(&steel_box(), material(ALU, "Alu", Some("2700 kg/m3")));
    let doc = with(
        &doc,
        Command::AssignMaterial {
            part: BOX,
            body: Some(slot),
            material: Some(ALU),
        },
    );
    let ev = eval(&doc);
    assert_eq!(ev.masses.bodies[0].material, Some(ALU));
    close(
        ev.masses.parts[&BOX].mass.unwrap().mass,
        2700.0 * A_ * B_ * C_,
        "mass",
    );
}

#[test]
fn an_edit_of_the_density_changes_the_mass_and_nothing_else() {
    let doc = steel_box();
    let mut ev = Evaluator::new(Registry::with_core_types());
    let before = ev.evaluate(&doc);
    let doc2 = with(
        &doc,
        Command::EditMaterial {
            material: material(STEEL, "Steel", Some("1000 kg/m3")),
        },
    );
    let calls = ev.kernel_calls().len();
    let after = ev.evaluate(&doc2);
    assert_eq!(
        ev.kernel_calls().len(),
        calls,
        "no kernel call for a density"
    );
    close(
        after.masses.parts[&BOX].mass.unwrap().mass / before.masses.parts[&BOX].mass.unwrap().mass,
        1000.0 / 7850.0,
        "mass ratio",
    );
}
