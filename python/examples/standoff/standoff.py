"""A hexagonal standoff, the sample Tier 2 plugin (plans/boundary-m1 step 8):
a hexagonal prism on a plane input, a hole through it, and its bottom ring
of edges rounded, built by `extrude`, `cut` and `fillet` on the host's kernel.

Run by the manifest's `python3 -m arrix.plugin standoff` in this directory,
with the SDK (`python/`) on PYTHONPATH:

    PYTHONPATH=python cargo run -p arrix-cli -- eval --plugin python/examples/standoff <doc>
"""

import math
from dataclasses import replace

import arrix
from arrix import _wire as w
from arrix import shapes

# The curve keys are the hexagon's sides 1 to 6 and the hole's wall, 7. A
# key must be the same for the same curve on every evaluation: a document
# names the standoff's faces and edges by them.
SIDES = 1
HOLE = 7
# How far the hole's tool overshoots the prism at each end, in metres.
OVERSHOOT = 1e-3


@arrix.feature_type(
    "standoff.standoff",
    "Standoff",
    params=[
        arrix.param("across", "Across flats", default="8 mm"),
        arrix.param("height", "Height", default="10 mm"),
        arrix.param("hole", "Hole diameter", default="3 mm"),
        arrix.param("round", "Edge radius", default="0.5 mm"),
        arrix.param("x", "X on the plane", default="0 mm"),
        arrix.param("y", "Y on the plane", default="0 mm"),
    ],
    inputs=[arrix.input("plane", "Plane", arrix.InputKind.PLANE)],
    outputs=[arrix.slot("body")],
)
def standoff(kernel, args):
    p = args.params
    plane = args.input("plane")
    x, y = p["x"], p["y"]
    flat = p["across"] / 2
    corner = p["across"] / math.sqrt(3)
    hexagon = [
        (x + corner, y),
        (x + corner / 2, y + flat),
        (x - corner / 2, y + flat),
        (x - corner, y),
        (x - corner / 2, y - flat),
        (x + corner / 2, y - flat),
    ]
    prism = kernel.extrude(
        shapes.profile(plane, shapes.polygon_loop(hexagon, SIDES)), p["height"]
    )
    # The hole's tool starts below the plane and ends above the top.
    z = plane.z_axis
    below = w.Vec3(*(getattr(plane.origin, a) - OVERSHOOT * getattr(z, a) for a in "xyz"))
    tool = kernel.extrude(
        shapes.profile(
            replace(plane, origin=below),
            shapes.circle_loop((x, y), p["hole"] / 2, HOLE),
        ),
        p["height"] + 2 * OVERSHOOT,
    )
    body = kernel.cut(prism, tool)

    # The bottom ring: the hexagon's six start edges, which the sweep names by
    # the keys above (`edge:sweep.<feature>.start-edge.<key>`).
    ring = [n for n in kernel.names(body) if ".start-edge." in n.encoded]
    return {"body": kernel.fillet(body, ring, p["round"])}
