//! Tier 0 hosting (plans/c1-m1-document step 10): a plugin written against
//! `arrix-plugin-api` alone evaluates on the built-ins' path.

use std::collections::BTreeMap;
use std::sync::Arc;

use arrix_core::PartId;
use arrix_doc::{
    Command, Document, Evaluator, FeatureOutcome, FeatureRecord, FeatureTypeId, InputHash, Part,
    Registry, SlotView, apply, expr::Expr,
};
use arrix_plugin_api::{
    CurveKey, DVec2, Diagnostic, Feature, FeatureId, FeatureOutput, FeatureTypeSpec, Id, InputKind,
    InputSpec, InputValue, Kernel, OutputValue, ParamSpec, ParamValue, Profile, ProfileLoop,
    QuantityKind, Ref, ResolvedInput, SlotKind, SlotName, SlotOutput, SlotSpec,
};
use arrix_plugin_host::{HostError, register_tier0};

fn manifest(id: &str, version: &str, api: &str) -> String {
    format!(
        "[plugin]\nid = \"{id}\"\nversion = \"{version}\"\napi = \"{api}\"\ntier = 0\n\
         title = \"Demo\"\nlicence = \"MIT OR Apache-2.0\"\n"
    )
}

/// `demo.disc`: a disc of `radius` on its plane, extruded 1 mm.
/// `demo.stack`: that disc with a disc of half the radius standing on it,
/// fused. `demo.washer`: that disc less a bore of half the radius. (One
/// boolean each: a feature's two sweeps name their caps alike, so a second
/// boolean over both can give two entities one name, `kernel.naming`.)
/// `demo.boom`: panics.
struct Demo {
    prefix: &'static str,
}

impl Feature for Demo {
    fn describe(&self) -> Vec<FeatureTypeSpec> {
        let body = SlotSpec {
            name: SlotName::new("body").unwrap(),
            kind: SlotKind::Body,
            modifies: None,
        };
        let spec = |name: &str| FeatureTypeSpec {
            id: format!("{}.{name}", self.prefix),
            version: 1,
            title: name.into(),
            params: vec![ParamSpec {
                name: "radius".into(),
                title: "Radius".into(),
                kind: QuantityKind::Length,
                default: "2 mm".into(),
            }],
            inputs: vec![InputSpec {
                name: "plane".into(),
                title: "Plane".into(),
                kind: InputKind::Plane,
            }],
            outputs: vec![body.clone()],
        };
        vec![spec("disc"), spec("stack"), spec("washer"), spec("boom")]
    }

    fn evaluate(
        &self,
        kernel: &mut dyn Kernel,
        type_id: &str,
        params: &[ParamValue],
        inputs: &[ResolvedInput],
    ) -> Result<FeatureOutput, Diagnostic> {
        if type_id.ends_with(".boom") {
            panic!("the demo plugin's boom");
        }
        let InputValue::Plane(plane) = inputs[0].value else {
            unreachable!("the demo's one input is a plane");
        };
        let r = params[0].value.si;
        let mut disc = |key: u64, radius: f64, below: f64, height: f64| {
            let circle = ProfileLoop::Circle {
                key: CurveKey(Id(key)),
                center: DVec2::ZERO,
                radius,
            };
            let on = Profile::new(plane.offset(-below), circle, vec![]).unwrap();
            kernel.extrude(&on, height)
        };
        let mut body = disc(1, r, 0.0, 1e-3)?;
        if type_id.ends_with(".stack") {
            let top = disc(2, r / 2.0, -1e-3, 1e-3)?;
            body = kernel.fuse(&body, &top)?;
        } else if type_id.ends_with(".washer") {
            let bore = disc(3, r / 2.0, 0.5e-3, 2e-3)?;
            body = kernel.cut(&body, &bore)?;
        }
        Ok(FeatureOutput {
            slots: vec![SlotOutput {
                name: SlotName::new("body").unwrap(),
                value: OutputValue::Body(body),
            }],
        })
    }
}

fn demo() -> Arc<dyn Feature> {
    Arc::new(Demo { prefix: "demo" })
}

fn registry(version: &str) -> Registry {
    let mut r = Registry::with_core_types();
    register_tier0(&mut r, &manifest("demo", version, "^0.3"), demo()).unwrap();
    r
}

fn record(id: u64, type_id: &str, inputs: &[(&str, Ref)]) -> FeatureRecord {
    FeatureRecord {
        id: FeatureId(Id(id)),
        type_id: FeatureTypeId::new(type_id).unwrap(),
        type_version: 1,
        name: format!("f{id}"),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: inputs
            .iter()
            .map(|(k, r)| (k.to_string(), r.clone()))
            .collect(),
        suppressed: false,
        sketch: None,
    }
}

fn on_plane(id: u64, type_id: &str) -> FeatureRecord {
    let plane = Ref::Slot {
        feature: FeatureId(Id(1)),
        slot: SlotName::new("plane").unwrap(),
    };
    record(id, type_id, &[("plane", plane)])
}

fn doc(features: Vec<FeatureRecord>) -> Document {
    let mut plane = record(1, "core.datum-plane", &[]);
    plane
        .params
        .insert("offset".into(), Expr::parse("5 mm").unwrap());
    let features: Vec<_> = std::iter::once(plane).chain(features).collect();
    let part = Part {
        id: PartId(Id(100)),
        name: "Part".into(),
        history: features.iter().map(|f| f.id).collect(),
        features: features.into_iter().map(|f| (f.id, f)).collect(),
        rollback: None,
    };
    apply(&Document::default(), &Command::AddPart { part })
        .unwrap()
        .document
}

fn hash(registry: Registry, d: &Document, feature: u64) -> InputHash {
    match Evaluator::new(registry)
        .evaluate(d)
        .outcome(FeatureId(Id(feature)))
    {
        Some(FeatureOutcome::Ok { hash, .. }) => *hash,
        other => panic!("{other:?}"),
    }
}

#[test]
fn every_type_registers_under_the_plugins_namespace() {
    let mut r = Registry::with_core_types();
    let ids = register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.3"), demo()).unwrap();
    let ids: Vec<_> = ids.iter().map(FeatureTypeId::as_str).collect();
    assert_eq!(ids, ["demo.disc", "demo.stack", "demo.washer", "demo.boom"]);
    let all: Vec<_> = r.ids().map(FeatureTypeId::as_str).collect();
    assert_eq!(
        all,
        [
            "core.datum-plane",
            "core.sketch",
            "demo.boom",
            "demo.disc",
            "demo.stack",
            "demo.washer"
        ]
    );
}

#[test]
fn a_plugin_that_does_not_fit_is_refused_whole() {
    let mut r = Registry::with_core_types();
    let before: Vec<_> = r.ids().cloned().collect();
    let foreign = Arc::new(Demo { prefix: "other" });
    assert!(matches!(
        register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.3"), foreign),
        Err(HostError::Namespace { .. })
    ));
    // Built against a plugin API before this host's (ADR-0006, ADR-0007).
    for old in ["^0.1", "^0.2"] {
        assert!(matches!(
            register_tier0(&mut r, &manifest("demo", "0.1.0", old), demo()),
            Err(HostError::Api { .. })
        ));
    }
    let tier1 = manifest("demo", "0.1.0", "^0.3").replace("tier = 0", "tier = 1");
    assert!(matches!(
        register_tier0(&mut r, &tier1, demo()),
        Err(HostError::NotTier0 { tier: 1, .. })
    ));
    assert!(matches!(
        register_tier0(&mut r, "[plugin]", demo()),
        Err(HostError::Manifest(_))
    ));
    assert_eq!(r.ids().cloned().collect::<Vec<_>>(), before);
    register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.3"), demo()).unwrap();
    assert!(matches!(
        register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.3"), demo()),
        Err(HostError::Registry(_))
    ));
}

#[test]
fn a_plugin_feature_evaluates_like_a_built_in() {
    let d = doc(vec![on_plane(2, "demo.disc")]);
    let e = Evaluator::new(registry("0.1.0")).evaluate(&d);
    let Some(SlotView::Body(m)) = e.slot(FeatureId(Id(2)), "body") else {
        panic!("{:?}", e.events)
    };
    let expected = std::f64::consts::PI * 2e-3 * 2e-3 * 1e-3;
    assert!((m.volume - expected).abs() < 1e-9 * expected, "{m:?}");
    assert_eq!((m.faces, m.edges, m.vertices), (3, 3, 2));
}

#[test]
fn the_plugins_version_is_part_of_the_hash() {
    let d = doc(vec![on_plane(2, "demo.disc")]);
    let first = hash(registry("0.1.0"), &d, 2);
    assert_eq!(first, hash(registry("0.1.0"), &d, 2));
    assert_ne!(first, hash(registry("0.1.1"), &d, 2));
    assert_eq!(
        hash(registry("0.1.0"), &d, 1),
        hash(registry("0.1.1"), &d, 1),
        "a built-in's hash has no plugin version"
    );
}

#[test]
fn a_panicking_plugin_is_a_diagnostic_not_a_crash() {
    let d = doc(vec![on_plane(2, "demo.boom"), on_plane(3, "demo.disc")]);
    let mut ev = Evaluator::new(registry("0.1.0"));
    for _ in 0..2 {
        let e = ev.evaluate(&d);
        let Some(FeatureOutcome::Failed { diagnostic, .. }) = e.outcome(FeatureId(Id(2))) else {
            panic!("{:?}", e.events)
        };
        assert_eq!(diagnostic.code.as_str(), "plugin.panic");
        assert!(
            diagnostic.message.contains("the demo plugin's boom"),
            "{}",
            diagnostic.message
        );
        assert!(e.slot(FeatureId(Id(3)), "body").is_some());
    }
}

#[test]
fn a_plugin_type_takes_no_choices() {
    let mut disc = on_plane(2, "demo.disc");
    disc.choices.insert("world".into(), "xy".into());
    let e = Evaluator::new(registry("0.1.0")).evaluate(&doc(vec![disc]));
    let Some(FeatureOutcome::Failed { diagnostic, .. }) = e.outcome(FeatureId(Id(2))) else {
        panic!("{:?}", e.events)
    };
    assert_eq!(diagnostic.code.as_str(), "feature.unknown-choice");
}

#[test]
fn a_plugin_fuses_and_cuts_through_the_hosts_kernel() {
    let d = doc(vec![on_plane(2, "demo.stack"), on_plane(3, "demo.washer")]);
    let e = Evaluator::new(registry("0.1.0")).evaluate(&d);
    let body = |f| match e.slot(FeatureId(Id(f)), "body") {
        Some(SlotView::Body(m)) => *m,
        _ => panic!("{:?}", e.events),
    };
    let (r, t) = (2e-3, 1e-3);
    let disc = |radius: f64, height: f64| std::f64::consts::PI * radius * radius * height;
    let close = |a: f64, b: f64| (a - b).abs() < 1e-9 * b;
    // The stub stands flush on the disc: the disc's wall, bottom and top
    // ring, the stub's wall and top.
    let stack = body(2);
    assert!(
        close(stack.volume, disc(r, t) + disc(r / 2.0, t)),
        "{stack:?}"
    );
    assert_eq!(stack.faces, 5, "{stack:?}");
    let washer = body(3);
    assert!(
        close(washer.volume, disc(r, t) - disc(r / 2.0, t)),
        "{washer:?}"
    );
    assert_eq!(washer.faces, 4, "{washer:?}");
}
