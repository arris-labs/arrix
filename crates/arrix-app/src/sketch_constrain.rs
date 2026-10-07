//! Constraints from the selection (docs/UI-RENDERING.md §Sketch mode): one
//! `core.sketch.constrain.<kind>` command per kind a selection can make,
//! enabled only for a selection that fits it. A constraint is checked
//! against the sketch first (`Sketch::check_candidate`): a redundant or a
//! conflicting one is refused with the reason in the status line and no
//! command; the rest are added, the draft solved so the geometry meets
//! them, and the change is one `SketchEdit`.
//!
//! `core.sketch.construction` (Q) is here too: it turns the selected curves
//! to construction, or back when every one already is.

use std::collections::BTreeSet;

use arrix_config::SKETCH_CONSTRAIN_PREFIX;
use arrix_sketch::{
    CandidateVerdict, Constraint, DatumEntity, Draft, Entity, EntityId, PointId, Sketch,
    SketchEntityId,
};

use crate::ArrixApp;

/// What a selection holds, counted: how many points, lines, circles and
/// arcs, and anything else (a lone point entity, a constraint's mark).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shape {
    pub points: u8,
    pub lines: u8,
    pub rounds: u8,
    pub others: u8,
}

impl Shape {
    pub fn of(sketch: &Sketch, selection: &BTreeSet<SketchEntityId>) -> Self {
        let mut shape = Self::default();
        let count = |n: &mut u8| *n = n.saturating_add(1);
        for id in selection {
            if sketch.point(*id).is_some() {
                count(&mut shape.points);
            } else {
                match sketch.entity(*id) {
                    Some(Entity::Line { .. }) => count(&mut shape.lines),
                    Some(Entity::Circle { .. } | Entity::Arc { .. }) => count(&mut shape.rounds),
                    _ => count(&mut shape.others),
                }
            }
        }
        shape
    }

    /// Whether anything in it is a curve.
    pub fn curves(self) -> bool {
        self.lines + self.rounds > 0
    }
}

/// The kinds of constraint the selection can make: the ones of
/// `arrix_config::CONSTRAIN_KINDS`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Coincident,
    Horizontal,
    Vertical,
    Parallel,
    Perpendicular,
    Equal,
    Concentric,
    Tangent,
    Symmetric,
    PointOn,
    Midpoint,
    AlignHorizontal,
    AlignVertical,
    Fix,
    Origin,
    XAxis,
    YAxis,
}

impl Kind {
    /// The kind a command id names, `None` for any other command.
    pub fn of_command(command: &str) -> Option<Self> {
        Some(match command.strip_prefix(SKETCH_CONSTRAIN_PREFIX)? {
            "coincident" => Self::Coincident,
            "horizontal" => Self::Horizontal,
            "vertical" => Self::Vertical,
            "parallel" => Self::Parallel,
            "perpendicular" => Self::Perpendicular,
            "equal" => Self::Equal,
            "concentric" => Self::Concentric,
            "tangent" => Self::Tangent,
            "symmetric" => Self::Symmetric,
            "point-on" => Self::PointOn,
            "midpoint" => Self::Midpoint,
            "align-horizontal" => Self::AlignHorizontal,
            "align-vertical" => Self::AlignVertical,
            "fix" => Self::Fix,
            "origin" => Self::Origin,
            "x-axis" => Self::XAxis,
            "y-axis" => Self::YAxis,
            _ => return None,
        })
    }

    /// Whether a selection of this shape makes the constraint.
    pub fn fits(self, s: Shape) -> bool {
        let (p, l, r) = (s.points, s.lines, s.rounds);
        s.others == 0
            && match self {
                Self::Coincident | Self::AlignHorizontal | Self::AlignVertical => {
                    (p, l, r) == (2, 0, 0)
                }
                Self::Horizontal | Self::Vertical => p == 0 && r == 0 && l >= 1,
                Self::Parallel | Self::Perpendicular => (p, l, r) == (0, 2, 0),
                Self::Equal => p == 0 && matches!((l, r), (2, 0) | (0, 2)),
                Self::Concentric => (p, l, r) == (0, 0, 2),
                Self::Tangent => p == 0 && matches!((l, r), (1, 1) | (0, 2)),
                Self::Symmetric => matches!((p, l, r), (2, 1, 0) | (3, 0, 0)),
                Self::PointOn => p == 1 && l + r == 1,
                Self::Midpoint => (p, l, r) == (1, 1, 0),
                Self::Fix => p >= 1 && l + r == 0,
                Self::Origin | Self::XAxis | Self::YAxis => (p, l, r) == (1, 0, 0),
            }
    }

    /// What it needs selected, for the status line when it is off.
    pub fn needs(self) -> &'static str {
        match self {
            Self::Coincident | Self::AlignHorizontal | Self::AlignVertical => {
                "needs two points selected"
            }
            Self::Horizontal | Self::Vertical => "needs lines selected",
            Self::Parallel | Self::Perpendicular => "needs two lines selected",
            Self::Equal => "needs two lines, or two circles or arcs, selected",
            Self::Concentric => "needs two circles or arcs selected",
            Self::Tangent => "needs a line and a circle or arc, or two circles or arcs, selected",
            Self::Symmetric => "needs two points and a line, or three points, selected",
            Self::PointOn => "needs a point and a curve selected",
            Self::Midpoint => "needs a point and a line selected",
            Self::Fix => "needs points selected",
            Self::Origin | Self::XAxis | Self::YAxis => "needs a point selected",
        }
    }

    /// The constraints this kind makes of `selection`, in a fixed order.
    /// Empty when the selection does not fit.
    fn make(self, sketch: &Sketch, selection: &BTreeSet<SketchEntityId>) -> Vec<Constraint> {
        if !self.fits(Shape::of(sketch, selection)) {
            return Vec::new();
        }
        let points: Vec<PointId> = selection
            .iter()
            .copied()
            .filter(|id| sketch.point(*id).is_some())
            .collect();
        let of = |keep: fn(&Entity) -> bool| -> Vec<EntityId> {
            selection
                .iter()
                .copied()
                .filter(|id| sketch.entity(*id).is_some_and(keep))
                .collect()
        };
        let lines = of(Entity::is_line);
        let rounds = of(|e| matches!(e, Entity::Circle { .. } | Entity::Arc { .. }));
        match self {
            Self::Coincident => vec![Constraint::Coincident {
                a: points[0],
                b: points[1],
            }],
            Self::AlignHorizontal => vec![Constraint::HorizontalPoints {
                a: points[0],
                b: points[1],
            }],
            Self::AlignVertical => vec![Constraint::VerticalPoints {
                a: points[0],
                b: points[1],
            }],
            Self::Horizontal => lines
                .into_iter()
                .map(|line| Constraint::Horizontal { line })
                .collect(),
            Self::Vertical => lines
                .into_iter()
                .map(|line| Constraint::Vertical { line })
                .collect(),
            Self::Parallel => vec![Constraint::Parallel {
                a: lines[0],
                b: lines[1],
            }],
            Self::Perpendicular => vec![Constraint::Perpendicular {
                a: lines[0],
                b: lines[1],
            }],
            Self::Equal => {
                let both = if lines.is_empty() { &rounds } else { &lines };
                vec![Constraint::Equal {
                    a: both[0],
                    b: both[1],
                }]
            }
            Self::Concentric => vec![Constraint::Concentric {
                a: rounds[0],
                b: rounds[1],
            }],
            Self::Tangent => match (lines.first(), rounds.as_slice()) {
                (Some(&line), [circle]) => vec![Constraint::Tangent {
                    line,
                    circle: *circle,
                }],
                _ => vec![Constraint::TangentCircles {
                    a: rounds[0],
                    b: rounds[1],
                }],
            },
            Self::Symmetric => match lines.first() {
                Some(&mirror) => vec![Constraint::Symmetric {
                    a: points[0],
                    b: points[1],
                    mirror,
                }],
                None => {
                    let (a, b, center) = centre_of(sketch, &points);
                    vec![Constraint::SymmetricPoints { a, b, center }]
                }
            },
            Self::PointOn => match (lines.first(), rounds.first()) {
                (Some(&line), _) => vec![Constraint::PointOnLine {
                    point: points[0],
                    line,
                }],
                (_, Some(&circle)) => vec![Constraint::PointOnCircle {
                    point: points[0],
                    circle,
                }],
                _ => Vec::new(),
            },
            Self::Midpoint => vec![Constraint::Midpoint {
                point: points[0],
                line: lines[0],
            }],
            Self::Fix => points
                .into_iter()
                .filter_map(|point| {
                    let p = sketch.point(point)?;
                    Some(Constraint::Fix {
                        point,
                        x: p.x,
                        y: p.y,
                    })
                })
                .collect(),
            Self::Origin => vec![Constraint::coincident_to_datum(
                points[0],
                DatumEntity::Origin,
            )],
            Self::XAxis => vec![Constraint::point_on_datum(points[0], DatumEntity::AxisX)],
            Self::YAxis => vec![Constraint::point_on_datum(points[0], DatumEntity::AxisY)],
        }
    }
}

/// Of three points, the two to mirror and the one they are mirrored about:
/// the one nearest the midpoint of the other two.
fn centre_of(sketch: &Sketch, points: &[PointId]) -> (PointId, PointId, PointId) {
    let pos = |id: PointId| sketch.point(id).map_or([0.0, 0.0], |p| p.pos());
    let off = |centre: usize| {
        let others: Vec<[f64; 2]> = (0..3)
            .filter(|i| *i != centre)
            .map(|i| pos(points[i]))
            .collect();
        let mid = [
            (others[0][0] + others[1][0]) / 2.0,
            (others[0][1] + others[1][1]) / 2.0,
        ];
        let c = pos(points[centre]);
        (c[0] - mid[0]).hypot(c[1] - mid[1])
    };
    let centre = (0..3)
        .min_by(|a, b| off(*a).total_cmp(&off(*b)))
        .unwrap_or(2);
    let rest: Vec<PointId> = (0..3).filter(|i| *i != centre).map(|i| points[i]).collect();
    (rest[0], rest[1], points[centre])
}

/// Why `verdict` keeps `kind`'s constraint out, in the status line's words.
fn refusal(verdict: &CandidateVerdict) -> &'static str {
    match verdict {
        CandidateVerdict::Redundant(_) => "The sketch already enforces that: nothing was added.",
        _ => "That cannot hold together with the sketch's constraints: nothing was added.",
    }
}

impl ArrixApp {
    /// `core.sketch.constrain.<kind>`: the selection's constraint, one
    /// command, or none and the reason in the status line.
    pub(crate) fn constrain(&mut self, kind: Kind) {
        let ids = self.ids.clone();
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let Some(solved) = &mode.solved else {
            return;
        };
        let before = solved.sketch.clone();
        let wanted = kind.make(&before, &mode.selection);
        let mut draft = Draft::with_sketch(before.clone(), ids);
        let mut reason = None;
        let mut added = 0;
        for constraint in wanted {
            let verdict = draft.check_candidate(&constraint);
            if !verdict.is_ok() {
                reason = Some(refusal(&verdict));
                continue;
            }
            draft.add_constraint(constraint);
            // The next one is checked against a sketch that meets this.
            if !draft.solve().converged {
                self.session.notice =
                    Some("The sketch did not solve with that: nothing was added.".into());
                return;
            }
            added += 1;
        }
        if added == 0 {
            self.session.notice = reason.map(str::to_owned);
            return;
        }
        self.submit_draft(&before, draft);
    }

    /// `core.sketch.construction`: the selected curves become construction,
    /// or solid when every one already is. One command.
    pub(crate) fn toggle_construction(&mut self) {
        let ids = self.ids.clone();
        let Some(mode) = self.active_sketch_mut() else {
            return;
        };
        let Some(solved) = &mode.solved else {
            return;
        };
        let sketch = &solved.sketch;
        let curves: Vec<EntityId> = mode
            .selection
            .iter()
            .copied()
            .filter(|id| sketch.entity(*id).is_some_and(Entity::is_curve))
            .collect();
        if curves.is_empty() {
            return;
        }
        let on = !curves.iter().all(|id| sketch.is_construction(*id));
        let before = sketch.clone();
        let mut draft = Draft::with_sketch(before.clone(), ids);
        for id in curves {
            draft.set_construction(id, on);
        }
        self.submit_draft(&before, draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrix_sketch::Point;

    fn shape(points: u8, lines: u8, rounds: u8) -> Shape {
        Shape {
            points,
            lines,
            rounds,
            others: 0,
        }
    }

    #[test]
    fn a_kind_fits_only_the_selections_it_names() {
        assert!(Kind::Equal.fits(shape(0, 2, 0)));
        assert!(Kind::Equal.fits(shape(0, 0, 2)));
        assert!(!Kind::Equal.fits(shape(0, 1, 1)), "a line is not a radius");
        assert!(Kind::Tangent.fits(shape(0, 1, 1)));
        assert!(Kind::Tangent.fits(shape(0, 0, 2)));
        assert!(!Kind::Tangent.fits(shape(0, 2, 0)));
        assert!(Kind::Symmetric.fits(shape(2, 1, 0)));
        assert!(Kind::Symmetric.fits(shape(3, 0, 0)));
        assert!(!Kind::Symmetric.fits(shape(2, 0, 0)));
        assert!(Kind::PointOn.fits(shape(1, 0, 1)));
        assert!(!Kind::Midpoint.fits(shape(1, 0, 1)), "a midpoint of a line");
        assert!(Kind::Horizontal.fits(shape(0, 3, 0)));
        assert!(!Kind::Horizontal.fits(shape(1, 1, 0)));
        let mut with_a_mark = shape(0, 2, 0);
        with_a_mark.others = 1;
        assert!(!Kind::Parallel.fits(with_a_mark), "a constraint's mark");
    }

    /// Of three points the one nearest the other two's midpoint is the
    /// centre.
    #[test]
    fn three_points_mirror_about_the_middle_one() {
        let mut draft = Draft::seeded(5);
        let a = draft.add_point(Point::new(0.0, 0.0));
        let c = draft.add_point(Point::new(0.011, 0.0));
        let b = draft.add_point(Point::new(0.02, 0.0));
        assert_eq!(centre_of(&draft, &[a, c, b]), (a, b, c));
        assert_eq!(centre_of(&draft, &[c, a, b]), (a, b, c));
    }
}
