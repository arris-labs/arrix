//! Projected sketch geometry (docs/DATA-MODEL.md §Sketches): before the
//! solve, each entity with a source takes the place of its edge or vertex
//! projected onto the sketch's plane. The solver holds it there, so what
//! the solve and the input hash read is the body's geometry, not the
//! stored positions.

use arrix_core::{DVec2, Diagnostic, FeatureId, Frame, Ref, Severity};
use arrix_kernel::{Kernel, Projected};
use arrix_sketch::{Entity, EntityId, PointId, Sketch};

use super::Failure;
use super::resolve::{Scope, kernel_failure, locate};

fn error(code: &str, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(Severity::Error, code, message).expect("a literal code is well-formed")
}

/// Re-projects every sourced entity of `feature`'s sketch onto `plane`,
/// one recorded `project` call each. A lost source is `ref.lost` with
/// candidates, a source the plane sees end-on or as an ellipse is the
/// kernel's code, and a projection of another shape than the entity's is
/// `sketch.source-kind`; each names the entity and its source.
pub(super) fn sources(
    kernel: &mut Kernel,
    scope: &Scope,
    feature: FeatureId,
    sketch: &mut Sketch,
    plane: &Frame,
) -> Result<(), Failure> {
    for (entity, source) in sketch.sources().clone() {
        let Ref::Topo(name) = &source else {
            unreachable!("a sketch holds only edge and vertex sources");
        };
        let named = Ref::Sketch { feature, entity };
        let fail = |mut f: Failure| {
            f.diagnostic.message = format!("entity {entity}'s source: {}", f.diagnostic.message);
            f.diagnostic = f.diagnostic.with_refs([named.clone()]);
            f
        };
        let body = locate(kernel, scope, &source, name).map_err(fail)?;
        let projected = kernel
            .project(body, std::slice::from_ref(name), plane)
            .map_err(|e| fail(kernel_failure(&e, &source)))?;
        place(sketch, entity, projected[0]).map_err(|wanted| {
            fail(
                error(
                    "sketch.source-kind",
                    format!(
                        "{name} projects to {}, where {wanted} is drawn",
                        shape(&projected[0])
                    ),
                )
                .with_refs([source.clone()])
                .into(),
            )
        })?;
    }
    Ok(())
}

fn shape(p: &Projected) -> &'static str {
    match p {
        Projected::Point(_) => "a point",
        Projected::Line { .. } => "a line",
        Projected::Arc { .. } => "an arc",
        Projected::Circle { .. } => "a circle",
    }
}

/// Moves `entity`'s points (and a circle's radius) onto `projected`, or
/// names the entity's own shape when the two differ. A line keeps
/// whichever of its ends is nearer each projected end, so an edge that
/// runs the other way does not swap the points other entities share.
fn place(sketch: &mut Sketch, entity: EntityId, projected: Projected) -> Result<(), &'static str> {
    let drawn = sketch
        .entity(entity)
        .cloned()
        .expect("a source is swept with its entity");
    let at = |p: PointId| {
        let q = sketch
            .point(p)
            .expect("an entity's points are in its sketch");
        DVec2::new(q.x, q.y)
    };
    let (moves, radius): (Vec<(PointId, DVec2)>, _) = match (&drawn, projected) {
        (&Entity::Point(p), Projected::Point(to)) => (vec![(p, to)], None),
        (&Entity::Line { start, end }, Projected::Line { start: a, end: b }) => {
            let (s, e) = (at(start), at(end));
            let kept = s.distance(a) + e.distance(b) <= s.distance(b) + e.distance(a);
            let (a, b) = if kept { (a, b) } else { (b, a) };
            (vec![(start, a), (end, b)], None)
        }
        (
            &Entity::Arc { center, start, end },
            Projected::Arc {
                center: c,
                start: a,
                end: b,
            },
        ) => (vec![(center, c), (start, a), (end, b)], None),
        (&Entity::Circle { center, .. }, Projected::Circle { center: c, radius }) => {
            (vec![(center, c)], Some(radius))
        }
        (drawn, _) => {
            return Err(match drawn {
                Entity::Point(_) => "a point",
                Entity::Line { .. } => "a line",
                Entity::Arc { .. } => "an arc",
                _ => "a circle",
            });
        }
    };
    for (p, to) in moves {
        sketch
            .point_mut(p)
            .expect("an entity's points are in its sketch")
            .set_pos(to.x, to.y);
    }
    if let (Some(Entity::Circle { radius: r, .. }), Some(radius)) =
        (sketch.entity_mut(entity), radius)
    {
        *r = radius;
    }
    Ok(())
}
