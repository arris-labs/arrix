//! The compiled-in `gears` plugin against the real kernel: the checks the
//! plugin's own tests cannot make, since a plugin depends on the plugin API
//! alone (docs/PLUGINS.md §The test kit). They live beside the registration
//! list that names it until C3 packages the test kit.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_core::{
    DVec2, Diagnostic, FeatureId, Frame, Id, PartId, PersistentName, Profile, ProfileLoop,
    ProfileSegment, Quantity, QuantityKind, Ref, Severity, SlotName, TopoKind,
};
use arrix_doc::{
    Command, Document, Evaluator, FeatureOutcome, FeatureRecord, FeatureTypeId, Part, Registry,
    SlotView, apply, expr::Expr,
};
use arrix_gears::involute::{Spur, circle};
use arrix_kernel::{Kernel, KernelBody, KernelCall};
use arrix_plugin_api::{
    Body, Feature, InputValue, MassProperties, OutputValue, ParamValue, ResolvedInput,
};
use arrix_plugin_host::register_tier0;

const MM: f64 = 1e-3;

/// The plugin API's kernel over `arrix-kernel`, as a host gives it to a
/// plugin, for one feature.
struct Host {
    kernel: Kernel,
    bodies: Vec<KernelBody>,
}

fn diag(e: arrix_kernel::KernelError) -> Diagnostic {
    Diagnostic::new(Severity::Error, e.code().as_str(), e.to_string()).unwrap()
}

impl arrix_plugin_api::Kernel for Host {
    fn extrude(&mut self, profile: &Profile, distance: f64) -> Result<Body, Diagnostic> {
        let b = self
            .kernel
            .extrude(FeatureId(Id(7)), profile, distance)
            .map_err(diag)?;
        self.bodies.push(b);
        Ok(Body::from_handle(self.bodies.len() as u32 - 1))
    }

    fn names(&mut self, body: &Body) -> Result<Vec<PersistentName>, Diagnostic> {
        let b = self.bodies[body.handle() as usize];
        Ok(self
            .kernel
            .names(b)
            .map_err(diag)?
            .iter()
            .cloned()
            .collect())
    }

    fn face_frame(&mut self, body: &Body, face: &PersistentName) -> Result<Frame, Diagnostic> {
        let b = self.bodies[body.handle() as usize];
        self.kernel.face_frame(b, face).map_err(diag)
    }

    fn measure(&mut self, body: &Body) -> Result<MassProperties, Diagnostic> {
        let b = self.bodies[body.handle() as usize];
        let p = self.kernel.mass_properties(b).map_err(diag)?;
        Ok(MassProperties {
            volume: p.volume,
            area: p.area,
            centroid: p.centroid,
        })
    }
}

fn params(teeth: f64, module: f64, width: f64) -> Vec<ParamValue> {
    let q = |name: &str, kind, si| ParamValue {
        name: name.into(),
        value: Quantity::si(kind, si),
    };
    vec![
        q("teeth", QuantityKind::Count, teeth),
        q("module", QuantityKind::Length, module),
        q("width", QuantityKind::Length, width),
        q("pressure_angle", QuantityKind::Angle, 20.0_f64.to_radians()),
    ]
}

fn on_xy() -> Vec<ResolvedInput> {
    vec![ResolvedInput {
        name: "plane".into(),
        value: InputValue::Plane(Frame::WORLD_XY),
    }]
}

/// One evaluation from fresh state: names, volume bits and the records.
fn run(teeth: f64) -> (Vec<PersistentName>, u64, Vec<KernelCall>) {
    let mut host = Host {
        kernel: Kernel::new(),
        bodies: Vec::new(),
    };
    let out = arrix_gears::Gears
        .evaluate(
            &mut host,
            arrix_gears::SPUR,
            &params(teeth, MM, 8.0 * MM),
            &on_xy(),
        )
        .unwrap();
    let OutputValue::Body(body) = &out.slots[0].value else {
        panic!("a spur gear's slot is a body")
    };
    let names = arrix_plugin_api::Kernel::names(&mut host, body).unwrap();
    let volume = arrix_plugin_api::Kernel::measure(&mut host, body)
        .unwrap()
        .volume;
    (names, volume.to_bits(), host.kernel.records().to_vec())
}

/// The area a path loop of lines and arcs encloses: the polygon of its
/// ends, plus or minus each arc's circular segment.
fn area(l: &ProfileLoop) -> f64 {
    let ProfileLoop::Path { start, segments } = l else {
        panic!("a gear outline is a path")
    };
    let cross = |a: DVec2, b: DVec2| a.x * b.y - a.y * b.x;
    let mut at = *start;
    let mut sum = 0.0;
    for s in segments {
        let to = s.end();
        sum += cross(at, to) / 2.0;
        if let ProfileSegment::Arc { via, .. } = s {
            let (_, r) = circle(at, *via, to).unwrap();
            let chord = (to - at).length();
            let theta = 2.0 * (chord / (2.0 * r)).asin();
            let segment = r * r * (theta - theta.sin()) / 2.0;
            // An anticlockwise loop's inside is on its left: an arc bulging
            // left of its chord cuts into it, one bulging right adds to it.
            sum -= segment * cross(to - at, *via - at).signum();
        }
        at = to;
    }
    sum
}

#[test]
fn a_spur_gear_evaluated_twice_from_fresh_state_is_identical() {
    let (names, volume, records) = run(20.0);
    assert_eq!(run(20.0), (names.clone(), volume, records));
    let spur = Spur {
        teeth: 20,
        module: MM,
        pressure_angle: 20.0_f64.to_radians(),
    };
    let outline = spur.outline().unwrap();
    let ProfileLoop::Path { segments, .. } = &outline else {
        unreachable!()
    };
    let faces = names.iter().filter(|n| n.kind == TopoKind::Face).count();
    assert_eq!(faces, segments.len() + 2, "a side per curve and two caps");
    let expected = area(&outline) * 8.0 * MM;
    let got = f64::from_bits(volume);
    assert!(
        (got - expected).abs() < 1e-9 * expected,
        "{got} vs {expected}"
    );
    // Between the root and tip cylinders.
    let cylinder = |r: f64| std::f64::consts::PI * r * r * 8.0 * MM;
    assert!(got > cylinder(spur.root_radius()) && got < cylinder(spur.tip_radius()));
}

#[test]
fn a_teeth_edit_keeps_the_surviving_teeths_names() {
    let (twenty, ..) = run(20.0);
    let (eighteen, ..) = run(18.0);
    let key = |n: &PersistentName| n.to_string();
    let flank_20 = twenty
        .iter()
        .find(|n| key(n).contains(".side.") && key(n).ends_with(&Id(20_100).to_string()))
        .expect("tooth 20's first flank face");
    assert!(!eighteen.contains(flank_20), "tooth 20 is gone at 18 teeth");
    let caps: Vec<_> = twenty.iter().filter(|n| key(n).contains("cap")).collect();
    assert!(caps.iter().all(|c| eighteen.contains(c)));
    let tip_1 = twenty
        .iter()
        .find(|n| key(n).contains(".side.") && key(n).ends_with(&Id(1_003).to_string()))
        .unwrap();
    assert!(eighteen.contains(tip_1), "tooth 1's tip survives");
}

#[test]
fn the_registered_plugin_evaluates_on_a_datum_plane() {
    let mut registry = Registry::with_core_types();
    register_tier0(
        &mut registry,
        arrix_gears::MANIFEST,
        Arc::new(arrix_gears::Gears),
    )
    .unwrap();
    let record =
        |id: u64, ty: &str, params: &[(&str, &str)], inputs: Vec<(&str, Ref)>| FeatureRecord {
            id: FeatureId(Id(id)),
            type_id: FeatureTypeId::new(ty).unwrap(),
            type_version: 1,
            name: format!("f{id}"),
            params: params
                .iter()
                .map(|(k, v)| (k.to_string(), Expr::parse(v).unwrap()))
                .collect(),
            choices: BTreeMap::new(),
            inputs: inputs.into_iter().map(|(k, r)| (k.into(), r)).collect(),
            suppressed: false,
            sketch: None,
        };
    let plane = Ref::Slot {
        feature: FeatureId(Id(1)),
        slot: SlotName::new("plane").unwrap(),
    };
    let features = vec![
        record(1, "core.datum-plane", &[("offset", "10 mm")], vec![]),
        record(
            2,
            "gears.spur",
            &[("teeth", "0 + 18")],
            vec![("plane", plane)],
        ),
        record(3, "gears.spur", &[("teeth", "4")], vec![]),
    ];
    let part = Part {
        id: PartId(Id(100)),
        name: "Part".into(),
        history: features.iter().map(|f| f.id).collect(),
        features: features.into_iter().map(|f| (f.id, f)).collect(),
        rollback: None,
    };
    let doc: Document = apply(&Document::default(), &Command::AddPart { part })
        .unwrap()
        .document;
    let e = Evaluator::new(registry).evaluate(&doc);
    let Some(SlotView::Body(m)) = e.slot(FeatureId(Id(2)), "body") else {
        panic!("{:?}", e.events)
    };
    assert!(m.volume > 0.0 && m.faces > 18 * 6);
    let Some(FeatureOutcome::Failed { diagnostic, .. }) = e.outcome(FeatureId(Id(3))) else {
        panic!("{:?}", e.events)
    };
    assert_eq!(diagnostic.code.as_str(), "gears.teeth");
}

/// The kernel probe's gear case (plans/c1-m1-slice step 2): a 20-tooth,
/// 1 mm, 8 mm gear standing flush on a 40 × 30 × 5 mm plate's top, fused
/// (the touching case), then a 3 mm bore cut through both. Returns every
/// name of the joined and the cut body, their volumes' bits, and the
/// outline the gear was swept from.
fn gear_on_plate() -> (Vec<String>, Vec<String>, [u64; 2], ProfileLoop) {
    let [plate, gear, join, bore, cut] = [1, 2, 3, 4, 5].map(|n| FeatureId(Id(n)));
    let mut k = Kernel::new();
    let p = |x: f64, y: f64| DVec2::new(x * MM, y * MM);
    let line = |n, to| ProfileSegment::Line {
        key: arrix_core::CurveKey(Id(n)),
        to,
    };
    let rectangle = ProfileLoop::Path {
        start: p(0.0, 0.0),
        segments: vec![
            line(1, p(40.0, 0.0)),
            line(2, p(40.0, 30.0)),
            line(3, p(0.0, 30.0)),
            line(4, p(0.0, 0.0)),
        ],
    };
    let rectangle = Profile::new(Frame::WORLD_XY, rectangle, vec![]).unwrap();
    let plate = k.extrude(plate, &rectangle, 5.0 * MM).unwrap();
    let spur = Spur {
        teeth: 20,
        module: MM,
        pressure_angle: 20.0_f64.to_radians(),
    };
    let outline = spur.outline().unwrap();
    let at = |z: f64| {
        Frame::new(
            arrix_core::DVec3::new(20.0 * MM, 15.0 * MM, z * MM),
            arrix_core::DVec3::X,
            arrix_core::DVec3::Z,
        )
        .unwrap()
    };
    let profile = Profile::new(at(5.0), outline.clone(), vec![]).unwrap();
    let gear = k.extrude(gear, &profile, 8.0 * MM).unwrap();
    let joined = k.fuse(join, plate, gear).unwrap();
    let circle = ProfileLoop::Circle {
        key: arrix_core::CurveKey(Id(9)),
        center: DVec2::ZERO,
        radius: 3.0 * MM,
    };
    let tool = Profile::new(at(-1.0), circle, vec![]).unwrap();
    let tool = k.extrude(bore, &tool, 15.0 * MM).unwrap();
    let cut = k.cut(cut, joined, tool).unwrap();
    let names = |b| {
        k.names(b)
            .unwrap()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    let (joined_names, cut_names) = (names(joined), names(cut));
    let volumes = [joined, cut].map(|b| k.mass_properties(b).unwrap().volume.to_bits());
    (joined_names, cut_names, volumes, outline)
}

#[test]
fn a_gear_fused_flush_on_a_plate_and_bored_gives_its_hand_computed_body() {
    let (joined, cut, [v_joined, v_cut], outline) = gear_on_plate();
    let ProfileLoop::Path { segments, .. } = &outline else {
        unreachable!()
    };
    let plate = 0.04 * 0.03 * 0.005;
    let gear = area(&outline) * 8.0 * MM;
    let bore = std::f64::consts::PI * (3.0 * MM).powi(2) * 13.0 * MM;
    let close = |bits: u64, want: f64| (f64::from_bits(bits) - want).abs() < 1e-9 * want;
    assert!(
        close(v_joined, plate + gear),
        "{}",
        f64::from_bits(v_joined)
    );
    assert!(
        close(v_cut, plate + gear - bore),
        "{}",
        f64::from_bits(v_cut)
    );
    let faces = |names: &[String]| names.iter().filter(|n| n.starts_with("face:")).count();
    // The gear's bottom cap lay on the plate's top, opposed, and went; the
    // top face is one piece of itself; the bore adds its wall.
    assert_eq!(faces(&joined), 6 + segments.len() + 1);
    assert_eq!(faces(&cut), 6 + segments.len() + 1 + 1);
    let gear_bottom = format!("face:sweep.{}.start-cap", FeatureId(Id(2)));
    assert!(!joined.contains(&gear_bottom));
    let top = format!(
        "face:sweep.{}.end-cap/mod.{}.0",
        FeatureId(Id(1)),
        FeatureId(Id(3))
    );
    assert!(joined.contains(&top), "{joined:#?}");
}

#[test]
fn a_gear_fused_and_bored_twice_from_fresh_state_is_identical() {
    assert_eq!(gear_on_plate(), gear_on_plate());
}
