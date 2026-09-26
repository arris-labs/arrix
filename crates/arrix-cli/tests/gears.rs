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
