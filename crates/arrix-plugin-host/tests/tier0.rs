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
/// `demo.boom`: panics.
struct Demo {
    prefix: &'static str,
}

impl Feature for Demo {
    fn describe(&self) -> Vec<FeatureTypeSpec> {
        let body = SlotSpec {
            name: SlotName::new("body").unwrap(),
            kind: SlotKind::Body,
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
        vec![spec("disc"), spec("boom")]
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
        let InputValue::Plane(plane) = inputs[0].value;
        let disc = ProfileLoop::Circle {
            key: CurveKey(Id(1)),
            center: DVec2::ZERO,
            radius: params[0].value.si,
        };
        let body = kernel.extrude(&Profile::new(plane, disc, vec![]).unwrap(), 1e-3)?;
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
    register_tier0(&mut r, &manifest("demo", version, "^0.1"), demo()).unwrap();
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
    let ids = register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.1"), demo()).unwrap();
    let ids: Vec<_> = ids.iter().map(FeatureTypeId::as_str).collect();
    assert_eq!(ids, ["demo.disc", "demo.boom"]);
    let all: Vec<_> = r.ids().map(FeatureTypeId::as_str).collect();
    assert_eq!(all, ["core.datum-plane", "demo.boom", "demo.disc"]);
}

#[test]
fn a_plugin_that_does_not_fit_is_refused_whole() {
    let mut r = Registry::with_core_types();
    let before: Vec<_> = r.ids().cloned().collect();
    let foreign = Arc::new(Demo { prefix: "other" });
    assert!(matches!(
        register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.1"), foreign),
        Err(HostError::Namespace { .. })
    ));
    assert!(matches!(
        register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.2"), demo()),
        Err(HostError::Api { .. })
    ));
    let tier1 = manifest("demo", "0.1.0", "^0.1").replace("tier = 0", "tier = 1");
    assert!(matches!(
        register_tier0(&mut r, &tier1, demo()),
        Err(HostError::NotTier0 { tier: 1, .. })
    ));
    assert!(matches!(
        register_tier0(&mut r, "[plugin]", demo()),
        Err(HostError::Manifest(_))
    ));
    assert_eq!(r.ids().cloned().collect::<Vec<_>>(), before);
    register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.1"), demo()).unwrap();
    assert!(matches!(
        register_tier0(&mut r, &manifest("demo", "0.1.0", "^0.1"), demo()),
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
