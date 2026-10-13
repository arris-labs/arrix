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


@arrix.exporter(
    "sample.dump",
    "Dump",
    "txt",
    params=[arrix.param("scale", "Scale", kind=arrix.QuantityKind.RATIO, default="1")],
    choices=[arrix.choice("style", "Style", {"short": "Short", "long": "Long"}, "short")],
)
def dump(snapshot, args):
    names = [p.name for p in snapshot.parts()]
    text = f"{args.subject_kind} {args.subject} x{args.params['scale']} {args.choices['style']} {names}"
    return {"dump.txt": text.encode(), "meshes/a.bin": b"\x01\x02"}


@arrix.exporter("sample.mass", "Mass", "txt", subject=arrix.SubjectKind.PART)
def mass(snapshot, args):
    return [("mass.txt", str(snapshot.mass(args.subject).mass).encode())]
