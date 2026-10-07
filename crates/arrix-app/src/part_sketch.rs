//! Finished sketches in part mode (docs/UI-RENDERING.md §The viewport). A
//! sketch no feature consumes yet is drawn on its plane, curves and points
//! only; while a form's field asks for a region, a point or a line, every
//! sketch is drawn and the field's kind is what the pointer finds in them.
//! Everything here is session state: a pick goes to the form as any other.

use std::collections::BTreeMap;

use arrix_core::{FeatureId, Frame, Ref};
use arrix_doc::{FeatureOutcome, Generation, SlotView};
use arrix_plugin_api::InputKind;
use arrix_ui::{HitKind, PartSketchItem, SketchAsk, SketchHit, SketchView};

use crate::ArrixApp;
use crate::documents::OpenDocument;
use crate::sketch_view::{Solved, region_key, sketch_view};

/// A finished sketch as the evaluator last solved it.
pub struct PartSketch {
    generation: Generation,
    plane: Frame,
    /// Its curves, points and regions; no dimensions or glyphs.
    view: SketchView,
}

/// One sketch as the viewport draws and hit-tests it this frame.
pub struct PartOverlay {
    pub feature: FeatureId,
    pub plane: Frame,
    pub view: SketchView,
}

/// The finished sketches drawn this frame, and what the form's active field
/// asks of them.
#[derive(Default)]
pub struct PartSketches {
    pub overlays: Vec<PartOverlay>,
    pub ask: Option<SketchAsk>,
}

/// What the pointer did among the part-mode sketches this frame.
#[derive(Default)]
pub struct PartPointer {
    /// The item under the pointer, when that is not what the view already
    /// shows hovered.
    pub hover: Option<Option<(FeatureId, PartSketchItem)>>,
    /// A click on an item, and whether shift was held.
    pub click: Option<(FeatureId, PartSketchItem, bool)>,
}

/// What a field of `kind` takes from a sketch, if it takes anything.
fn asks(kind: InputKind) -> Option<SketchAsk> {
    match kind {
        InputKind::Region => Some(SketchAsk::Region),
        InputKind::Point => Some(SketchAsk::Point),
        InputKind::Axis => Some(SketchAsk::Axis),
        _ => None,
    }
}

/// Whether some feature of the document takes a region or an item of the
/// sketch `feature`: it is then spent, and hidden unless a field asks.
fn consumed(document: &OpenDocument, feature: FeatureId) -> bool {
    document
        .replica
        .document()
        .parts()
        .values()
        .flat_map(|p| p.features.values())
        .flat_map(|r| r.inputs.values())
        .flat_map(|i| i.refs())
        .any(|r| match r {
            Ref::Region { feature: f, .. } | Ref::Sketch { feature: f, .. } => *f == feature,
            _ => false,
        })
}

impl OpenDocument {
    /// Keeps `part_sketches` in step with the evaluation: a sketch is
    /// solved again only when a newer evaluation of it arrives.
    pub(crate) fn follow_part_sketches(&mut self) {
        let open = self.sketch.as_ref().map(|m| m.feature);
        let mut kept = BTreeMap::new();
        for part in self.replica.document().parts().values() {
            for id in part.history.iter().copied().filter(|id| Some(*id) != open) {
                let Some((generation, FeatureOutcome::Ok { slots, .. })) = self.replica.outcome(id)
                else {
                    continue;
                };
                let Some(SlotView::Sketch(sketch)) =
                    slots.values().find(|s| matches!(s, SlotView::Sketch(_)))
                else {
                    continue;
                };
                let cached = self
                    .part_sketches
                    .remove(&id)
                    .filter(|c| c.generation == generation);
                let entry = cached.unwrap_or_else(|| {
                    let mut view = sketch_view(&Solved::new(generation, sketch.sketch.clone()));
                    view.glyphs.clear();
                    view.dimensions.clear();
                    PartSketch {
                        generation,
                        plane: sketch.plane,
                        view,
                    }
                });
                kept.insert(id, entry);
            }
        }
        self.part_sketches = kept;
    }
}

impl ArrixApp {
    /// What the open form's active field takes from a sketch.
    pub(crate) fn sketch_ask(&self) -> Option<SketchAsk> {
        asks(self.active_field()?.2)
    }

    /// The sketches the viewport draws over the part and hit-tests: none in
    /// sketch mode, which draws its own.
    pub(crate) fn part_overlays(&self) -> Vec<PartOverlay> {
        let Some(document) = self.active_document() else {
            return Vec::new();
        };
        if document.sketch.is_some() {
            return Vec::new();
        }
        let ask = self.sketch_ask();
        let held = self.form_refs();
        document
            .part_sketches
            .iter()
            .filter(|(_, sketch)| !sketch.view.points.is_empty() || !sketch.view.curves.is_empty())
            .filter(|(id, _)| ask.is_some() || !consumed(document, **id))
            .map(|(id, sketch)| {
                let mut view = sketch.view.clone();
                if ask != Some(SketchAsk::Region) {
                    view.regions.clear();
                }
                for r in &held {
                    match r {
                        Ref::Region { feature, key } if feature == id => {
                            view.selected_regions.push(region_key(key));
                        }
                        Ref::Sketch { feature, entity } if feature == id => {
                            view.selected.push(entity.to_string());
                        }
                        _ => {}
                    }
                }
                match &document.view.part_hover {
                    Some((feature, PartSketchItem::Region { key })) if feature == id => {
                        view.hover_region = Some(key.clone());
                    }
                    Some((feature, PartSketchItem::Point { id: item })) if feature == id => {
                        view.hover = Some(SketchHit {
                            kind: HitKind::Point,
                            id: item.clone(),
                        });
                    }
                    Some((feature, PartSketchItem::Line { id: item })) if feature == id => {
                        view.hover = Some(SketchHit {
                            kind: HitKind::Curve,
                            id: item.clone(),
                        });
                    }
                    _ => {}
                }
                PartOverlay {
                    feature: *id,
                    plane: sketch.plane,
                    view,
                }
            })
            .collect()
    }

    pub(crate) fn part_sketches(&self) -> PartSketches {
        PartSketches {
            overlays: self.part_overlays(),
            ask: self.sketch_ask(),
        }
    }

    /// The pointer is over `item` of a sketch, or over none.
    pub(crate) fn hover_part_sketch(&mut self, hover: Option<(FeatureId, PartSketchItem)>) {
        if let Some(document) = self.documents.get_mut(self.active) {
            document.view.part_hover = hover;
        }
    }
}
