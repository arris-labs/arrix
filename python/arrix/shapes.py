"""Profiles without the ceremony: a planar region in the (u, v) of a plane,
every curve keyed (ids.key). Side faces are rooted at a curve's key, so a
key must be the same for the same logical curve on every evaluation."""

from __future__ import annotations

from typing import Sequence

from . import _wire as w
from .ids import key


def polygon_loop(points: Sequence[tuple[float, float]], first_key: int) -> w.ProfileLoop:
    """A closed polygon through `points`, its edges keyed `first_key`, +1, …
    (the edge ending at each point after the first, then the closing one)."""
    start, rest = points[0], list(points[1:]) + [points[0]]
    segments = [
        w.ProfileSegment.line(w.LineSegment(key(first_key + i), w.Vec2(*p)))
        for i, p in enumerate(rest)
    ]
    return w.ProfileLoop.path(w.PathLoop(w.Vec2(*start), segments))


def circle_loop(center: tuple[float, float], radius: float, key_: int) -> w.ProfileLoop:
    return w.ProfileLoop.circle(w.CircleLoop(key(key_), w.Vec2(*center), radius))


def profile(
    plane: w.Frame, outer: w.ProfileLoop, holes: Sequence[w.ProfileLoop] = ()
) -> w.Profile:
    return w.Profile(plane, outer, list(holes))
