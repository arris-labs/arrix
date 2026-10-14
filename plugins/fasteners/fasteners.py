"""Fasteners, first-party plugin #2 (plans/c3-m3 step 9), Tier 2 Python: an
ISO 4762 socket head cap screw and the command that puts one into each
picked hole edge of an assembly.

`fasteners.socket-head` builds the screw from one table, no thread: the
head up from the part's XY plane, the shank down from it, a hexagonal
socket cut into the head. `fasteners.place` expands to one part per size
and length (found again through the snapshot, ADR-0038 §4), a component
per screw, and a fixed joint from the picked edge to the head's underside.

Run by the manifest's `python3 -m arrix.plugin fasteners` in this
directory, with the SDK (`python/`) on PYTHONPATH.
"""

import math
from dataclasses import dataclass

import arrix
from arrix import _wire as w
from arrix import shapes

MM = 1e-3

# The curve keys of the screw's profiles; a key must be the same on every
# evaluation, since a document names the screw's edges by it.
HEAD = 1
SHANK = 2
SOCKET = 3
# How far the socket's tool overshoots the head's top, in metres.
OVERSHOOT = 1e-3

SOCKET_HEAD = "fasteners.socket-head"
DENSITY = "7850 kg/m3"
# The hole a screw is drawn to fit is the ISO 273 medium series.
FIT_TOLERANCE = 1e-9


@dataclass(frozen=True)
class Size:
    """One row of ISO 4762, in millimetres: the thread's diameter, the
    head's diameter and height, the socket's width across flats and depth,
    and the medium clearance hole (ISO 273)."""

    name: str
    thread: float
    head: float
    height: float
    socket: float
    depth: float
    clearance: float


TABLE = [
    Size("M2", 2.0, 3.8, 2.0, 1.5, 1.0, 2.4),
    Size("M2.5", 2.5, 4.5, 2.5, 2.0, 1.1, 2.9),
    Size("M3", 3.0, 5.5, 3.0, 2.5, 1.3, 3.4),
    Size("M4", 4.0, 7.0, 4.0, 3.0, 2.0, 4.5),
    Size("M5", 5.0, 8.5, 5.0, 4.0, 2.5, 5.5),
    Size("M6", 6.0, 10.0, 6.0, 5.0, 3.0, 6.6),
    Size("M8", 8.0, 13.0, 8.0, 6.0, 4.0, 9.0),
    Size("M10", 10.0, 16.0, 10.0, 8.0, 5.0, 11.0),
    Size("M12", 12.0, 18.0, 12.0, 10.0, 6.0, 13.5),
]
BY_NAME = {s.name: s for s in TABLE}
AUTO = "auto"


def size_for(diameter_mm: float) -> Size | None:
    """The largest size whose clearance hole is no wider than a hole of
    `diameter_mm`, or none when even the smallest does not fit."""
    fits = [s for s in TABLE if s.clearance <= diameter_mm + FIT_TOLERANCE]
    return fits[-1] if fits else None


def label(size: Size, length: float) -> str:
    """`M3x10`: the size and the length in millimetres."""
    return f"{size.name}x{length / MM:g}"


def _sizes(auto: bool) -> dict[str, str]:
    options = {AUTO: "From the hole"} if auto else {}
    options.update({s.name: f"{s.name} socket head" for s in TABLE})
    return options


PLANE = w.Frame(w.Vec3(0.0, 0.0, 0.0), w.Vec3(1.0, 0.0, 0.0), w.Vec3(0.0, 0.0, 1.0))


def _at(z: float) -> w.Frame:
    return w.Frame(w.Vec3(0.0, 0.0, z), PLANE.x_axis, PLANE.z_axis)


@arrix.feature_type(
    SOCKET_HEAD,
    "Socket head cap screw",
    params=[arrix.param("length", "Length", default="10 mm")],
    choices=[arrix.choice("size", "Size", _sizes(False), default="M3")],
    outputs=[arrix.slot("body")],
)
def socket_head(kernel, args):
    size = BY_NAME[args.choices["size"]]
    length = args.params["length"]
    if length < 0:
        raise arrix.Failure(arrix.diagnostic("fasteners.length", "a screw's length is not below zero"))
    height = size.height * MM
    head = kernel.extrude(
        shapes.profile(PLANE, shapes.circle_loop((0.0, 0.0), size.head * MM / 2, HEAD)),
        height,
    )
    shank = kernel.extrude(
        shapes.profile(PLANE, shapes.circle_loop((0.0, 0.0), size.thread * MM / 2, SHANK)),
        -length,
    )
    body = kernel.fuse(head, shank)

    # The socket: a hexagon across flats, from inside the head to above it.
    corner = size.socket * MM / math.sqrt(3)
    flat = size.socket * MM / 2
    hexagon = [
        (corner, 0.0),
        (corner / 2, flat),
        (-corner / 2, flat),
        (-corner, 0.0),
        (-corner / 2, -flat),
        (corner / 2, -flat),
    ]
    floor = height - size.depth * MM
    tool = kernel.extrude(
        shapes.profile(_at(floor), shapes.polygon_loop(hexagon, SOCKET)),
        size.depth * MM + OVERSHOOT,
    )
    return {"body": kernel.cut(body, tool)}


def underside(feature: str) -> w.PersistentRef:
    """The head's underside edge, the screw's seat: the start edge of the
    head's extrusion (`edge:sweep.<feature>.start-edge.<key>`)."""
    return w.PersistentRef(f"edge:sweep.{feature}.start-edge.{arrix.key(HEAD)}")


def _dot(a: w.Vec3, b: w.Vec3) -> float:
    return a.x * b.x + a.y * b.y + a.z * b.z


def _fail(code: str, message: str) -> arrix.Failure:
    return arrix.Failure(arrix.diagnostic(code, message))


def _length(value: float) -> w.Expression:
    return w.Expression.si(w.Quantity(w.QuantityKind.LENGTH, value))


def _zero_angle() -> w.Expression:
    return w.Expression.si(w.Quantity(w.QuantityKind.ANGLE, 0.0))


def _find_part(snapshot, size: Size, length: float) -> str | None:
    """The part an earlier gesture made for this size and length: the one
    whose history holds a socket-head feature of both."""
    for part in snapshot.parts():
        for f in part.features:
            if f.type_id != SOCKET_HEAD:
                continue
            chosen = {c.name: c.value for c in f.choices}.get("size")
            have = {p.name: p.value.si for p in f.params}.get("length")
            if chosen == size.name and have is not None and abs(have - length) < 1e-9:
                return part.id
    return None


def _flip(edge: w.ResolvedEdge) -> bool:
    """Whether the joint flips so that the screw's axis, head to tip, runs
    against the outward normal of the face the edge bounds: the head on the
    surface, the shank into the material. The side is read from the face,
    never from the hole circle's axis alone (ADR-0038 §3). The head's
    underside circle is oriented along the screw's +z as Arris builds it
    (plans/c3-m3 step 9, checked by the accept's pose), so an unflipped
    joint puts the screw's +z along the hole circle's axis, and the joint
    flips when that axis opposes the face's normal."""
    if not edge.face_normals:
        raise _fail(
            "fasteners.side",
            f"{edge.name.encoded} bounds no planar face, so the screw's side is unknown",
        )
    return _dot(edge.curve.value.axis, edge.face_normals[0]) < 0


@arrix.command(
    "fasteners.place",
    "Place screws",
    group="Fasteners",
    params=[arrix.param("length", "Length", default="10 mm")],
    choices=[arrix.choice("size", "Size", _sizes(True), default=AUTO)],
    inputs=[arrix.input("holes", "Hole edges", arrix.InputKind.EDGE, list=True)],
    max_new=64,
)
def place(snapshot, args):
    if args.context.case != "assembly":
        raise _fail("fasteners.context", "screws are placed in an assembly")
    assembly = args.context.value
    length = args.params["length"]
    if length <= 0:
        raise _fail("fasteners.length", "a screw's length must be above zero")
    holes = args.input("holes")
    if not holes:
        raise _fail("fasteners.holes", "pick at least one hole edge")
    chosen = args.choices.get("size", AUTO)

    # A size for every pick first, so a hole no size fits is refused by name
    # before anything is written.
    picks = []
    for edge in holes:
        if edge.curve.case != "circle":
            raise _fail("fasteners.hole", f"{edge.name.encoded} is not a circular hole edge")
        if chosen != AUTO:
            size = BY_NAME[chosen]
        else:
            size = size_for(2 * edge.curve.value.radius / MM)
            if size is None:
                raise _fail(
                    "fasteners.size",
                    f"no ISO 4762 size fits {edge.name.encoded}: its diameter is "
                    f"{2 * edge.curve.value.radius / MM:.3g} mm, the smallest clearance hole "
                    f"{TABLE[0].clearance} mm",
                )
        picks.append((edge, size))

    commands = []
    parts: dict[str, tuple[str, str]] = {}  # label -> (part id, feature id)
    material = None
    existing = {}
    for a in snapshot.assemblies():
        if a.id == assembly:
            for c in a.components:
                if c.of.case == "part":
                    existing[c.of.value] = existing.get(c.of.value, 0) + 1
    count: dict[str, int] = {}

    for edge, size in picks:
        name = label(size, length)
        if name not in parts:
            part = _find_part(snapshot, size, length)
            if part is not None:
                feature = _feature_of(snapshot, part)
            else:
                if material is None:
                    material = args.new_id()
                    commands.append(w.PluginCommand.add_material(w.MaterialDraft(
                        material,
                        "Steel",
                        w.Expression.text(DENSITY),
                        w.Rgb(0.55, 0.57, 0.6),
                    )))
                part, feature = args.new_id(), args.new_id()
                draft = w.FeatureDraft(
                    feature,
                    SOCKET_HEAD,
                    "Socket head cap screw",
                    [w.NamedExpression("length", _length(length))],
                    [w.ChoiceValue("size", size.name)],
                    [],
                )
                commands.append(w.PluginCommand.add_part(w.PartDraft(part, name, [draft], material)))
            parts[name] = (part, feature)
        part, feature = parts[name]
        count[part] = count.get(part, 0) + 1
        n = existing.get(part, 0) + count[part]
        component, joint = args.new_id(), args.new_id()
        screw = f"{name} {n}"
        commands.append(w.PluginCommand.add_component(w.ComponentChange(
            assembly, w.ComponentDraft(component, screw, w.ComponentOf.part(part), None)
        )))
        commands.append(w.PluginCommand.add_joint(w.JointChange(
            assembly,
            w.JointDraft(
                joint,
                f"{screw} seat",
                w.JointKind.FIXED,
                w.Anchor(list(edge.path), edge.name, None),
                w.Anchor([component], underside(feature), None),
                _zero_angle(),
                _zero_angle(),
                None,
                _flip(edge),
            ),
        )))
    return "Place screws", commands


def _feature_of(snapshot, part_id: str) -> str:
    for part in snapshot.parts():
        if part.id == part_id:
            return next(f.id for f in part.features if f.type_id == SOCKET_HEAD)
    raise _fail("fasteners.part", f"part {part_id} is gone from the snapshot")
