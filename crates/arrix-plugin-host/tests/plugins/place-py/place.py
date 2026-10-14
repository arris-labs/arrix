"""The test commands `place.*` in Python (plans/c3-m3 step 8): the same as
`tests/plugins/place/src/place.rs`, which the Tier 0 and Tier 1 tests run.
In an assembly, a block part's component on each picked edge; the part is
found by a feature of its own in the snapshot and made, with a material,
only when no part has one."""

import arrix
from arrix import _wire as w

MARKER = "block marker"


def _spec(name):
    return dict(
        group="Place",
        params=[arrix.param("size", "Size", default="5 mm")],
        inputs=[arrix.input("edges", "Edges", arrix.InputKind.EDGE, list=True)],
        max_new=8,
    )


def _si(kind, value):
    return w.Expression.si(w.Quantity(kind, value))


def _expand(snapshot, args, name):
    if name == "refuse":
        raise arrix.Failure(arrix.diagnostic("place.refused", "this command refuses"))
    if name == "panic":
        raise RuntimeError("place.panic was asked to")
    if args.context.case != "assembly":
        raise arrix.Failure(arrix.diagnostic("place.context", "an assembly is needed"))
    assembly = args.context.value
    size = args.params.get("size", 0.005)
    edges = args.input("edges")
    commands = []

    part = next(
        (p.id for p in snapshot.parts() if any(f.name == MARKER for f in p.features)),
        None,
    )
    if part is None:
        part = arrix.key(0) if not edges else args.new_id()
        if edges:
            feature, material = args.new_id(), args.new_id()
            marker = w.FeatureDraft(
                feature,
                "core.datum-plane",
                MARKER,
                [],
                [w.ChoiceValue("world", "xy")],
                [],
            )
            commands.append(w.PluginCommand.add_material(w.MaterialDraft(
                material,
                "Placed steel",
                w.Expression.text("7850 kg/m3"),
                w.Rgb(0.5, 0.5, 0.55),
            )))
            commands.append(w.PluginCommand.add_part(w.PartDraft(part, "Block", [marker], None)))
            commands.append(w.PluginCommand.assign_material(w.AssignMaterial(part, None, material)))

    for n, edge in enumerate(edges, start=1):
        curve = edge.curve
        if curve.case == "circle":
            at = curve.value.centre
            at = (at.x, at.y, at.z)
        elif curve.case == "line":
            a, b = curve.value.start, curve.value.end
            at = ((a.x + b.x) * 0.5, (a.y + b.y) * 0.5, (a.z + b.z) * 0.5)
        else:
            at = (0.0, 0.0, 0.0)
        component = args.new_id() if name != "rogue" else arrix.key(1)
        length = w.QuantityKind.LENGTH
        zero = _si(w.QuantityKind.ANGLE, 0.0)
        commands.append(w.PluginCommand.add_component(w.ComponentChange(
            assembly,
            w.ComponentDraft(
                component,
                f"block {n}",
                w.ComponentOf.part(part),
                w.Pose(
                    (_si(length, at[0]), _si(length, at[1]), _si(length, at[2] + size)),
                    (zero, zero, zero),
                ),
            ),
        )))
    return "Place blocks", commands


for _name in ["block", "refuse", "rogue", "panic"]:
    arrix.command(f"place.{_name}", _name, **_spec(_name))(
        lambda snapshot, args, _name=_name: _expand(snapshot, args, _name)
    )
