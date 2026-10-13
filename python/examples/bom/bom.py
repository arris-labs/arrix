"""A bill of materials, the sample Tier 2 exporter (plans/c3-m2 step 7):
`bom.csv` writes one row per part an assembly uses, at every depth, with
how many components are of it, its mass and its material. It reads the
evaluated document through the snapshot only, so it is a pure function of
the document and its options.

Run by the manifest's `python3 -m arrix.plugin bom` in this directory, with
the SDK (`python/`) on PYTHONPATH:

    PYTHONPATH=python cargo run -p arrix-cli -- export <doc> \\
        --plugin python/examples/bom --exporter bom.csv \\
        --assembly "Servo on bracket" -o bom.csv
"""

import arrix
from arrix import _wire as w


def _cell(text: str) -> str:
    """A CSV cell: quoted when it holds a comma, a quote or a line break."""
    if any(c in text for c in ',"\n\r'):
        return '"' + text.replace('"', '""') + '"'
    return text


@arrix.exporter(
    "bom.csv",
    "Bill of materials (CSV)",
    "csv",
    subject=arrix.SubjectKind.ASSEMBLY,
    choices=[
        arrix.choice(
            "mass-unit",
            "Mass unit",
            {"kg": "Kilograms", "g": "Grams"},
            default="kg",
        )
    ],
)
def bom(snapshot, args):
    assembly = next(a for a in snapshot.assemblies() if a.id == args.subject)
    parts = {p.id: p for p in snapshot.parts()}
    scale, unit = (1000.0, "g") if args.choices["mass-unit"] == "g" else (1.0, "kg")
    counts: dict[str, int] = {}
    for component in assembly.components:
        # A nested assembly is listed by its own components, not itself.
        if component.of.case == "part":
            counts[component.of.value] = counts.get(component.of.value, 0) + 1
    lines = [f"part,count,mass ({unit}),material"]
    total = 0.0
    for part_id in sorted(counts, key=lambda i: (parts[i].name, i)):
        part = parts[part_id]
        try:
            mass = snapshot.mass(part_id).mass * scale
        except arrix.DocumentError:
            mass = None  # no density: the row says so with an empty cell
        materials = sorted({b.material.name for b in part.bodies if b.material})
        total += (mass or 0.0) * counts[part_id]
        cells = [
            part.name,
            str(counts[part_id]),
            "" if mass is None else f"{mass:.6g}",
            "; ".join(materials),
        ]
        lines.append(",".join(_cell(c) for c in cells))
    lines.append(",".join(["total", str(sum(counts.values())), f"{total:.6g}", ""]))
    return [w.File("bom.csv", ("\n".join(lines) + "\n").encode("utf-8"))]
