"""The plugin the SDK's tests run: one feature type per behaviour."""

import arrix
from arrix import shapes


@arrix.feature_type(
    "sample.slab",
    "Slab",
    params=[arrix.param("size", "Size", default="10 mm")],
    choices=[arrix.choice("edges", "Edges", {"sharp": "Sharp", "round": "Round"}, "sharp")],
    inputs=[arrix.input("plane", "Plane", arrix.InputKind.PLANE)],
    outputs=[arrix.slot("body")],
)
def slab(kernel, args):
    print("a stray print must not reach the wire")
    h = args.params["size"] / 2
    outer = shapes.polygon_loop([(-h, -h), (h, -h), (h, h), (-h, h)], first_key=1)
    body = kernel.extrude(shapes.profile(args.input("plane"), outer), 2 * h)
    if args.choices["edges"] == "round":
        edge = kernel.names(body)[0]
        body = kernel.fillet(body, [edge], h / 5)
    return {"body": body}


@arrix.feature_type("sample.broken", "Broken")
def broken(kernel, args):
    raise ZeroDivisionError("by design")


@arrix.feature_type("sample.refuses", "Refuses")
def refuses(kernel, args):
    raise arrix.Failure(arrix.diagnostic("sample.refused", "no, by design"))
