"""The SDK against a fake host: the test plays the host over real pipes,
answering the plugin's kernel calls from a script (plans/boundary-m1
step 7). Run by `cargo test -p arrix-plugin-host --test python_sdk`."""

import json
import os
import subprocess
import sys
import unittest

import arrix
from arrix import _wire as w

HERE = os.path.dirname(os.path.abspath(__file__))
PLANE = {
    "origin": {"x": 0.0, "y": 0.0, "z": 0.0},
    "x-axis": {"x": 1.0, "y": 0.0, "z": 0.0},
    "z-axis": {"x": 0.0, "y": 0.0, "z": 1.0},
}


class FakeHost:
    """Starts the plugin and speaks its wire."""

    def __init__(self, test: unittest.TestCase):
        self.p = subprocess.Popen(
            [sys.executable, "-m", "arrix.plugin", "sample_plugin"],
            cwd=HERE,
            env={**os.environ, "PYTHONPATH": os.path.dirname(HERE)},
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        )
        test.addCleanup(self.close)
        self.next = 0

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=10)
        self.p.stdout.close()

    def send(self, message):
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", **message}) + "\n")
        self.p.stdin.flush()

    def read(self):
        return json.loads(self.p.stdout.readline())

    def call(self, method, params=None, kernel=()):
        """Calls `method`; each kernel request is answered by the next of
        `kernel`, a function of its method and params. Returns (result,
        the kernel requests seen)."""
        self.next += 1
        self.send({"id": self.next, "method": method, "params": params or {}})
        seen = []
        answers = iter(kernel)
        while True:
            m = self.read()
            if "method" in m:
                seen.append((m["method"], m["params"]))
                self.send({"id": m["id"], "result": next(answers)(m["method"], m["params"])})
            else:
                assert m["id"] == self.next, m
                return m, seen


def run_params(exporter, subject, scale=1.0, style="short"):
    return {
        "exporter": exporter,
        "subject": subject,
        "params": [{"name": "scale", "value": {"kind": "ratio", "si": scale}}],
        "choices": [{"name": "style", "value": style}],
    }


def evaluate_params(type_id, size=0.01, choice="sharp"):
    return {
        "type-id": type_id,
        "params": [{"name": "size", "value": {"kind": "length", "si": size}}],
        "choices": [{"name": "edges", "value": choice}],
        "inputs": [{"name": "plane", "value": {"plane": PLANE}}],
    }


def ok(handle):
    return lambda method, params: {"ok": handle}


class Exporters(unittest.TestCase):
    def test_exporters_are_listed_as_the_wire_has_them(self):
        r, _ = FakeHost(self).call("exchange.exporters")
        specs = r["result"]
        self.assertEqual([s["id"] for s in specs], ["sample.dump", "sample.mass"])
        self.assertEqual(specs[0]["extension"], "txt")
        self.assertEqual(specs[0]["subject"], "assembly")
        self.assertEqual(specs[1]["subject"], "part")
        self.assertEqual(w.ExporterSpec.from_json(specs[0]).to_json(), specs[0])

    def test_a_run_reads_the_document_by_callback_and_returns_files(self):
        parts = lambda m, p: []  # noqa: E731
        r, seen = FakeHost(self).call(
            "exchange.run", run_params("sample.dump", {"assembly": "1A"}, 2.0, "long"), [parts]
        )
        self.assertEqual([m for m, _ in seen], ["document.parts"])
        files = [w.File.from_json(f) for f in r["result"]["ok"]]
        self.assertEqual([f.name for f in files], ["dump.txt", "meshes/a.bin"])
        self.assertEqual(files[0].bytes, b"assembly 1A x2.0 long []")
        self.assertEqual(files[1].bytes, b"\x01\x02")

    def test_a_refused_document_call_is_the_exports_diagnostic(self):
        refusal = w.Diagnostic(w.Severity.ERROR, "mass.no-density", "no density", [], [])
        deny = lambda m, p: {"err": refusal.to_json()}  # noqa: E731
        r, seen = FakeHost(self).call(
            "exchange.run", run_params("sample.mass", {"part": "2B"}), [deny]
        )
        self.assertEqual(seen, [("document.mass", {"part": "2B"})])
        self.assertEqual(r["result"], {"err": refusal.to_json()})

    def test_an_exporter_the_plugin_lacks_is_a_diagnostic(self):
        r, _ = FakeHost(self).call("exchange.run", run_params("sample.nothing", {"part": "2B"}))
        self.assertEqual(r["result"]["err"]["code"], "plugin.unknown-type")


def expand_params(command, ids, edges=()):
    return {
        "command": command,
        "context": {"assembly": "1A"},
        "params": [{"name": "scale", "value": {"kind": "ratio", "si": 2.0}}],
        "choices": [],
        "inputs": [
            {
                "name": "edges",
                "value": {
                    "edge": {
                        "path": [],
                        "name": {"name": e, "version": 1},
                        "curve": {"other": None},
                        "face-normals": [],
                    }
                },
            }
            for e in edges
        ],
        "ids": ids,
    }


class Commands(unittest.TestCase):
    def test_commands_are_listed_as_the_wire_has_them(self):
        r, _ = FakeHost(self).call("commands.commands")
        [spec] = r["result"]
        self.assertEqual(spec["id"], "sample.stamp")
        self.assertEqual(spec["group"], "Sample")
        self.assertEqual(spec["max-new"], 2)
        self.assertEqual(spec["inputs"][0]["kind"], "edge")
        self.assertEqual(w.CommandSpec.from_json(spec).to_json(), spec)

    def test_an_expansion_reads_the_snapshot_by_callback_and_takes_its_ids_in_order(self):
        parts = lambda m, p: []  # noqa: E731
        r, seen = FakeHost(self).call(
            "commands.expand", expand_params("sample.stamp", ["000000000000A", "000000000000B"]), [parts]
        )
        self.assertEqual([m for m, _ in seen], ["document.parts"])
        group = w.CommandGroup.from_json(r["result"]["ok"])
        self.assertEqual(group.label, "Stamp")
        [command] = group.commands
        self.assertEqual(command.case, "add-material")
        self.assertEqual(command.value.id, "000000000000A")
        self.assertEqual(command.value.name, "assembly 0 x2.0 []")

    def test_the_same_arguments_give_the_same_group(self):
        host = FakeHost(self)
        parts = lambda m, p: []  # noqa: E731
        args = expand_params("sample.stamp", ["000000000000A"])
        first, _ = host.call("commands.expand", args, [parts])
        second, _ = host.call("commands.expand", args, [parts])
        self.assertEqual(first["result"], second["result"])

    def test_more_ids_than_it_was_given_is_a_diagnostic(self):
        host = FakeHost(self)
        parts = lambda m, p: []  # noqa: E731
        args = expand_params("sample.stamp", ["000000000000A"])
        args["choices"] = [{"name": "again", "value": "yes"}]
        r, _ = host.call("commands.expand", args, [parts])
        self.assertEqual(r["result"]["err"]["code"], "command.ids")

    def test_a_command_the_plugin_lacks_is_a_diagnostic(self):
        r, _ = FakeHost(self).call("commands.expand", expand_params("sample.nothing", []))
        self.assertEqual(r["result"]["err"]["code"], "plugin.unknown-type")


class Sdk(unittest.TestCase):
    def test_initialize_names_the_protocol_and_the_api(self):
        r, _ = FakeHost(self).call("initialize", {"protocol": 1, "api": w.API_VERSION})
        self.assertEqual(r["result"], {"protocol": 1, "api": w.API_VERSION})

    def test_describe_lists_the_registered_types_as_the_wire_has_them(self):
        r, _ = FakeHost(self).call("feature.describe")
        specs = r["result"]
        self.assertEqual([s["id"] for s in specs], ["sample.slab", "sample.broken", "sample.refuses"])
        slab = specs[0]
        self.assertEqual(slab["params"][0], {
            "name": "size", "title": "Size", "kind": "length", "default": "10 mm", "shown-when": None})
        self.assertEqual(slab["inputs"][0]["kind"], "plane")
        self.assertEqual(slab["outputs"], [{"name": "body", "kind": "body", "modifies": None}])
        self.assertEqual(w.FeatureTypeSpec.from_json(slab).to_json(), slab)

    def test_evaluate_builds_by_kernel_callback(self):
        r, seen = FakeHost(self).call("feature.evaluate", evaluate_params("sample.slab"), [ok(7)])
        self.assertEqual([m for m, _ in seen], ["kernel.extrude"])
        extrude = seen[0][1]
        self.assertEqual(extrude["distance"], 0.01)
        self.assertEqual(extrude["p"]["plane"], PLANE)
        keys = [s["line"]["key"] for s in extrude["p"]["outer"]["path"]["segments"]]
        self.assertEqual(keys, [arrix.key(n) for n in (1, 2, 3, 4)])
        self.assertEqual(
            r["result"], {"ok": {"slots": [{"name": "body", "value": {"body": 7}}], "tool": None}})

    def test_a_choice_reaches_the_feature(self):
        names = lambda m, p: {"ok": [{"encoded": "edge:x"}]}  # noqa: E731
        r, seen = FakeHost(self).call(
            "feature.evaluate", evaluate_params("sample.slab", choice="round"),
            [ok(0), names, ok(1)])
        self.assertEqual([m for m, _ in seen], ["kernel.extrude", "kernel.names", "kernel.fillet"])
        self.assertEqual(seen[2][1]["edges"], [{"encoded": "edge:x"}])
        self.assertEqual(r["result"]["ok"]["slots"][0]["value"], {"body": 1})

    def test_a_refused_kernel_call_is_the_features_err(self):
        diagnostic = {"severity": "error", "code": "kernel.failed", "message": "no", "refs": [], "candidates": []}
        r, _ = FakeHost(self).call(
            "feature.evaluate", evaluate_params("sample.slab"), [lambda m, p: {"err": diagnostic}])
        self.assertEqual(r["result"], {"err": diagnostic})

    def test_a_failure_of_the_features_own_is_its_err(self):
        r, _ = FakeHost(self).call("feature.evaluate", evaluate_params("sample.refuses"))
        self.assertEqual(r["result"]["err"]["code"], "sample.refused")

    def test_a_bug_in_a_feature_is_a_diagnostic_and_the_process_lives(self):
        host = FakeHost(self)
        r, _ = host.call("feature.evaluate", evaluate_params("sample.broken"))
        err = r["result"]["err"]
        self.assertEqual(err["code"], "plugin.exception")
        self.assertIn("ZeroDivisionError", err["message"])
        r, _ = host.call("feature.evaluate", evaluate_params("sample.slab"), [ok(3)])
        self.assertIn("ok", r["result"])

    def test_an_unknown_type_is_a_diagnostic(self):
        r, _ = FakeHost(self).call("feature.evaluate", evaluate_params("sample.nothing"))
        self.assertEqual(r["result"]["err"]["code"], "plugin.unknown-type")

    def test_an_unknown_method_is_the_protocols_error(self):
        r, _ = FakeHost(self).call("feature.nonesuch")
        self.assertEqual(r["error"]["code"], -32601)


class Ids(unittest.TestCase):
    def test_ids_round_trip_and_match_the_wire(self):
        for n in (0, 5, 31, 32, 2**53 + 1, 2**64 - 1):
            self.assertEqual(arrix.ids.number(arrix.key(n)), n)
        self.assertEqual(arrix.key(5), "0000000000005")
        self.assertEqual(arrix.key(2**64 - 1), "FZZZZZZZZZZZZ")

    def test_text_that_is_no_id_is_refused(self):
        for bad in ("", "5", "0000000000I05", "GZZZZZZZZZZZZ", "0000000000005 "):
            with self.assertRaises(ValueError):
                arrix.ids.number(bad)
        with self.assertRaises(ValueError):
            arrix.key(2**64)


class Codec(unittest.TestCase):
    def test_every_variant_case_round_trips(self):
        ref = {"slot": {"feature": arrix.key(1), "slot": "body"}}
        self.assertEqual(w.Reference.from_json(ref).to_json(), ref)
        loop = {"circle": {"key": arrix.key(9), "center": {"x": 0.0, "y": 1.0}, "radius": 2.0}}
        self.assertEqual(w.ProfileLoop.from_json(loop).to_json(), loop)
        region = {"feature": arrix.key(1), "entities": [arrix.key(2)], "sample": [3, -4]}
        self.assertEqual(w.RegionRef.from_json(region).to_json(), region)
        self.assertEqual(w.RegionRef.from_json(region).sample, (3, -4))

    def test_the_method_table_is_the_worlds(self):
        self.assertEqual(w.METHODS["kernel.extrude"], "plugin-to-host")
        self.assertEqual(w.METHODS["feature.evaluate"], "host-to-plugin")


if __name__ == "__main__":
    unittest.main()
