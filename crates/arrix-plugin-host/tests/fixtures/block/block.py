"""A Tier 2 fixture plugin written on the bare wire, without the SDK
(plans/boundary-m1 step 5): `block.block`, a square of side `size` on its
plane, extruded `size / 2`, with the first edge `kernel.names` lists
filleted at `size / 10`. Every body is the host's, built by callback.

`tests/tier2.rs` holds it equal to a Tier 0 double making the same calls.
"""

import json
import sys

API = "0.5.0"
PROTOCOL = 1

SPEC = {
    "id": "block.block",
    "version": 1,
    "title": "Block",
    "params": [
        {
            "name": "size",
            "title": "Size",
            "kind": "length",
            "default": "10 mm",
            "shown-when": None,
        }
    ],
    "choices": [],
    "inputs": [
        {
            "name": "plane",
            "title": "Plane",
            "kind": "plane",
            "list": False,
            "shown-when": None,
        }
    ],
    "outputs": [{"name": "body", "kind": "body", "modifies": None}],
}

next_id = 0


def send(message):
    message["jsonrpc"] = "2.0"
    sys.stdout.write(json.dumps(message) + "\n")
    sys.stdout.flush()


def kernel(method, **params):
    """One `kernel.*` call: the request, then the host's answer."""
    global next_id
    next_id += 1
    send({"id": next_id, "method": method, "params": params})
    answer = json.loads(sys.stdin.readline())
    assert answer["id"] == next_id, answer
    return answer["result"]


def key(n):
    """An id's 13-character text (ADR-0022 §3), for n < 32."""
    return "000000000000" + "0123456789ABCDEFGHJKMNPQRSTVWXYZ"[n]


def evaluate(params):
    size = next(p["value"]["si"] for p in params["params"] if p["name"] == "size")
    plane = next(i["value"]["plane"] for i in params["inputs"] if i["name"] == "plane")
    h = size / 2
    corners = [(h, -h), (h, h), (-h, h), (-h, -h)]
    profile = {
        "plane": plane,
        "outer": {
            "path": {
                "start": {"x": -h, "y": -h},
                "segments": [
                    {"line": {"key": key(i + 1), "to": {"x": x, "y": y}}}
                    for i, (x, y) in enumerate(corners)
                ],
            }
        },
        "holes": [],
    }
    made = kernel("kernel.extrude", p=profile, distance=size / 2)
    if "err" in made:
        return made
    body = made["ok"]
    names = kernel("kernel.names", b=body)
    if "err" in names:
        return names
    edge = next(n for n in names["ok"] if n["encoded"].startswith("edge:"))
    made = kernel("kernel.fillet", b=body, edges=[edge], radius=size / 10)
    if "err" in made:
        return made
    slot = {"name": "body", "value": {"body": made["ok"]}}
    return {"ok": {"slots": [slot], "tool": None}}


def answer(request):
    method = request["method"]
    if method == "initialize":
        return {"protocol": PROTOCOL, "api": API}
    if method == "feature.describe":
        return [SPEC]
    if method == "feature.evaluate":
        return evaluate(request["params"])
    raise KeyError(method)


for line in sys.stdin:
    request = json.loads(line)
    send({"id": request["id"], "result": answer(request)})
