//! Dimensions (docs/UI-RENDERING.md §Sketch mode): the dimension tool
//! makes one from what is picked, placed by a click, and a double-click on
//! a label opens the field its value is typed in as an expression. Both are
//! one `SketchEdit`. A dimension that would over-constrain is placed as a
//! reference dimension (shown, measured, not driving), the draft solved
//! with what the user typed before the command is made, and a text that
//! does not parse or a solve that fails leaves the field open with the
//! reason and issues nothing.

use arrix_core::QuantityKind;
use arrix_doc::expr::{Expr, ExprError};
use arrix_sketch::{
    CandidateVerdict, Constraint, ConstraintId, Draft, Entity, EntityId, PointId, Sketch,
    SketchEntityId,
};
use arrix_ui::{DimensionFieldView, HitKind, PlanePointer, SketchHit};

use crate::ArrixApp;
use crate::sketch_view::format_quantity;

type Uv = [f64; 2];

/// What a dimension tool pick is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Point(PointId),
    Line(EntityId),
    Round(EntityId),
}

impl Pick {
    pub fn of(sketch: &Sketch, id: SketchEntityId) -> Option<Self> {
        if sketch.point(id).is_some() {
            return Some(Self::Point(id));
        }
        match sketch.entity(id)? {
            Entity::Line { .. } => Some(Self::Line(id)),
            Entity::Circle { .. } | Entity::Arc { .. } => Some(Self::Round(id)),
            _ => None,
        }
    }
}

/// Whether `picks` can be dimensioned as they stand: a line, a circle or
/// arc, two points, a point and a line, two lines.
pub fn complete(picks: &[Pick]) -> bool {
    matches!(
        picks,
        [Pick::Line(_) | Pick::Round(_)]
            | [Pick::Point(_), Pick::Point(_) | Pick::Line(_)]
            | [Pick::Line(_), Pick::Point(_) | Pick::Line(_)]
    )
}

/// Whether `next` can join `picks` toward a dimension.
pub fn extends(picks: &[Pick], next: Pick) -> bool {
    let mut with = picks.to_vec();
    with.push(next);
    picks.is_empty() || complete(&with)
}

/// What a dimension states, as the status line names it.
fn dimension_kind(constraint: &Constraint) -> &'static str {
    match constraint {
        Constraint::Distance { .. } => "distance",
        Constraint::HorizontalDistance { .. } => "horizontal distance",
        Constraint::VerticalDistance { .. } => "vertical distance",
        Constraint::DistancePointLine { .. } => "distance from a point to a line",
        Constraint::DistanceParallelLines { .. } => "distance between parallel lines",
        Constraint::Angle { .. } => "angle",
        Constraint::Radius { .. } => "radius",
        Constraint::Diameter { .. } => "diameter",
        _ => "dimension",
    }
}

fn line_points(sketch: &Sketch, line: EntityId) -> Option<(PointId, PointId)> {
    match sketch.entity(line)? {
        Entity::Line { start, end } => Some((*start, *end)),
        _ => None,
    }
}

/// The dimension `picks` make, its label placed at `at`: for two points,
/// horizontal when the label stands above or below them and vertical
/// beside; for a circle or arc, a radius, or with `diameter` its
/// diameter. The value is what the geometry measures now.
pub fn make(sketch: &Sketch, picks: &[Pick], at: Uv, diameter: bool) -> Option<Constraint> {
    let pos = |id: PointId| sketch.point(id).map(arrix_sketch::Point::pos);
    let seed = match *picks {
        [Pick::Line(line)] => {
            let (a, b) = line_points(sketch, line)?;
            Constraint::Distance { a, b, value: 0.0 }
        }
        [Pick::Round(target)] if diameter => Constraint::Diameter { target, value: 0.0 },
        [Pick::Round(target)] => Constraint::Radius { target, value: 0.0 },
        [Pick::Point(a), Pick::Point(b)] => {
            let (pa, pb) = (pos(a)?, pos(b)?);
            let offset = [at[0] - (pa[0] + pb[0]) / 2.0, at[1] - (pa[1] + pb[1]) / 2.0];
            if offset[1].abs() >= offset[0].abs() {
                Constraint::HorizontalDistance { a, b, value: 0.0 }
            } else {
                Constraint::VerticalDistance { a, b, value: 0.0 }
            }
        }
        [Pick::Point(point), Pick::Line(line)] | [Pick::Line(line), Pick::Point(point)] => {
            Constraint::DistancePointLine {
                point,
                line,
                value: 0.0,
            }
        }
        [Pick::Line(a), Pick::Line(b)] => {
            let (a0, a1) = line_points(sketch, a)?;
            let (b0, b1) = line_points(sketch, b)?;
            let da = sub(pos(a1)?, pos(a0)?);
            let db = sub(pos(b1)?, pos(b0)?);
            let sin =
                (da[0] * db[1] - da[1] * db[0]).abs() / (da[0].hypot(da[1]) * db[0].hypot(db[1]));
            if sin < 1e-3 {
                Constraint::DistanceParallelLines { a, b, value: 0.0 }
            } else {
                Constraint::Angle { a, b, value: 0.0 }
            }
        }
        _ => return None,
    };
    let mut measured = seed.clone();
    measured.set_dimensional_value(sketch.measure(&seed)?);
    Some(measured)
}

fn sub(a: Uv, b: Uv) -> Uv {
    [a[0] - b[0], a[1] - b[1]]
}

/// An open dimension field: session state, per open sketch.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldState {
    pub constraint: ConstraintId,
    pub text: String,
    pub error: Option<String>,
}

impl ArrixApp {
    /// A click with the dimension tool: on a point or a curve it is a pick
    /// when the picks can grow by it; with a dimension's picks complete, a
    /// click that is not such a pick places the label there. `diameter`
    /// (shift) makes a circle's a diameter.
    pub(crate) fn dimension_click(
        &mut self,
        hit: Option<SketchHit>,
        diameter: bool,
        pointer: Option<PlanePointer>,
    ) {
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let Some(solved) = &mode.solved else {
            return;
        };
        let sketch = &solved.sketch;
        let picks: Vec<Pick> = mode
            .drawing
            .picks
            .iter()
            .filter_map(|id| Pick::of(sketch, *id))
            .collect();
        let hit_kind = hit.as_ref().map(|h| h.kind);
        let hit_id = hit
            .filter(|h| matches!(h.kind, HitKind::Point | HitKind::Curve))
            .and_then(|h| h.id.parse::<SketchEntityId>().ok());
        let picked = hit_id.and_then(|id| Some((id, Pick::of(sketch, id)?)));
        if let Some((id, pick)) = picked
            && !mode.drawing.picks.contains(&id)
        {
            let together = extends(&picks, pick);
            if together {
                mode.drawing.picks.push(id);
            } else {
                // Not together with what is picked: start again from it.
                mode.drawing.picks = vec![id];
            }
            mode.selection = mode.drawing.picks.iter().copied().collect();
            if !together {
                self.session.notice = Some("Those cannot be dimensioned together.".into());
            }
            return;
        }
        // A pick already made, a mark or a label: not a place for the label.
        if picked.is_some()
            || hit_kind.is_some_and(|k| !matches!(k, HitKind::Point | HitKind::Curve))
        {
            return;
        }
        match (complete(&picks), pointer) {
            (true, Some(pointer)) => self.place_dimension(&picks, pointer.at, diameter),
            _ => {
                mode.drawing.picks.clear();
                mode.selection.clear();
            }
        }
    }

    /// Makes the dimension of `picks` with its label at `at`: driving, or a
    /// reference when the sketch already holds what it would say. One
    /// command.
    fn place_dimension(&mut self, picks: &[Pick], at: Uv, diameter: bool) {
        let ids = self.ids.clone();
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let Some(solved) = &mode.solved else {
            return;
        };
        let before = solved.sketch.clone();
        let Some(constraint) = make(&before, picks, at, diameter) else {
            self.session.notice = Some("Those cannot be dimensioned.".into());
            return;
        };
        if constraint.dimensional_value().is_none_or(|v| v <= 1e-12) {
            self.session.notice = Some("There is nothing there to measure.".into());
            return;
        }
        let name = dimension_kind(&constraint);
        let mut draft = Draft::with_sketch(before.clone(), ids);
        let reference = !matches!(draft.check_candidate(&constraint), CandidateVerdict::Ok);
        let id = if reference {
            draft.add_reference_constraint(constraint)
        } else {
            draft.add_constraint(constraint)
        };
        draft.set_constraint_display_pos(id, Some((at[0], at[1])));
        if reference {
            self.session.notice = Some(format!(
                "The sketch already holds that {name}: placed as a reference dimension."
            ));
        }
        if self.submit_draft(&before, draft)
            && let Some(mode) = self.active_sketch_mut()
        {
            mode.drawing.picks.clear();
            mode.selection.clear();
        }
    }

    /// A double-click: on a driving dimension's label it opens the field.
    pub(crate) fn open_dimension_field(&mut self, hit: Option<SketchHit>) {
        let Some(hit) = hit.filter(|h| h.kind == HitKind::Dimension) else {
            return;
        };
        let Ok(id) = hit.id.parse::<ConstraintId>() else {
            return;
        };
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let Some(record) = mode
            .solved
            .as_ref()
            .and_then(|s| s.sketch.get_constraint_record(id))
        else {
            return;
        };
        if record.is_reference() {
            self.session.notice =
                Some("That dimension is measured, not driving: there is no value to type.".into());
            return;
        }
        let (Some(kind), Some(value)) = (
            record.constraint.dimension_kind(),
            record.constraint.dimensional_value(),
        ) else {
            return;
        };
        let text = record
            .expr
            .clone()
            .unwrap_or_else(|| format_quantity(kind, value));
        mode.field = Some(FieldState {
            constraint: id,
            text,
            error: None,
        });
    }

    /// `DimensionCancelled`: the field closes.
    pub(crate) fn close_dimension_field(&mut self) {
        if let Some(mode) = self.active_sketch_mut() {
            mode.field = None;
        }
    }

    /// `DimensionCommitted`: `text` is an expression for the dimension's
    /// value. The draft is solved with it, and the change is one
    /// `SketchEdit`: the constraint with its expression and the positions
    /// the solve gave. A text that does not parse, an unknown name, a unit
    /// that is not the dimension's or a solve that fails leaves the field
    /// open with the reason; a text that is the value it has issues nothing.
    pub(crate) fn commit_dimension(&mut self, constraint: &str, text: &str) {
        let Ok(id) = constraint.parse::<ConstraintId>() else {
            return;
        };
        let ids = self.ids.clone();
        let Some(document) = self.documents.get(self.active) else {
            return;
        };
        let params = document.replica.document().params();
        let finished = document.replica.finished().map(|(_, v)| v);
        let kinds = |name: &str| params.values().find(|p| p.name == name).map(|p| p.kind);
        let values = |name: &str| {
            let (pid, _) = params.iter().find(|(_, p)| p.name == name)?;
            finished?.get(pid)?.as_ref().ok().map(|q| q.si)
        };
        let Some(mode) = document.sketch.as_ref() else {
            return;
        };
        let Some(solved) = &mode.solved else {
            return;
        };
        let before = solved.sketch.clone();
        let Some(kind) = before
            .get_constraint_record(id)
            .and_then(|r| r.constraint.dimension_kind())
        else {
            return;
        };
        let outcome = self.dimension_value(text, kind, &kinds, &values);
        let (value, expr) = match outcome {
            Ok(v) => v,
            Err(reason) => {
                self.field_error(reason);
                return;
            }
        };
        let current = before
            .get_constraint_record(id)
            .map(|r| (r.constraint.dimensional_value(), r.expr.clone()));
        if current == Some((Some(value), expr.clone())) {
            self.close_dimension_field();
            return;
        }
        let mut draft = Draft::with_sketch(before.clone(), ids);
        if let Some(c) = draft.get_constraint_mut(id) {
            c.set_dimensional_value(value);
        }
        draft.set_constraint_expr(id, expr);
        if !draft.solve().converged {
            self.field_error("The sketch does not solve with that value.".into());
            return;
        }
        if self.submit_draft(&before, draft) {
            self.close_dimension_field();
        }
    }

    /// `text` as an expression of `kind`: its value in SI, and the text to
    /// keep with the dimension, none when it names no parameter.
    fn dimension_value(
        &self,
        text: &str,
        kind: QuantityKind,
        kinds: &dyn Fn(&str) -> Option<QuantityKind>,
        values: &dyn Fn(&str) -> Option<f64>,
    ) -> Result<(f64, Option<String>), String> {
        let expr = Expr::parse(text).map_err(|e| e.to_string())?;
        let quantity = expr
            .quantity(kind, kinds, values)
            .map_err(|e: ExprError| e.to_string())?;
        let keep = (!expr.names().is_empty()).then(|| text.trim().to_owned());
        Ok((quantity.si, keep))
    }

    fn field_error(&mut self, reason: String) {
        if let Some(field) = self.active_sketch_mut().and_then(|m| m.field.as_mut()) {
            field.error = Some(reason);
        }
    }

    /// The field the viewport draws, if one is open.
    pub(crate) fn field_view(&self, view: &arrix_ui::SketchView) -> Option<DimensionFieldView> {
        let field = self.active_document()?.sketch.as_ref()?.field.as_ref()?;
        let id = field.constraint.to_string();
        let at = view.dimensions.iter().find(|d| d.id == id)?.at;
        Some(DimensionFieldView {
            id,
            at,
            text: field.text.clone(),
            error: field.error.clone(),
        })
    }
}
