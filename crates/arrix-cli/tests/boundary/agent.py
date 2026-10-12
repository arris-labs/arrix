"""The open boundary's accept, acting as the agent (plans/boundary-m3 step 8,
ADR-0016). Run by `cargo test -p arrix-cli --test boundary_m3` in a temporary
directory holding copies of `arrix`, `arrix-app` and the SDK, with nothing of
the workspace on PATH, PYTHONPATH or in the working directory. Everything
below uses only those and the standard library:

1. writes a plugin, `boss` (a round boss on a face), with a `[test]` table;
2. loads it into `arrix serve` through `arrix.session`, builds a plate and a
   boss on its top face, settles, and saves (the fixture too);
3. renders with `arrix-app render --json` and checks the JSON against
   `arrix-app render --schema`: two bodies, the boss's bounds;
4. runs `arrix test` on the plugin: every line `pass`;
5. evaluates the saved document without the plugin: the boss frozen with its
   reason, the plate whole.

Prints `SKIPPED` for the render without a wgpu adapter, unless
`ARRIX_REQUIRE_GPU=1`; ends with `ACCEPTED`.
"""

import json
import os
import subprocess
import sys
import textwrap
from pathlib import Path

import arrix
from arrix.session import Session

HERE = Path.cwd()
PLUGIN = HERE / "boss"
DOC = HERE / "plate-with-boss"
PART = "0000000000P01"
SKETCH = "0000000000S01"
PLATE = "0000000000E01"
BOSS = "0000000000B01"
MM = 1e-3
# The plate is 40 x 30 x 5 mm from the origin; the boss, 8 mm across and
# 6 mm tall, stands at (20, 15) on its top face.
PLATE_TOP = 5 * MM
BOSS_TOP = 11 * MM
BOSS_XY = (20 * MM, 15 * MM)
BOSS_R = 4 * MM


def write_plugin():
    PLUGIN.mkdir()
    (PLUGIN / "arrix-plugin.toml").write_text(
        textwrap.dedent(
            """\
            [plugin]
            id = "boss"
            version = "0.1.0"
            api = "^0.6"
            tier = 2
            title = "Boss"
            licence = "MIT OR Apache-2.0"

            [tier2]
            command = ["python3", "-m", "arrix.plugin", "boss"]

            [capabilities]
            kernel = true

            [contributes]
            features = ["boss.boss"]

            [test]
            documents = ["fixtures/plate-with-boss"]
            """
        )
    )
    (PLUGIN / "boss.py").write_text(
        textwrap.dedent(
            '''\
            """A round boss standing on a plane: one circle, extruded."""

            import arrix
            from arrix import shapes

            WALL = 1


            @arrix.feature_type(
                "boss.boss",
                "Boss",
                params=[
                    arrix.param("diameter", "Diameter", default="8 mm"),
                    arrix.param("height", "Height", default="6 mm"),
                    arrix.param("x", "X on the plane", default="0 mm"),
                    arrix.param("y", "Y on the plane", default="0 mm"),
                ],
                inputs=[arrix.input("plane", "Plane", arrix.InputKind.PLANE)],
                outputs=[arrix.slot("body")],
            )
            def boss(kernel, args):
                p = args.params
                circle = shapes.circle_loop((p["x"], p["y"]), p["diameter"] / 2, WALL)
                body = kernel.extrude(shapes.profile(args.input("plane"), circle), p["height"])
                return {"body": body}
            '''
        )
    )


def plate_commands():
    """The part, a 40 x 30 mm rectangle sketched on XY, and its extrusion."""
    corners = {"0000000000T01": (0, 0), "0000000000T02": (40, 0),
               "0000000000T03": (40, 30), "0000000000T04": (0, 30)}
    points = {k: {"fixed": True, "x": x * MM, "y": y * MM} for k, (x, y) in corners.items()}
    ids = list(corners)
    lines = {f"0000000000K0{i + 1}": {"line": {"start": ids[i], "end": ids[(i + 1) % 4]}}
             for i in range(4)}
    sketch = {
        "id": SKETCH, "name": "Plate sketch", "inputs": {}, "params": {},
        "sketch": {"constraints": {}, "entities": lines, "points": points},
        "suppressed": False, "type_id": "core.sketch", "type_version": 1,
    }
    region = {"feature": SKETCH, "key": {"entities": sorted(lines), "sample": [20_000_000, 15_000_000]}}
    plate = {
        "id": PLATE, "name": "Plate", "choices": {"mode": "new"},
        "inputs": {"region": {"region": region}}, "params": {"distance": "5 mm"},
        "suppressed": False, "type_id": "core.extrude", "type_version": 1,
    }
    boss = {
        "id": BOSS, "name": "Boss", "inputs": {"plane": {"topo": f"face:sweep.{PLATE}.end-cap"}},
        "params": {"x": "20 mm", "y": "15 mm"},
        "suppressed": False, "type_id": "boss.boss", "type_version": 1,
    }
    part = {"id": PART, "name": "Plate with a boss", "rollback": None, "history": [], "features": {}}
    commands = [{"add_part": {"part": part}}]
    for at, record in enumerate([sketch, plate, boss]):
        commands.append({"add_feature": {"part": PART, "at": at, "record": record}})
    return commands


def build_and_save():
    with Session.spawn(arrix="arrix", timeout=120.0) as s:
        hello = s.initialize()
        assert hello["protocol"] == 1, hello
        generation = s.load_plugin(PLUGIN)["loaded"]["generation"]
        registry = s.wait_event(kind="registry")
        assert [t["id"] for t in registry["added"]] == ["boss.boss"], registry
        for command in plate_commands():
            generation = s.submit(command, base=generation)["applied"]["generation"]
        s.settle(generation)
        for feature in (SKETCH, PLATE, BOSS):
            outcome = s.outcome(generation, feature)
            assert outcome["status"] == "ok", (feature, outcome)
        s.save(DOC)
        s.save(PLUGIN / "fixtures/plate-with-boss")
    assert (DOC / "document.json").is_file()


def check(schema, value, at="$"):
    """A minimal JSON Schema checker: the keywords `render --schema` uses."""
    if "oneOf" in schema:
        matched = 0
        for option in schema["oneOf"]:
            try:
                check(option, value, at)
                matched += 1
            except AssertionError:
                pass
        assert matched == 1, f"{at}: {matched} of oneOf match"
    kinds = {"object": dict, "array": list, "string": str, "boolean": bool, "null": type(None)}
    t = schema.get("type")
    if t == "number":
        assert isinstance(value, (int, float)) and not isinstance(value, bool), f"{at}: not a number"
    elif t == "integer":
        assert isinstance(value, int) and not isinstance(value, bool), f"{at}: not an integer"
    elif t is not None:
        assert isinstance(value, kinds[t]), f"{at}: not {t}"
    if "const" in schema:
        assert value == schema["const"], f"{at}: not {schema['const']!r}"
    if "minimum" in schema:
        assert value >= schema["minimum"], f"{at}: under {schema['minimum']}"
    if isinstance(value, dict):
        for name in schema.get("required", []):
            assert name in value, f"{at}: no {name}"
        props = schema.get("properties", {})
        for name, v in value.items():
            if name in props:
                check(props[name], v, f"{at}.{name}")
            else:
                assert schema.get("additionalProperties", True) is not False, f"{at}: extra {name}"
    if isinstance(value, list):
        assert len(value) >= schema.get("minItems", 0), f"{at}: too few items"
        assert len(value) <= schema.get("maxItems", len(value)), f"{at}: too many items"
        if "items" in schema:
            for i, v in enumerate(value):
                check(schema["items"], v, f"{at}[{i}]")


def promised(schema):
    """The `debug_state` fields the schema marks `x-arrix-promised`."""
    fields = schema["properties"]["debug_state"]["properties"]
    return {name for name, sub in fields.items() if sub.get("x-arrix-promised")}


def render():
    json_file = HERE / "render.json"
    done = subprocess.run(
        ["arrix-app", "render", str(DOC), "--plugin", str(PLUGIN), "--size", "480x300",
         "-o", str(HERE / "render.png"), "--json", str(json_file)],
        capture_output=True, text=True, timeout=600,
    )
    if done.returncode != 0:
        if "no wgpu" in done.stderr and os.environ.get("ARRIX_REQUIRE_GPU") != "1":
            print(f"SKIPPED the render: {done.stderr.strip()}")
            return
        raise AssertionError(f"arrix-app render failed: {done.stderr}")
    schema = json.loads(subprocess.run(
        ["arrix-app", "render", "--schema"], capture_output=True, text=True, check=True
    ).stdout)
    drawn = json.loads(json_file.read_text())
    check(schema, drawn)
    # `badges` is absent while nothing is frozen; the rest are drawn here.
    marked = promised(schema)
    assert {"bodies", "badges", "camera"} <= marked, marked
    state = drawn["debug_state"]
    assert marked - {"badges"} <= set(state), (marked, sorted(state))
    assert "badges" not in state, state["badges"]
    bodies = {b["feature"]: b for b in drawn["debug_state"]["bodies"]}
    assert sorted(bodies) == sorted([PLATE, BOSS]), bodies
    lo_x, lo_y, lo_z, hi_x, hi_y, hi_z = bodies[BOSS]["bounds"]
    near = lambda a, b, tol=1e-6: abs(a - b) < tol  # noqa: E731
    assert near(lo_z, PLATE_TOP) and near(hi_z, BOSS_TOP), bodies[BOSS]
    # The bounds are the drawn mesh's: across, they may fall short by a chord.
    chord = 1e-5
    assert near(lo_x, BOSS_XY[0] - BOSS_R, chord) and near(hi_x, BOSS_XY[0] + BOSS_R, chord), bodies[BOSS]
    assert near(lo_y, BOSS_XY[1] - BOSS_R, chord) and near(hi_y, BOSS_XY[1] + BOSS_R, chord), bodies[BOSS]
    assert near(bodies[PLATE]["bounds"][5], PLATE_TOP), bodies[PLATE]
    print("rendered: two bodies, the boss's bounds")


def test_plugin():
    done = subprocess.run(["arrix", "test", str(PLUGIN)], capture_output=True, text=True, timeout=600)
    report = [json.loads(line) for line in done.stdout.splitlines()]
    assert done.returncode == 0, (done.returncode, done.stderr, report)
    checks = {line["check"] for line in report}
    assert {"spec", "determinism", "round_trip", "frozen", "migration"} <= checks, checks
    for line in report:
        assert line["status"] == "pass", line
    print(f"arrix test: {len(report)} checks pass")


def frozen_without_it():
    [line] = arrix.eval(DOC, arrix="arrix", plugins=[PLUGIN], without_plugins=["boss"])
    features = {f["id"]: f for f in line["features"]}
    boss = features[BOSS]
    assert boss["status"] == "frozen", boss
    assert boss["category"] == "plugin.frozen.missing" and boss["diagnostic"]["message"], boss
    assert features[PLATE]["status"] == "ok", features[PLATE]
    plate = [b for b in line["bodies"] if b["feature"] == PLATE]
    assert len(plate) == 1 and abs(plate[0]["volume"] - 40 * 30 * 5 * MM**3) < 1e-12, line["bodies"]
    print("without boss: frozen, the plate whole")


def main():
    write_plugin()
    build_and_save()
    render()
    test_plugin()
    frozen_without_it()
    print("ACCEPTED")


if __name__ == "__main__":
    sys.exit(main())
