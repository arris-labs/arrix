//! References picked into a form's fields (docs/UI-RENDERING.md §Declarative
//! plugin UI, ADR-0018). The active reference field decides what a pick
//! means: a face of the scene is a plane, or the body it is on; an edge an
//! edge or an axis; a vertex a point; in the tree a datum plane is a plane,
//! a feature a tool, a body's maker a body. A pick that fits nothing says
//! what the field takes and changes nothing. A create form's pick edits the
//! draft; an edit form's is one `EditFeature`.

use arrix_core::{FeatureId, Input, PersistentName, Ref, TopoKind};
use arrix_doc::{Document, FeatureEdit};
use arrix_plugin_api::{FeatureTypeSpec, InputKind, SlotKind};
use arrix_ui::PartSketchItem;
use arrix_viewport::BodyKey;

use crate::ArrixApp;
use crate::form::{CANDIDATES_SHOWN, feature_record, lost_refs, standing, takes};

/// What the user picked, before a field decides what it means.
enum Pick {
    Topo(PersistentName, Option<BodyKey>),
    Feature(FeatureId),
    Sketch(FeatureId, PartSketchItem),
}

/// The feature named `id` in `doc`, with its name.
fn feature_name(doc: &Document, id: FeatureId) -> String {
    doc.parts()
        .values()
        .find_map(|p| p.features.get(&id))
        .map_or_else(|| id.to_string(), |r| r.name.clone())
}

/// The feature a topological name is rooted at.
fn root(name: &PersistentName) -> FeatureId {
    name.root.feature()
}

/// A reference in words, for the field that holds it.
pub(crate) fn label(doc: &Document, r: &Ref) -> String {
    match r {
        Ref::Topo(name) => {
            let what = match name.kind {
                TopoKind::Face => "Face",
                TopoKind::Edge => "Edge",
                TopoKind::Vertex => "Vertex",
            };
            format!("{what} of {}", feature_name(doc, root(name)))
        }
        Ref::Slot { feature, slot } => format!("{} ({slot})", feature_name(doc, *feature)),
        Ref::Feature(feature) => feature_name(doc, *feature),
        Ref::Region { feature, .. } => format!("Region of {}", feature_name(doc, *feature)),
        Ref::Sketch { feature, .. } => format!("Sketch item of {}", feature_name(doc, *feature)),
        Ref::Param(_) => "Parameter".into(),
        Ref::Plugin { .. } => "Plugin record".into(),
    }
}

/// What `pick` means to a field that takes `kind`, if anything.
fn fit(
    kind: InputKind,
    pick: &Pick,
    spec: impl Fn(FeatureId) -> Option<FeatureTypeSpec>,
) -> Option<Ref> {
    match (pick, kind) {
        (Pick::Topo(name, _), InputKind::Plane) if name.kind == TopoKind::Face => {
            Some(Ref::Topo(name.clone()))
        }
        (Pick::Topo(name, Some(body)), InputKind::Body) if name.kind == TopoKind::Face => {
            Some(Ref::Slot {
                feature: body.feature,
                slot: body.slot.clone(),
            })
        }
        (Pick::Topo(name, _), InputKind::Edge | InputKind::Axis) if name.kind == TopoKind::Edge => {
            Some(Ref::Topo(name.clone()))
        }
        (Pick::Topo(name, _), InputKind::Point) if name.kind == TopoKind::Vertex => {
            Some(Ref::Topo(name.clone()))
        }
        (Pick::Feature(id), InputKind::Plane) => {
            let slot = spec(*id)?
                .outputs
                .iter()
                .find(|o| o.kind == SlotKind::Plane)?
                .name
                .clone();
            Some(Ref::Slot { feature: *id, slot })
        }
        (Pick::Feature(id), InputKind::Body) => {
            let slot = spec(*id)?
                .outputs
                .iter()
                .find(|o| o.kind == SlotKind::Body && o.modifies.is_none())?
                .name
                .clone();
            Some(Ref::Slot { feature: *id, slot })
        }
        (Pick::Feature(id), InputKind::Tool) => Some(Ref::Feature(*id)),
        (Pick::Sketch(feature, PartSketchItem::Region { key }), InputKind::Region) => {
            Some(Ref::Region {
                feature: *feature,
                key: serde_json::from_str(key).ok()?,
            })
        }
        (
            Pick::Sketch(feature, PartSketchItem::Point { id } | PartSketchItem::Line { id }),
            InputKind::Point | InputKind::Axis,
        ) => Some(Ref::Sketch {
            feature: *feature,
            entity: id.parse().ok()?,
        }),
        _ => None,
    }
}

/// What a list field holds after `r` is picked into it: shift adds it or,
/// when it is there, removes it; without shift the list becomes `r`. A
/// single field becomes `r`. `None` is the field empty.
fn after_pick(held: Option<&Input>, list: bool, r: Ref, extend: bool) -> Option<Input> {
    if !list {
        return Some(Input::One(r));
    }
    let mut refs = match (extend, held) {
        (true, Some(input)) => input.refs().to_vec(),
        _ => Vec::new(),
    };
    match refs.iter().position(|x| *x == r) {
        Some(at) => {
            refs.remove(at);
        }
        None => refs.push(r),
    }
    (!refs.is_empty()).then_some(Input::Many(refs))
}

/// `held` with its lost references replaced by `r`, at the place of the
/// first, unless `r` is already in the list.
fn rebind(held: &Input, lost: &[Ref], r: Ref) -> Input {
    let mut refs = Vec::new();
    let mut placed = held.refs().contains(&r);
    for x in held.refs() {
        if !lost.contains(x) {
            refs.push(x.clone());
        } else if !placed {
            refs.push(r.clone());
            placed = true;
        }
    }
    Input::Many(refs)
}

impl ArrixApp {
    fn feature_spec(&self, id: FeatureId) -> Option<FeatureTypeSpec> {
        let record = feature_record(self.active_document()?, id)?;
        self.feature_types
            .iter()
            .find(|t| t.id == record.type_id.as_str())
            .cloned()
    }

    /// Every reference the open form's fields hold, and an edit form's
    /// diagnostic names or offers.
    pub(crate) fn form_refs(&self) -> Vec<Ref> {
        let Some(document) = self.active_document() else {
            return Vec::new();
        };
        let Some(form) = &document.form else {
            return Vec::new();
        };
        let record = form.edit.and_then(|f| feature_record(document, f));
        let mut refs: Vec<Ref> = form
            .held(record)
            .values()
            .flat_map(|input| input.refs().iter().cloned())
            .collect();
        // What the feature's diagnostic is about, and what a lost
        // reference may be re-picked as, are shown with them.
        if let Some(d) = form
            .edit
            .and_then(|f| standing(document, f))
            .and_then(|s| s.diagnostic)
        {
            refs.extend(d.refs);
            refs.extend(d.candidates);
        }
        let mut seen = std::collections::HashSet::new();
        refs.retain(|r| seen.insert(r.clone()));
        refs
    }

    /// What the open form's reference fields hold in the scene, to draw as
    /// the selection is.
    pub(crate) fn form_marks(&self) -> Vec<PersistentName> {
        self.form_refs()
            .into_iter()
            .filter_map(|r| match r {
                Ref::Topo(name) => Some(name),
                _ => None,
            })
            .collect()
    }

    /// A click on an item of a finished sketch while a field asks for one.
    pub(crate) fn form_pick_sketch(
        &mut self,
        feature: FeatureId,
        item: PartSketchItem,
        extend: bool,
    ) {
        if self.field_asks() {
            self.form_pick(&Pick::Sketch(feature, item), extend);
        }
    }

    /// A click in the scene while a field asks: puts the entity under it in
    /// the field, or says what the field takes. True when a field was
    /// asking, so the click selects nothing.
    pub(crate) fn form_pick_topo(&mut self, name: Option<PersistentName>) -> bool {
        if !self.field_asks() {
            return false;
        }
        let Some(name) = name else {
            return true;
        };
        let view = &self.active_document().expect("a form is open").view;
        let extend = view.click_extend;
        let body = view.scene.body_of(&[], &name).cloned();
        self.form_pick(&Pick::Topo(name, body), extend);
        true
    }

    /// A click on a feature's row in the tree while a field asks.
    pub(crate) fn form_pick_feature(&mut self, feature: FeatureId) -> bool {
        if !self.field_asks() {
            return false;
        }
        let extend = self.session.shift;
        self.form_pick(&Pick::Feature(feature), extend);
        true
    }

    fn field_asks(&self) -> bool {
        self.active_document()
            .and_then(|d| d.form.as_ref())
            .is_some_and(|f| f.active.is_some())
    }

    /// The pick into the active field, as the field takes it.
    fn form_pick(&mut self, pick: &Pick, extend: bool) {
        let Some((field, title, kind, list)) = self.active_field() else {
            return;
        };
        let spec = |id| self.feature_spec(id);
        let Some(r) = fit(kind, pick, spec) else {
            let form_title = self.form_title();
            self.session.notice = Some(format!(
                "{form_title}: the {title} field takes: {}.",
                takes(kind)
            ));
            return;
        };
        self.put_reference(&field, list, r, extend);
    }

    /// The active field's name, title, kind and whether it is a list.
    pub(crate) fn active_field(&self) -> Option<(String, String, InputKind, bool)> {
        let document = self.active_document()?;
        let form = document.form.as_ref()?;
        let active = form.active.as_deref()?;
        let record = form.edit.and_then(|f| feature_record(document, f));
        let i = form.shown_inputs(record).find(|i| i.name == active)?;
        Some((i.name.clone(), i.title.clone(), i.kind, i.list))
    }

    fn form_title(&self) -> String {
        self.active_document()
            .and_then(|d| d.form.as_ref())
            .map(|f| f.spec.title.clone())
            .unwrap_or_default()
    }

    /// `r` into `field`: the draft's, or one `EditFeature`.
    fn put_reference(&mut self, field: &str, list: bool, r: Ref, extend: bool) {
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(form) = document.form.as_ref() else {
            return;
        };
        let record = form.edit.and_then(|f| feature_record(document, f));
        let lost = form
            .edit
            .and_then(|f| standing(document, f))
            .and_then(|s| s.diagnostic)
            .map(|d| lost_refs(&d).to_vec())
            .unwrap_or_default();
        let held = form.held(record).get(field);
        let next = match held {
            // A pick into a list that holds a lost reference re-binds it
            // in place; the rest of the list stays.
            Some(input) if list && input.refs().iter().any(|x| lost.contains(x)) => {
                Some(rebind(input, &lost, r))
            }
            _ => after_pick(held, list, r, extend),
        };
        self.set_reference(field, next);
    }

    /// The candidate at `index` of a lost reference's list was taken for
    /// `field`: the pick it would have been, one `EditFeature`.
    pub(crate) fn pick_candidate(&mut self, field: &str, index: usize) {
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(form) = document.form.as_ref() else {
            return;
        };
        let Some(feature) = form.edit else {
            return;
        };
        let record = feature_record(document, feature);
        let holds_lost = standing(document, feature)
            .and_then(|s| s.diagnostic)
            .filter(|d| {
                form.held(record)
                    .get(field)
                    .is_some_and(|i| i.refs().iter().any(|r| lost_refs(d).contains(r)))
            });
        let Some(r) = holds_lost.and_then(|d| d.candidates.get(index).cloned()) else {
            return;
        };
        if index >= CANDIDATES_SHOWN {
            return;
        }
        let list = form.spec.inputs.iter().any(|i| i.name == field && i.list);
        self.put_reference(field, list, r, false);
    }

    /// The value at `index` of a reference field was removed.
    pub(crate) fn remove_reference(&mut self, field: &str, index: usize) {
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(form) = document.form.as_ref() else {
            return;
        };
        let record = form.edit.and_then(|f| feature_record(document, f));
        let mut refs = form
            .held(record)
            .get(field)
            .map(|i| i.refs().to_vec())
            .unwrap_or_default();
        if index >= refs.len() {
            return;
        }
        refs.remove(index);
        let list = form.spec.inputs.iter().any(|i| i.name == field && i.list);
        let next = match (refs.pop(), list) {
            (None, _) => None,
            (Some(only), false) => Some(Input::One(only)),
            (Some(last), true) => {
                refs.push(last);
                Some(Input::Many(refs))
            }
        };
        self.set_reference(field, next);
    }

    /// Sets `field` to `next` (`None` empties it): the draft takes it, an
    /// edit form commits it, and an equal value is none.
    fn set_reference(&mut self, field: &str, next: Option<Input>) {
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(form) = document.form.as_ref() else {
            return;
        };
        let edit = form.edit;
        let held = edit
            .and_then(|f| feature_record(document, f))
            .map_or(&form.inputs, |r| &r.inputs)
            .get(field)
            .cloned();
        if held == next {
            return;
        }
        match edit {
            None => {
                if let Some(form) = self.form_mut() {
                    match next {
                        Some(input) => form.inputs.insert(field.to_owned(), input),
                        None => form.inputs.remove(field),
                    };
                }
            }
            Some(feature) => {
                let mut change = FeatureEdit::default();
                change.inputs.insert(field.to_owned(), next);
                self.edit_feature(feature, change);
            }
        }
    }

    /// A create form's first fields take what is selected when it opens:
    /// the scene's selection, then the tree's, each into the first empty
    /// field it fits.
    pub(crate) fn seed_form(&mut self) {
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(form) = document.form.as_ref() else {
            return;
        };
        let mut picks = Vec::new();
        if let Some(name) = &document.view.selection {
            let body = document.view.scene.body_of(&[], name).cloned();
            picks.push(Pick::Topo(name.clone(), body));
        }
        if let Some(feature) = document.view.feature_selection {
            picks.push(Pick::Feature(feature));
        }
        let fields: Vec<_> = form
            .shown_inputs(None)
            .map(|i| (i.name.clone(), i.kind))
            .collect();
        let mut seeded = Vec::new();
        for pick in &picks {
            for (name, kind) in &fields {
                let taken = seeded.iter().any(|(s, _)| s == name);
                if !taken && let Some(r) = fit(*kind, pick, |id| self.feature_spec(id)) {
                    seeded.push((name.clone(), r));
                    break;
                }
            }
        }
        if let Some(form) = self.form_mut() {
            for (name, r) in seeded {
                form.inputs.insert(name, Input::One(r));
            }
        }
    }
}
