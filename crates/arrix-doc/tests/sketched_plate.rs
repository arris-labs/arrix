//! The acceptance test of plans/c1-m1-sketch, headless and by commands
//! alone: a plate sketched and dimensioned by expressions over `w`, `h`
//! and `d`, evaluated, extruded through `arrix-kernel`, edited, its bore
//! deleted, and every command undone to the byte.

use arrix_core::{
    CurveKey, FeatureId, Frame, Id, IdMinter, NameRoot, ParamId, PartId, PersistentName,
    QuantityKind, Ref, RegionKey, SweepPartName, TopoKind,
};
use arrix_doc::{
    AuthorId, Authority, Command, CommandEnvelope, Document, Evaluation, Evaluator, FeatureOutcome,
    FeatureRecord, FeatureTypeId, Outcome, Param, Part, Registry, SketchView, SlotView, apply,
    expr::Expr, save,
};
use arrix_kernel::Kernel;
use arrix_sketch::{
    Constraint, Draft, Entity, EntityId, Point, ResolveRegion, Sketch, SketchEdit, curve_key,
};
use std::f64::consts::PI;

const ME: AuthorId = AuthorId(Id(1));
const MM: f64 = 1e-3;
const THICKNESS: f64 = 5.0 * MM;

fn registry() -> Registry {
    Registry::with_core_types()
}

/// The document as commands build it, every file saved after each.
struct Session {
    auth: Authority,
    evaluator: Evaluator,
    saves: Vec<arrix_doc::MemorySource>,
}

impl Session {
    fn new() -> Self {
        let auth = Authority::default();
        let saves = vec![save(auth.document())];
        Self {
            auth,
            evaluator: Evaluator::new(registry()),
            saves,
        }
    }

    /// Submits `command`, after checking that it and its inverse cross as
    /// JSON unchanged (step 5).
    fn submit(&mut self, command: Command) {
        let crosses = |c: &Command| {
            let json = serde_json::to_string(c).unwrap();
            assert_eq!(
                &serde_json::from_str::<Command>(&json).unwrap(),
                c,
                "{json}"
            );
        };
        crosses(&command);
        crosses(&apply(self.auth.document(), &command).unwrap().inverse);
        let envelope = CommandEnvelope {
            author: ME,
            base: self.auth.generation(),
            command,
        };
        match self.auth.submit(&envelope) {
            Ok(Outcome::Applied(_)) => {}
            other => panic!("{:?}: {other:?}", envelope.command),
        }
        self.saves.push(save(self.auth.document()));
    }

    fn evaluate(&mut self) -> Evaluation {
        self.evaluator.evaluate(self.auth.document())
    }

    fn document(&self) -> &Document {
        self.auth.document()
    }
}

/// The sketch feature's sketch as the document holds it.
fn stored(doc: &Document, sketch: FeatureId) -> Sketch {
    let (_, _, record) = doc.feature(sketch).unwrap();
    record.sketch.as_deref().unwrap().clone()
}

fn view(e: &Evaluation, sketch: FeatureId) -> &SketchView {
    match e.slot(sketch, "sketch") {
        Some(SlotView::Sketch(view)) => view,
        other => panic!("{:?}: {other:?}", e.outcome(sketch)),
    }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs()
}

/// `(w·h − π·d²/4)·t`, in mm to m.
fn plate_volume(w: f64, h: f64, d: f64) -> f64 {
    (w * h - PI * d * d / 4.0) * THICKNESS * MM * MM
}

/// The region with the bore, extruded straight through `arrix-kernel`:
/// its volume, and whether every side face is named by the entity that
/// swept it.
fn extrude_directly(v: &SketchView, key: &RegionKey, sides: &[EntityId]) -> f64 {
    let region = key.resolve(&v.sketch).expect("the key resolves");
    let profile = region.to_profile(&v.sketch, v.plane).unwrap();
    let feature = FeatureId(Id(900));
    let mut k = Kernel::new();
    let body = k.extrude(feature, &profile, THICKNESS).unwrap();
    let names = k.names(body).unwrap();
    for entity in sides {
        let side = PersistentName {
            kind: TopoKind::Face,
            root: NameRoot::Sweep {
                feature,
                part: SweepPartName::Side(curve_key(*entity, None)),
            },
            chain: vec![],
        };
        assert!(names.contains(&side), "no side face named by {entity}");
        assert_eq!(curve_key(*entity, None), CurveKey(entity.0));
    }
    k.mass_properties(body).unwrap().volume
}

#[test]
fn a_sketched_plate() {
    let mut ids = IdMinter::new(2026);
    let (w, h, d): (ParamId, ParamId, ParamId) = (ids.mint(), ids.mint(), ids.mint());
    let part: PartId = ids.mint();
    let (sketch, pad): (FeatureId, FeatureId) = (ids.mint(), ids.mint());
    let mut s = Session::new();

    // 1. Parameters, a part and a sketch on world XY, then the sketch by
    // `SketchEdit`s alone: drawn, constrained, dimensioned, bored. Each
    // edit is the author's draft before and after the gesture, solved on
    // the author's side (ADR-0005).
    let param = |name: &str, expr: &str| Param {
        name: name.into(),
        kind: QuantityKind::Length,
        expr: expr.parse().unwrap(),
    };
    s.submit(Command::AddParam {
        param: w,
        record: param("w", "40 mm"),
    });
    s.submit(Command::AddParam {
        param: h,
        record: param("h", "30 mm"),
    });
    s.submit(Command::AddParam {
        param: d,
        record: param("d", "10 mm"),
    });
    s.submit(Command::AddPart {
        part: Part {
            id: part,
            name: "Plate".into(),
            history: vec![],
            features: Default::default(),
            rollback: None,
        },
    });
    let sketch_record = FeatureRecord {
        id: sketch,
        type_id: FeatureTypeId::new("core.sketch").unwrap(),
        type_version: 1,
        name: "Sketch".into(),
        params: Default::default(),
        choices: Default::default(),
        inputs: Default::default(),
        suppressed: false,
        sketch: Some(Box::default()),
    };
    s.submit(Command::AddFeature {
        part,
        at: 0,
        record: sketch_record.clone(),
    });

    let mut draft = Draft::seeded(7);
    let gesture = |s: &mut Session, draft: &Draft| {
        let edit = SketchEdit::diff(&stored(s.document(), sketch), draft);
        s.submit(Command::SketchEdit {
            feature: sketch,
            edit,
        });
    };
    let corners = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)];
    let p = corners.map(|(x, y)| draft.add_point(Point::new(x * MM, y * MM)));
    let sides: Vec<EntityId> = (0..4)
        .map(|i| {
            draft.add_entity(Entity::Line {
                start: p[i],
                end: p[(i + 1) % 4],
            })
        })
        .collect();
    gesture(&mut s, &draft);

    draft.point_mut(p[0]).unwrap().fixed = true;
    for (i, line) in sides.iter().enumerate() {
        draft.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line: *line }
        } else {
            Constraint::Vertical { line: *line }
        });
    }
    gesture(&mut s, &draft);

    let (center, bore) = draft.add_circle(20.0 * MM, 15.0 * MM, 5.0 * MM);
    gesture(&mut s, &draft);

    let dimensions = [
        (
            Constraint::Distance {
                a: p[0],
                b: p[1],
                value: 40.0 * MM,
            },
            "w",
        ),
        (
            Constraint::Distance {
                a: p[1],
                b: p[2],
                value: 30.0 * MM,
            },
            "h",
        ),
        (
            Constraint::HorizontalDistance {
                a: p[0],
                b: center,
                value: 20.0 * MM,
            },
            "w / 2",
        ),
        (
            Constraint::VerticalDistance {
                a: p[0],
                b: center,
                value: 15.0 * MM,
            },
            "h / 2",
        ),
        (
            Constraint::Diameter {
                target: bore,
                value: 10.0 * MM,
            },
            "d",
        ),
    ];
    for (c, expr) in dimensions {
        let id = draft.add_constraint(c);
        draft.set_constraint_expr(id, Some(expr.into()));
    }
    assert!(draft.solve().converged);
    gesture(&mut s, &draft);

    // 2. Evaluated: fully constrained, nothing redundant or in conflict,
    // the holed plate one region; extruded, the volume and the names.
    let e = s.evaluate();
    let v = view(&e, sketch);
    assert_eq!(v.plane, Frame::WORLD_XY);
    assert_eq!((v.dof, v.redundant.len()), (0, 0));
    let holed: Vec<_> = v
        .regions
        .iter()
        .filter(|r| r.profile.holes().len() == 1)
        .collect();
    assert_eq!(holed.len(), 1, "the holed plate is one region");
    let key = holed[0].key.clone();
    assert!(key.entities.contains(&bore));
    let mut sides_and_bore = sides.clone();
    sides_and_bore.push(bore);
    let volume = extrude_directly(v, &key, &sides_and_bore);
    assert!(close(volume, plate_volume(40.0, 30.0, 10.0)), "{volume}");

    // The feature that holds the key.
    s.submit(Command::AddFeature {
        part,
        at: 1,
        record: FeatureRecord {
            id: pad,
            type_id: FeatureTypeId::new("core.extrude").unwrap(),
            name: "Pad".into(),
            params: [("distance".into(), Expr::parse("5 mm").unwrap())].into(),
            inputs: [(
                "region".into(),
                Ref::Region {
                    feature: sketch,
                    key: key.clone(),
                },
            )]
            .into(),
            sketch: None,
            ..sketch_record
        },
    });
    let pad_volume = |e: &Evaluation| match e.slot(pad, "body") {
        Some(SlotView::Body(m)) => m.volume,
        _ => panic!("{:?}", e.outcome(pad)),
    };
    let e = s.evaluate();
    assert!(close(pad_volume(&e), plate_volume(40.0, 30.0, 10.0)));

    // 3. `w` to 50 mm: the sketch re-solves from its stored positions, the
    // same key resolves, and the volume follows.
    s.submit(Command::SetParam {
        param: w,
        expr: "50 mm".parse().unwrap(),
    });
    let e = s.evaluate();
    let v = view(&e, sketch);
    assert_eq!(v.dof, 0);
    let corner = v.sketch.point(p[2]).unwrap().pos();
    assert!(close(corner[0], 50.0 * MM) && close(corner[1], 30.0 * MM));
    let volume = extrude_directly(v, &key, &sides_and_bore);
    assert!(close(volume, plate_volume(50.0, 30.0, 10.0)), "{volume}");
    assert!(close(pad_volume(&e), plate_volume(50.0, 30.0, 10.0)));

    // 4. The circle deleted, with what stands on it: the holed plate's key
    // resolves to nothing, and the pad gets `ref.lost` with the plain
    // rectangle as a candidate, never as its region.
    let remove = SketchEdit::remove(&stored(s.document(), sketch), &[bore].into());
    s.submit(Command::SketchEdit {
        feature: sketch,
        edit: remove,
    });
    let e = s.evaluate();
    let v = view(&e, sketch);
    assert!(key.resolve(&v.sketch).is_none());
    assert_eq!(v.regions.len(), 1, "the plain rectangle");
    let rectangle = v.regions[0].key.clone();
    assert_ne!(rectangle, key);
    let Some(FeatureOutcome::Failed { diagnostic, .. }) = e.outcome(pad) else {
        panic!("{:?}", e.outcome(pad));
    };
    assert_eq!(diagnostic.code.as_str(), "ref.lost");
    assert_eq!(
        diagnostic.candidates,
        [Ref::Region {
            feature: sketch,
            key: rectangle
        }]
    );

    // 5. Every command undone restores the earlier saved bytes, and redone
    // the later ones.
    for want in s.saves.iter().rev().skip(1) {
        s.auth.undo(ME).unwrap();
        assert_eq!(save(s.auth.document()).0, want.0);
    }
    for want in s.saves.iter().skip(1) {
        s.auth.redo(ME).unwrap();
        assert_eq!(save(s.auth.document()).0, want.0);
    }
}
