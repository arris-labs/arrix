//! The sketch's drawing tools: line and rectangle, with inference
//! (docs/UI-RENDERING.md §Sketch mode). The pointer's snap is
//! `arrix_sketch::snap_search` over the solved draft, drawn as the tool's
//! preview; a click places what the preview showed and binds the same
//! inferences by the commit rule (`arrix_sketch::commit_inferences`). A
//! click that adds geometry is one `SketchEdit`, diffed between the solved
//! draft before it and after: both carry the resolved values of the
//! sketch's expressions, so only what the click made rides along.

use arrix_config::{SKETCH_ARC, SKETCH_CIRCLE, SKETCH_LINE, SKETCH_RECTANGLE, SKETCH_SELECT};
use arrix_doc::Command;
use arrix_sketch::{
    Constraint, Draft, Entity, EntityId, GlyphKind, Inferred, Point, PointId, Sketch, SketchEdit,
    Snap, SnapQuery, commit_inferences, snap_search,
};
use arrix_ui::{CurveShape, PlanePointer, SketchPreviewView, SnapGlyphView};

use crate::ArrixApp;
use crate::sketch_view::Solved;

type Uv = [f64; 2];

/// How long the pointer rests on a point or a line to acquire it as an
/// alignment source, in seconds of egui's clock.
pub const DWELL: f64 = 0.3;

/// The most acquired sources kept; the oldest goes first.
const MAX_ACQUIRED: usize = 8;

/// The tool sketch mode's pointer is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    Line,
    Rectangle,
    Circle,
    Arc,
}

impl Tool {
    /// The tool's command id.
    pub fn command(self) -> &'static str {
        match self {
            Self::Select => SKETCH_SELECT,
            Self::Line => SKETCH_LINE,
            Self::Rectangle => SKETCH_RECTANGLE,
            Self::Circle => SKETCH_CIRCLE,
            Self::Arc => SKETCH_ARC,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Line => "Line",
            Self::Rectangle => "Rectangle",
            Self::Circle => "Circle",
            Self::Arc => "Arc",
        }
    }
}

/// What a drawing tool holds between its clicks.
#[derive(Clone, Debug, PartialEq)]
pub enum Pending {
    /// The line tool's first click: where, and what its snap would bind
    /// once the point exists.
    LineStart { at: Uv, inferred: Vec<Inferred> },
    /// A chain of lines: the last segment's end, and the chain's first
    /// point, which a segment closing on it ends the chain at.
    Chain { from: PointId, first: PointId },
    /// The rectangle's first corner.
    Corner { at: Uv, inferred: Vec<Inferred> },
    /// The circle's centre, awaiting the click that gives the radius.
    Centre { at: Uv, inferred: Vec<Inferred> },
    /// The arc's first end.
    ArcStart { at: Uv, inferred: Vec<Inferred> },
    /// The arc's two ends, awaiting the point it passes through.
    ArcEnd {
        start: Uv,
        start_inferred: Vec<Inferred>,
        end: Uv,
        end_inferred: Vec<Inferred>,
    },
}

/// A drawing tool's state: session state, per open sketch.
#[derive(Clone, Debug, Default)]
pub struct Drawing {
    pub tool: Tool,
    pub pending: Option<Pending>,
    /// The pointer as last seen in the plane; `None` off the viewport.
    pub pointer: Option<PlanePointer>,
    /// Points and lines acquired as alignment sources.
    pub acquired_points: Vec<PointId>,
    pub acquired_lines: Vec<EntityId>,
    /// What the pointer rests on, and since when.
    dwell: Option<(arrix_sketch::SketchEntityId, f64)>,
}

impl Drawing {
    /// The tool `tool`, with nothing pending or acquired.
    pub fn with(tool: Tool) -> Self {
        Self {
            tool,
            ..Self::default()
        }
    }

    /// Lets go of what an edit or an undo took out of `sketch`: an
    /// acquired source, a chain whose end is gone.
    pub fn forget_missing(&mut self, sketch: &Sketch) {
        self.acquired_points.retain(|p| sketch.point(*p).is_some());
        self.acquired_lines.retain(|e| sketch.entity(*e).is_some());
        if let Some(Pending::Chain { from, first }) = self.pending
            && (sketch.point(from).is_none() || sketch.point(first).is_none())
        {
            self.pending = None;
        }
    }

    /// Where the pending segment starts, when the tool's band has a
    /// direction (the line tool's; a rectangle's diagonal has none).
    fn direction_from(&self, sketch: &Sketch) -> Option<Uv> {
        match self.pending.as_ref()? {
            Pending::LineStart { at, .. } => Some(*at),
            Pending::Chain { from, .. } => sketch.point(*from).map(Point::pos),
            _ => None,
        }
    }

    /// The snap for `pointer` over `sketch`: nothing snapped while the
    /// modifier is held.
    pub fn snap(&self, sketch: &Sketch, pointer: &PlanePointer) -> Snap {
        let cursor = (pointer.at[0], pointer.at[1]);
        if pointer.suppress {
            return Snap::raw(cursor);
        }
        // The arc's through point is a place, not a snap: nothing it
        // could bind exists yet.
        if matches!(self.pending, Some(Pending::ArcEnd { .. })) {
            return Snap::raw(cursor);
        }
        let from = self.direction_from(sketch);
        let snap = snap_search(
            sketch,
            &SnapQuery {
                cursor,
                tol: pointer.tol,
                direction_from: from.map(|p| (p[0], p[1])),
                excluded: None,
                acquired_points: &self.acquired_points,
                acquired_lines: &self.acquired_lines,
                relations: self.tool == Tool::Line && from.is_some(),
            },
        );
        // A circle's radius click binds the one thing a rim can: a point
        // it passes through. Any other snap would be drawn and not kept.
        match (&self.pending, snap.inferences.as_slice()) {
            (Some(Pending::Centre { .. }), [Inferred::OnPoint(_)]) => snap,
            (Some(Pending::Centre { .. }), _) => Snap::raw(cursor),
            _ => snap,
        }
    }

    /// Follows the pointer: what it rests on for [`DWELL`] seconds is
    /// acquired. Returns when the dwell under way completes, for a repaint
    /// then.
    pub fn follow(&mut self, sketch: &Sketch, pointer: Option<PlanePointer>) -> Option<f64> {
        self.pointer = pointer;
        let pointer = pointer?;
        let snap = self.snap(sketch, &pointer);
        let resting = snap.inferences.iter().find_map(|i| match *i {
            Inferred::OnPoint(p) => Some(p),
            Inferred::OnCurve(e) if sketch.entity(e).is_some_and(Entity::is_line) => Some(e),
            _ => None,
        });
        let Some(id) = resting else {
            self.dwell = None;
            return None;
        };
        match self.dwell {
            Some((on, since)) if on == id => {
                if pointer.time - since < DWELL {
                    return Some(since + DWELL);
                }
                let list = if sketch.point(id).is_some() {
                    &mut self.acquired_points
                } else {
                    &mut self.acquired_lines
                };
                acquire(list, id);
                None
            }
            _ => {
                self.dwell = Some((id, pointer.time));
                Some(pointer.time + DWELL)
            }
        }
    }

    /// The preview of the pointer's snap: what the next click draws.
    pub fn preview(&self, sketch: &Sketch) -> Option<SketchPreviewView> {
        let pointer = self.pointer.filter(|_| self.tool != Tool::Select)?;
        let snap = self.snap(sketch, &pointer);
        let cursor = [snap.uv.0, snap.uv.1];
        let radius = matches!(self.pending, Some(Pending::Centre { .. }));
        let curves = match &self.pending {
            Some(Pending::Corner { at, .. }) => rectangle(*at, cursor)
                .windows(2)
                .map(|w| line(w[0], w[1]))
                .collect(),
            Some(Pending::Centre { at, .. }) => {
                let r = (cursor[0] - at[0]).hypot(cursor[1] - at[1]);
                (r > 0.0)
                    .then(|| CurveShape::Circle {
                        centre: round(*at),
                        radius: round1(r),
                    })
                    .into_iter()
                    .collect()
            }
            Some(Pending::ArcStart { at, .. }) => vec![line(*at, cursor)],
            Some(Pending::ArcEnd { start, end, .. }) => {
                vec![arc_shape(*start, cursor, *end).unwrap_or_else(|| line(*start, *end))]
            }
            _ => self
                .direction_from(sketch)
                .map(|from| line(from, cursor))
                .into_iter()
                .collect(),
        };
        Some(SketchPreviewView {
            tool: self.tool.command().to_owned(),
            cursor: round(cursor),
            curves,
            glyphs: snap
                .glyphs
                .iter()
                .map(|g| SnapGlyphView {
                    at: round([g.uv.0, g.uv.1]),
                    kind: glyph_kind(g.kind).to_owned(),
                    symbol: glyph_symbol(g.kind).to_owned(),
                    commits: g
                        .inferences
                        .iter()
                        .map(|i| match snap.inferences[*i] {
                            // On a circle's radius click, a point is on the
                            // rim, not coincident with the centre.
                            Inferred::OnPoint(_) if radius => "point_on_circle".to_owned(),
                            inferred => commits(sketch, inferred).to_owned(),
                        })
                        .collect(),
                })
                .collect(),
            guides: snap
                .guides
                .iter()
                .map(|g| {
                    [
                        round([g.line[0].0, g.line[0].1]),
                        round([g.line[1].0, g.line[1].1]),
                    ]
                })
                .collect(),
            acquired: self
                .acquired_points
                .iter()
                .filter_map(|p| sketch.point(*p).map(|p| round(p.pos())))
                .collect(),
        })
    }

    /// The status line's help for the tool.
    pub fn hint(&self) -> String {
        let name = self.tool.name();
        match (&self.tool, &self.pending) {
            (Tool::Line, None) => format!("{name}: click where the line starts."),
            (Tool::Line, Some(_)) => {
                format!("{name}: click where the segment ends. Escape or a double-click ends it.")
            }
            (Tool::Rectangle, None) => format!("{name}: click the first corner."),
            (Tool::Rectangle, Some(_)) => format!("{name}: click the opposite corner."),
            (Tool::Circle, None) => format!("{name}: click the centre."),
            (Tool::Circle, Some(_)) => format!("{name}: click a point on the rim."),
            (Tool::Arc, None) => format!("{name}: click where the arc starts."),
            (Tool::Arc, Some(Pending::ArcStart { .. })) => {
                format!("{name}: click where the arc ends.")
            }
            (Tool::Arc, Some(_)) => format!("{name}: click a point the arc passes through."),
            (Tool::Select, _) => String::new(),
        }
    }
}

/// Puts `id` last in `list`, dropping the oldest past the cap.
fn acquire(list: &mut Vec<arrix_sketch::SketchEntityId>, id: arrix_sketch::SketchEntityId) {
    list.retain(|x| *x != id);
    list.push(id);
    if list.len() > MAX_ACQUIRED {
        list.remove(0);
    }
}

fn round1(x: f64) -> f64 {
    (x * 1e9).round() / 1e9
}

fn round(p: Uv) -> Uv {
    p.map(|x| (x * 1e9).round() / 1e9)
}

fn line(from: Uv, to: Uv) -> CurveShape {
    CurveShape::Line {
        from: round(from),
        to: round(to),
    }
}

/// The rectangle's corners from `a` to the opposite `b`, closed: `a`, the
/// corner level with it, `b`, the corner plumb with it, `a` again.
fn rectangle(a: Uv, b: Uv) -> [Uv; 5] {
    [a, [b[0], a[1]], b, [a[0], b[1]], a]
}

fn glyph_kind(kind: GlyphKind) -> &'static str {
    match kind {
        GlyphKind::Point => "point",
        GlyphKind::Center => "center",
        GlyphKind::Origin => "origin",
        GlyphKind::Midpoint => "midpoint",
        GlyphKind::Horizontal => "horizontal",
        GlyphKind::Vertical => "vertical",
        GlyphKind::AxisU => "axis-u",
        GlyphKind::AxisV => "axis-v",
        GlyphKind::OnCurve => "on-curve",
        GlyphKind::Intersection => "intersection",
        GlyphKind::Quadrant => "quadrant",
        GlyphKind::Parallel => "parallel",
        GlyphKind::Perpendicular => "perpendicular",
        GlyphKind::Tangent => "tangent",
    }
}

fn glyph_symbol(kind: GlyphKind) -> &'static str {
    match kind {
        GlyphKind::Point | GlyphKind::Center => "C",
        GlyphKind::Origin | GlyphKind::AxisU | GlyphKind::AxisV => "O",
        GlyphKind::Midpoint => "M",
        GlyphKind::Horizontal => "H",
        GlyphKind::Vertical => "V",
        GlyphKind::OnCurve => "on",
        GlyphKind::Intersection => "X",
        GlyphKind::Quadrant => "Q",
        GlyphKind::Parallel => "||",
        GlyphKind::Perpendicular => "_|",
        GlyphKind::Tangent => "T",
    }
}

/// The constraint kind `inferred` commits, as the document names it.
fn commits(sketch: &Sketch, inferred: Inferred) -> &'static str {
    match inferred {
        Inferred::OnPoint(_) => "coincident",
        Inferred::Origin => "coincident_to_datum",
        Inferred::Midpoint(_) => "midpoint",
        Inferred::Horizontal => "horizontal",
        Inferred::Vertical => "vertical",
        Inferred::AlignedH(_) => "horizontal_points",
        Inferred::AlignedV(_) => "vertical_points",
        Inferred::OnAxis(_) => "point_on_datum",
        Inferred::OnCurve(e) if sketch.entity(e).is_some_and(Entity::is_line) => "point_on_line",
        Inferred::OnCurve(_) => "point_on_circle",
        Inferred::Parallel(_) => "parallel",
        Inferred::Perpendicular(_) => "perpendicular",
        Inferred::Tangent(_) => "tangent",
    }
}

/// One click of a drawing tool on `draft` at `snap`: the pending state
/// advances, and geometry the click completes is drawn into the draft with
/// the inferences bound by the commit rule. Whether it drew any.
fn draw(drawing: &mut Drawing, draft: &mut Draft, snap: &Snap, tol: f64) -> bool {
    let at = [snap.uv.0, snap.uv.1];
    let near = |p: Uv| (p[0] - at[0]).hypot(p[1] - at[1]) <= tol;
    match (drawing.tool, drawing.pending.take()) {
        (Tool::Select, _) => false,
        (
            Tool::Line,
            None
            | Some(
                Pending::Corner { .. }
                | Pending::Centre { .. }
                | Pending::ArcStart { .. }
                | Pending::ArcEnd { .. },
            ),
        ) => {
            drawing.pending = Some(Pending::LineStart {
                at,
                inferred: snap.inferences.clone(),
            });
            false
        }
        (
            Tool::Line,
            Some(Pending::LineStart {
                at: start,
                inferred,
            }),
        ) => {
            if near(start) {
                drawing.pending = Some(Pending::LineStart {
                    at: start,
                    inferred,
                });
                return false;
            }
            let first = draft.add_point(Point::new(start[0], start[1]));
            let end = segment(draft, first, at, &inferred, &snap.inferences);
            drawing.pending = Some(Pending::Chain { from: end, first });
            for p in [first, end] {
                acquire(&mut drawing.acquired_points, p);
            }
            true
        }
        (Tool::Line, Some(Pending::Chain { from, first })) => {
            // A click on the chain's end, the second of a double-click
            // among them, ends the chain.
            if draft.point(from).is_none_or(|p| near(p.pos())) {
                return false;
            }
            let end = segment(draft, from, at, &[], &snap.inferences);
            acquire(&mut drawing.acquired_points, end);
            if !snap.inferences.contains(&Inferred::OnPoint(first)) {
                drawing.pending = Some(Pending::Chain { from: end, first });
            }
            true
        }
        (
            Tool::Rectangle,
            None
            | Some(
                Pending::LineStart { .. }
                | Pending::Chain { .. }
                | Pending::Centre { .. }
                | Pending::ArcStart { .. }
                | Pending::ArcEnd { .. },
            ),
        ) => {
            drawing.pending = Some(Pending::Corner {
                at,
                inferred: snap.inferences.clone(),
            });
            false
        }
        (Tool::Rectangle, Some(Pending::Corner { at: a, inferred })) => {
            if (a[0] - at[0]).abs() <= tol || (a[1] - at[1]).abs() <= tol {
                drawing.pending = Some(Pending::Corner { at: a, inferred });
                return false;
            }
            add_rectangle(draft, a, at, &inferred, &snap.inferences);
            true
        }
        (
            Tool::Circle,
            Some(Pending::Centre {
                at: centre,
                inferred,
            }),
        ) => {
            let radius = (centre[0] - at[0]).hypot(centre[1] - at[1]);
            if radius <= tol {
                drawing.pending = Some(Pending::Centre {
                    at: centre,
                    inferred,
                });
                return false;
            }
            let c = add_circle(draft, centre, radius, &inferred, &snap.inferences);
            acquire(&mut drawing.acquired_points, c);
            true
        }
        (Tool::Circle, _) => {
            drawing.pending = Some(Pending::Centre {
                at,
                inferred: snap.inferences.clone(),
            });
            false
        }
        (
            Tool::Arc,
            Some(Pending::ArcStart {
                at: start,
                inferred,
            }),
        ) => {
            drawing.pending = Some(if near(start) {
                Pending::ArcStart {
                    at: start,
                    inferred,
                }
            } else {
                Pending::ArcEnd {
                    start,
                    start_inferred: inferred,
                    end: at,
                    end_inferred: snap.inferences.clone(),
                }
            });
            false
        }
        (
            Tool::Arc,
            Some(Pending::ArcEnd {
                start,
                start_inferred,
                end,
                end_inferred,
            }),
        ) => {
            let Some((centre, ccw)) = circle_through(start, at, end) else {
                drawing.pending = Some(Pending::ArcEnd {
                    start,
                    start_inferred,
                    end,
                    end_inferred,
                });
                return false;
            };
            let ends = add_arc(
                draft,
                [start, end],
                centre,
                ccw,
                [&start_inferred, &end_inferred],
            );
            for p in ends {
                acquire(&mut drawing.acquired_points, p);
            }
            true
        }
        (Tool::Arc, _) => {
            drawing.pending = Some(Pending::ArcStart {
                at,
                inferred: snap.inferences.clone(),
            });
            false
        }
    }
}

/// The rectangle from corner `a` to `b`: four lines on four shared
/// corners, the level sides `Horizontal` and the others `Vertical`, then
/// the inferences of `a` and of `b` by the commit rule.
fn add_rectangle(draft: &mut Draft, a: Uv, b: Uv, at_a: &[Inferred], at_b: &[Inferred]) {
    let corners = rectangle(a, b);
    let [p0, p1, p2, p3] =
        [0, 1, 2, 3].map(|i| draft.add_point(Point::new(corners[i][0], corners[i][1])));
    for (i, (start, end)) in [(p0, p1), (p1, p2), (p2, p3), (p3, p0)]
        .into_iter()
        .enumerate()
    {
        let side = draft.add_entity(Entity::Line { start, end });
        draft.add_constraint(if i % 2 == 0 {
            Constraint::Horizontal { line: side }
        } else {
            Constraint::Vertical { line: side }
        });
    }
    let bound: Vec<_> = at_a
        .iter()
        .filter_map(|i| i.bind(draft, p0, None))
        .chain(at_b.iter().filter_map(|i| i.bind(draft, p2, None)))
        .collect();
    commit_inferences(draft, &bound);
}

/// A circle about `centre`, with the centre's inferences and, from the
/// radius click, a point the rim passes through. Returns the centre.
fn add_circle(
    draft: &mut Draft,
    centre: Uv,
    radius: f64,
    at_centre: &[Inferred],
    at_rim: &[Inferred],
) -> PointId {
    let c = draft.add_point(Point::new(centre[0], centre[1]));
    let circle = draft.add_entity(Entity::Circle { center: c, radius });
    let mut bound: Vec<_> = at_centre
        .iter()
        .filter_map(|i| i.bind(draft, c, None))
        .collect();
    bound.extend(at_rim.iter().filter_map(|i| match *i {
        Inferred::OnPoint(p) => Inferred::OnCurve(circle).bind(draft, p, None),
        _ => None,
    }));
    commit_inferences(draft, &bound);
    c
}

/// An arc about `centre` between two ends, running counter-clockwise from
/// the first when `ccw` and from the second when not, with each end's
/// inferences. Returns the ends.
fn add_arc(
    draft: &mut Draft,
    ends: [Uv; 2],
    centre: Uv,
    ccw: bool,
    inferred: [&[Inferred]; 2],
) -> [PointId; 2] {
    let [s, e] = ends.map(|p| draft.add_point(Point::new(p[0], p[1])));
    let c = draft.add_point(Point::new(centre[0], centre[1]));
    let (start, end) = if ccw { (s, e) } else { (e, s) };
    draft.add_entity(Entity::Arc {
        center: c,
        start,
        end,
    });
    let bound: Vec<_> = inferred[0]
        .iter()
        .filter_map(|i| i.bind(draft, s, None))
        .chain(inferred[1].iter().filter_map(|i| i.bind(draft, e, None)))
        .collect();
    commit_inferences(draft, &bound);
    [s, e]
}

/// The centre of the circle through `s`, `t` and `e`, and whether the
/// counter-clockwise arc from `s` to `e` is the one through `t`; `None`
/// when the three are on a line.
fn circle_through(s: Uv, t: Uv, e: Uv) -> Option<(Uv, bool)> {
    let (b, c) = ([t[0] - s[0], t[1] - s[1]], [e[0] - s[0], e[1] - s[1]]);
    let d = 2.0 * (b[0] * c[1] - b[1] * c[0]);
    let (bb, cc) = (b[0] * b[0] + b[1] * b[1], c[0] * c[0] + c[1] * c[1]);
    if d.abs() <= 1e-6 * bb.sqrt() * cc.sqrt() {
        return None;
    }
    let u = [(c[1] * bb - b[1] * cc) / d, (b[0] * cc - c[0] * bb) / d];
    Some(([s[0] + u[0], s[1] + u[1]], d > 0.0))
}

/// The arc from `s` to `e` through `t`, as the view draws one.
fn arc_shape(s: Uv, t: Uv, e: Uv) -> Option<CurveShape> {
    let (centre, ccw) = circle_through(s, t, e)?;
    let (from, to) = if ccw { (s, e) } else { (e, s) };
    let angle = |p: Uv| (p[1] - centre[1]).atan2(p[0] - centre[0]);
    Some(CurveShape::Arc {
        centre: round(centre),
        radius: round1((s[0] - centre[0]).hypot(s[1] - centre[1])),
        start: round1(angle(from)),
        end: round1(angle(to)),
    })
}

/// A segment from `from` to a new point at `at`, with the inferences of
/// its start (`at_start`, bound to `from`) and of its end bound by the
/// commit rule, the start's first. Returns the new end.
fn segment(
    draft: &mut Draft,
    from: PointId,
    at: Uv,
    at_start: &[Inferred],
    at_end: &[Inferred],
) -> PointId {
    let end = draft.add_point(Point::new(at[0], at[1]));
    let line = draft.add_entity(Entity::Line { start: from, end });
    let bound: Vec<_> = at_start
        .iter()
        .filter_map(|i| i.bind(draft, from, Some(line)))
        .chain(at_end.iter().filter_map(|i| i.bind(draft, end, Some(line))))
        .collect();
    commit_inferences(draft, &bound);
    end
}

impl ArrixApp {
    /// `core.sketch.select`, `.line`, `.rectangle`: the tool becomes
    /// `tool`, with nothing pending or acquired.
    pub(crate) fn set_tool(&mut self, tool: Tool) {
        if let Some(mode) = self.active_sketch_mut() {
            let pointer = mode.drawing.pointer;
            mode.drawing = Drawing::with(tool);
            mode.drawing.pointer = pointer;
            mode.hover = None;
        }
    }

    /// `core.sketch.tool.cancel` (Escape with a drawing tool): drops what
    /// is pending, or with nothing pending returns to the select tool.
    pub(crate) fn cancel_tool(&mut self) {
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        if mode.drawing.pending.take().is_none() {
            self.set_tool(Tool::Select);
        }
    }

    /// The pointer moved in the plane, or left it.
    pub(crate) fn sketch_pointer(&mut self, pointer: Option<PlanePointer>) {
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let Some(solved) = &mode.solved else {
            mode.drawing.pointer = pointer;
            return;
        };
        if mode.drawing.tool == Tool::Select {
            mode.drawing.pointer = pointer;
            return;
        }
        let wake = mode.drawing.follow(&solved.sketch, pointer);
        self.session.wake_at = wake;
    }

    /// A drawing tool's click at `pointer`: the snap there is placed, and
    /// a click that completes geometry submits it as one `SketchEdit`.
    pub(crate) fn draw_click(&mut self, pointer: PlanePointer) {
        let Some(document) = self.documents.get_mut(self.active) else {
            return;
        };
        let base = document.replica.generation();
        let Some(mode) = &mut document.sketch else {
            return;
        };
        let Some(solved) = &mode.solved else {
            return;
        };
        let before = solved.sketch.clone();
        let snap = mode.drawing.snap(&before, &pointer);
        let mut draft = Draft::with_sketch(before.clone(), self.ids.clone());
        let mut drawing = mode.drawing.clone();
        if !draw(&mut drawing, &mut draft, &snap, pointer.tol) {
            mode.drawing = drawing;
            return;
        }
        let result = draft.solve();
        if !result.converged {
            self.session.notice =
                Some("The sketch did not solve with that; nothing was drawn.".into());
            return;
        }
        let Draft { sketch: after, ids } = draft;
        let command = Command::SketchEdit {
            feature: mode.feature,
            edit: SketchEdit::diff(&before, &after),
        };
        match document.submit(command) {
            Ok(changed) => {
                self.ids = ids;
                let Some(mode) = &mut document.sketch else {
                    return;
                };
                mode.drawing = drawing;
                if changed {
                    mode.solved = Some(Solved::local(base, after));
                }
            }
            Err(reason) => self.session.notice = Some(reason),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrix_sketch::Point;

    /// The commit rule on the click's path: an inference the sketch cannot
    /// meet is neither committed nor in the way of the others. The search
    /// places the cursor where its inferences hold, so this one is made by
    /// hand: level between two fixed points at different heights.
    #[test]
    fn a_conflicting_inference_is_not_committed() {
        let mut draft = Draft::seeded(3);
        let a = draft.add_point(Point::fixed(0.0, 0.0));
        let b = draft.add_point(Point::fixed(0.03, 0.01));
        let mut drawing = Drawing {
            pending: Some(Pending::LineStart {
                at: [0.0, 0.0],
                inferred: vec![Inferred::OnPoint(a)],
            }),
            ..Drawing::with(Tool::Line)
        };
        let mut snap = Snap::raw((0.03, 0.01));
        snap.inferences = vec![Inferred::OnPoint(b), Inferred::Horizontal];
        assert!(draw(&mut drawing, &mut draft, &snap, 1e-4));
        assert_eq!(
            draft.constraints().len(),
            2,
            "the two coincidences, not the level"
        );
        assert!(
            draft
                .constraints()
                .values()
                .all(|r| matches!(r.constraint, Constraint::Coincident { .. }))
        );
    }
}
