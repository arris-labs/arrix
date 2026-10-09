"""A Tier 2 fixture plugin on the Python SDK (plans/boundary-m1 step 7;
step 5 wrote it on the bare wire): `block.block`, a square of side `size`
on its plane, extruded `size / 2`, with the first edge `kernel.names` lists
filleted at `size / 10`. Every body is the host's, built by callback.

`tests/tier2.rs` holds it equal to a Tier 0 double making the same calls.
"""

import arrix
from arrix import shapes


@arrix.feature_type(
    "block.block",
    "Block",
    params=[arrix.param("size", "Size", default="10 mm")],
    inputs=[arrix.input("plane", "Plane", arrix.InputKind.PLANE)],
    outputs=[arrix.slot("body")],
)
def block(kernel, args):
    size = args.params["size"]
    h = size / 2
    outer = shapes.polygon_loop([(-h, -h), (h, -h), (h, h), (-h, h)], first_key=1)
    body = kernel.extrude(shapes.profile(args.input("plane"), outer), size / 2)
    edge = next(n for n in kernel.names(body) if n.encoded.startswith("edge:"))
    return {"body": kernel.fillet(body, [edge], size / 10)}
