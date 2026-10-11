//! The sketch-and-extrude helpers the scenario tests build parts with
//! (`tests/slice.rs`, `tests/c2_m1.rs`).

use std::collections::BTreeMap;

use arrix_core::{FeatureId, PartId, Ref, RegionKey, SlotName};
use arrix_doc::{
    Command, FeatureOutcome, FeatureRecord, FeatureTypeId, SketchView, SlotView, expr::Expr,
};
use arrix_sketch::{Constraint, Draft, Entity, EntityId, Point, SketchEdit};

use super::Client;

const MM: f64 = 1e-3;

pub fn slot(feature: FeatureId, name: &str) -> Ref {
    Ref::Slot {
        feature,
        slot: SlotName::new(name).unwrap(),
    }
}

pub fn record(id: FeatureId, ty: &str, name: &str) -> FeatureRecord {
    FeatureRecord {
        id,
        type_id: FeatureTypeId::new(ty).unwrap(),
        type_version: 1,
        name: name.into(),
        params: BTreeMap::new(),
        choices: BTreeMap::new(),
        inputs: BTreeMap::new(),
        suppressed: false,
        sketch: None,
        frozen: None,
        file: None,
    }
}

pub fn with(
    mut r: FeatureRecord,
    params: &[(&str, &str)],
    inputs: &[(&str, Ref)],
) -> FeatureRecord {
    r.params = params
        .iter()
        .map(|(k, v)| (k.to_string(), Expr::parse(v).unwrap()))
        .collect();
    r.inputs = inputs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone().into()))
        .collect();
    r
}

pub fn view(c: &Client, sketch: FeatureId) -> &SketchView {
    match c.outcome(sketch) {
        FeatureOutcome::Ok { slots, .. } => match slots.values().next() {
            Some(SlotView::Sketch(v)) => v,
            other => panic!("feature {sketch}: {other:?}"),
        },
        other => panic!("feature {sketch}: {other:?}"),
    }
}

/// A body slot's volume in cubic millimetres.
pub fn volume(c: &Client, feature: FeatureId) -> f64 {
    match c.outcome(feature) {
        FeatureOutcome::Ok { slots, .. } => match slots.get(&SlotName::new("body").unwrap()) {
            Some(SlotView::Body(m)) => m.volume * 1e9,
            other => panic!("feature {feature}: {other:?}"),
        },
        other => panic!("feature {feature}: {other:?}"),
    }
}

pub fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

/// A sketch feature on `plane` (the world XY without one), with a
/// rectangle of `w` × `h` whose corner `(x, y)` is fixed, all in the
/// sketch's own millimetres; `w_expr` and `h_expr` are its dimensions'
/// expressions, if any. Returns the key of its one region.
pub fn rectangle_sketch(
    c: &mut Client,
    (feature, at): (FeatureId, usize),
    part: PartId,
    plane: Option<Ref>,
    (x, y, w, h): (f64, f64, f64, f64),
    exprs: (Option<&str>, Option<&str>),
) -> RegionKey {
    let mut r = record(feature, "core.sketch", &format!("Sketch {at}"));
    r.sketch = Some(Box::default());
    r.inputs
        .extend(plane.map(|p| ("plane".to_string(), p.into())));
    c.submit(Command::AddFeature {
        part,
        at,
        record: r,
    });
    let mut draft = Draft::seeded(7);
    let corners = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
    let p = corners.map(|(cx, cy)| draft.add_point(Point::new(cx * MM, cy * MM)));
    let sides: Vec<EntityId> = (0..4)
        .map(|i| {
            draft.add_entity(Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect();
    draft.point_mut(p[0]).unwrap().fixed = true;
    for (i, line) in sides.iter().enumerate() {
        draft.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line: *line }
        } else {
            Constraint::Vertical { line: *line }
        });
    }
    for (a, b, value, expr) in [(0, 1, w, exprs.0), (1, 2, h, exprs.1)] {
        let id = draft.add_constraint(Constraint::Distance {
            a: p[a],
            b: p[b],
            value: value * MM,
        });
        draft.set_constraint_expr(id, expr.map(String::from));
    }
    assert!(draft.solve().converged);
    let stored = {
        let (_, _, record) = c.replica.document().feature(feature).unwrap();
        record.sketch.as_deref().unwrap().clone()
    };
    let edit = SketchEdit::diff(&stored, &draft);
    c.submit(Command::SketchEdit { feature, edit });
    let v = view(c, feature);
    assert_eq!((v.dof, v.regions.len()), (0, 1));
    v.regions[0].key.clone()
}

pub fn extrude(
    id: FeatureId,
    name: &str,
    sketch: FeatureId,
    key: RegionKey,
    mode: &str,
) -> FeatureRecord {
    let mut r = with(
        record(id, "core.extrude", name),
        &[],
        &[(
            "region",
            Ref::Region {
                feature: sketch,
                key,
            },
        )],
    );
    r.choices.insert("mode".into(), mode.into());
    r
}
