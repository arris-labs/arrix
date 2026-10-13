"""`arrix.session` and `arrix.eval` against the built `arrix` (plans/boundary-m3
step 7). Run by `cargo test -p arrix-plugin-host --test python_sdk`, which sets
`ARRIX` to the binary; without it these skip, and say so."""

import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

import arrix
from arrix.session import ProtocolError, Rejected, ServeExited, Session, Timeout

ARRIX = os.environ.get("ARRIX")
REPO = Path(__file__).resolve().parents[2]
STANDOFF = REPO / "python/examples/standoff"
DOC = REPO / "tests/docs/standoff"
# The standoff plugin starts `python3 -m arrix.plugin`, which needs the SDK.
ENV = {**os.environ, "PYTHONPATH": str(REPO / "python")}


def plate_commands(features=2):
    """The commands that build `tests/docs/standoff`'s first `features`
    features, as plain dicts: the parameters, the part, its features."""
    params = json.loads((DOC / "params.json").read_text())
    part = json.loads(next((DOC / "parts").glob("*.json")).read_text())
    commands = [{"add_param": {"param": k, "record": v}} for k, v in params.items()]
    empty = {k: part[k] for k in ("id", "name", "rollback")}
    commands.append({"add_part": {"part": {**empty, "history": [], "features": {}}}})
    for at, feature in enumerate(part["history"][:features]):
        record = part["features"][feature]
        commands.append({"add_feature": {"part": part["id"], "at": at, "record": record}})
    return commands, part["history"]


BOM = REPO / "python/examples/bom"
EXCHANGE = REPO / "tests/docs/assembly-exchange"


@unittest.skipUnless(ARRIX, "ARRIX names the built binary; run through cargo test")
class SessionTest(unittest.TestCase):
    def spawn(self, **kw):
        kw.setdefault("timeout", 60.0)
        session = Session.spawn(arrix=ARRIX, env=ENV, **kw)
        self.addCleanup(session.close)
        return session

    def build_plate(self, s):
        commands, history = plate_commands()
        generation = 0
        for command in commands:
            generation = s.submit(command, base=generation)["applied"]["generation"]
        return generation, history

    def test_builds_a_plate_settles_and_saves(self):
        s = self.spawn()
        hello = s.initialize()
        self.assertEqual(hello["protocol"], 1)
        self.assertIn("session.submit", hello["methods"])
        generation, history = self.build_plate(s)
        s.settle(generation)
        self.assertEqual(s.outcome(generation, history[1])["status"], "ok")
        out = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, out, True)
        saved = s.save(out / "doc")
        self.assertIn("document.json", saved["files"])
        self.assertTrue((out / "doc/document.json").is_file())
        lines = arrix.eval(out / "doc", arrix=ARRIX)
        self.assertEqual(lines[0]["status"], "ok")
        self.assertEqual(len(lines[0]["bodies"]), 1)

    def test_an_exporter_at_tier_2_writes_the_bill_of_materials(self):
        assembly = json.loads(next((EXCHANGE / "assemblies").glob("*.json")).read_text())["id"]
        s = self.spawn(plugins=[BOM], doc=EXCHANGE)
        s.initialize()
        s.export_with("bom.csv", assembly=assembly)
        files = s.exported_files(s.wait_event(kind="exported"))
        self.assertEqual([name for name, _ in files], ["bom.csv"])
        rows = files[0][1].decode().splitlines()
        self.assertEqual(rows[0], "part,count,mass (kg),material")
        self.assertTrue(rows[1].startswith("Bracket,1,"), rows)
        self.assertTrue(rows[-1].startswith("total,6,"), rows)
        # An option is a word: grams, not kilograms.
        s.export_with("bom.csv", assembly=assembly, choices={"mass-unit": "g"})
        grams = s.wait_event(
            lambda e: e.get("kind") == "exported"
            and b"mass (g)" in s.exported_files(e)[0][1]
        )
        self.assertEqual(len(s.exported_files(grams)), 1)

    def test_events_iterate_in_order(self):
        s = self.spawn()
        s.initialize()
        generation, _ = self.build_plate(s)
        s.settle(generation)
        kinds = [e["kind"] for e in s.events(idle=0.3)]
        self.assertIn("evaluated", kinds)
        self.assertIn("finished", kinds)

    def test_a_stale_submit_is_rejected(self):
        s = self.spawn()
        s.initialize()
        generation, _ = self.build_plate(s)
        commands, _ = plate_commands()
        param = commands[0]["add_param"]["param"]
        with self.assertRaises(Rejected) as caught:
            s.submit({"set_param": {"param": param, "expr": "70 mm"}}, base=0, author="0000000000002")
        self.assertEqual(caught.exception.kind, "stale")
        with self.assertRaises(Rejected) as caught:
            s.submit(commands[0], base=generation)
        self.assertEqual(caught.exception.kind, "refused")

    def test_a_protocol_error_is_one(self):
        s = self.spawn()
        with self.assertRaises(ProtocolError) as caught:
            s.save()
        self.assertIn("initialize", caught.exception.message)
        s.initialize()
        with self.assertRaises(ProtocolError):
            s.initialize()
        with self.assertRaises(ProtocolError):
            s._call("session.nonsense", {})

    def test_load_and_reload_a_plugin(self):
        out = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, out, True)
        plugin = out / "standoff"
        shutil.copytree(STANDOFF, plugin, ignore=shutil.ignore_patterns("__pycache__"))
        s = self.spawn()
        s.initialize()
        loaded = s.load_plugin(plugin)
        self.assertEqual(loaded["loaded"]["generation"], 1)
        registry = s.wait_event(kind="registry")
        self.assertEqual(registry["added"][0]["id"], "standoff.standoff")
        reloaded = s.reload_plugin("standoff")
        self.assertEqual(reloaded["loaded"]["generation"], 2)
        with self.assertRaises(Rejected):
            s.load_plugin(out / "nowhere")

    def test_serve_exiting_is_reported(self):
        s = self.spawn(doc="/nonexistent/doc")
        with self.assertRaises(ServeExited) as caught:
            s.initialize()
        self.assertEqual(caught.exception.returncode, 2)
        self.assertIn("nonexistent", caught.exception.stderr)

    def test_serve_dying_mid_session_is_reported(self):
        s = self.spawn()
        s.initialize()
        s._process.kill()
        with self.assertRaises(ServeExited):
            s.wait_event(kind="never")
        with self.assertRaises(ServeExited):
            s.save()

    def test_a_wait_times_out(self):
        s = self.spawn()
        s.initialize()
        with self.assertRaises(Timeout):
            s.wait_event(timeout=0.2, kind="finished", generation=99)

    def test_a_missing_binary_is_a_session_error(self):
        with self.assertRaises(arrix.session.SessionError):
            Session.spawn(arrix="/nonexistent/arrix")


@unittest.skipUnless(ARRIX, "ARRIX names the built binary; run through cargo test")
class EvalTest(unittest.TestCase):
    def test_parses_the_line_and_checks_its_format(self):
        [line] = arrix.eval(DOC, arrix=ARRIX, plugins=[STANDOFF], env=ENV)
        self.assertEqual(line["format"], arrix.EVAL_FORMAT)
        self.assertEqual(line["status"], "ok")
        self.assertEqual([f["status"] for f in line["features"]][-2:], ["ok", "ok"])

    def test_without_the_plugin_the_standoff_is_frozen_or_failed(self):
        [line] = arrix.eval(
            DOC, arrix=ARRIX, plugins=[STANDOFF], without_plugins=["standoff"], env=ENV
        )
        standoff = [f for f in line["features"] if f["type"] == "standoff.standoff"]
        self.assertEqual(len(standoff), 1)

    def test_summary_and_an_unreadable_document(self):
        lines = arrix.eval([DOC], arrix=ARRIX, plugins=[STANDOFF], env=ENV, summary=True)
        self.assertEqual(lines[-1]["summary"], [])
        with self.assertRaises(arrix.EvalError):
            arrix.eval("/nonexistent/doc", arrix=ARRIX)

    def test_a_format_it_does_not_know_is_an_error(self):
        out = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, out, True)
        fake = out / "arrix"
        fake.write_text(
            f"#!{sys.executable}\nprint('{{\"format\": 99, \"doc\": \"x\"}}')\n"
        )
        fake.chmod(0o755)
        with self.assertRaises(arrix.EvalError) as caught:
            arrix.eval(DOC, arrix=str(fake))
        self.assertIn("99", str(caught.exception))


if __name__ == "__main__":
    unittest.main()
