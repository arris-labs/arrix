"""The fasteners plugin's table and its `place` expansion, against a fake
snapshot (plans/c3-m3 step 9). Run by `cargo test -p arrix-plugin-host
--test python_sdk_plugins` with the SDK on PYTHONPATH."""

import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import fasteners  # noqa: E402  (registers the plugin's types)
from arrix import _wire as w  # noqa: E402
import importlib  # noqa: E402

commands = importlib.import_module("arrix.command")

ASSEMBLY = "0000000000AAA"
BRACKET = "0000000000BBB"
MM = 1e-3
Q = w.QuantityKind.LENGTH


def v(x, y, z):
    return w.Vec3(x, y, z)


def hole(diameter_mm, name="edge:hole", axis=(0, 0, -1), normals=((0, 0, 1),)):
    circle = w.CircleEdge(v(0.02, 0, 0.005), v(*axis), diameter_mm * MM / 2)
    return w.CommandInput(
        "holes",
        w.CommandInputValue.edge(
            w.ResolvedEdge([BRACKET], w.PersistentRef(name), w.EdgeCurve.circle(circle), [v(*n) for n in normals])
        ),
    )


class Snapshot:
    def __init__(self, parts=(), components=()):
        self._parts, self._components = list(parts), list(components)

    def parts(self):
        return self._parts

    def assemblies(self):
        return [w.AssemblyInfo(ASSEMBLY, "plate", self._components, [])]


def made_part(part_id, size, length):
    feature = w.FeatureInfo(
        part_id + "F",
        fasteners.SOCKET_HEAD,
        "Socket head cap screw",
        [w.ChoiceValue("size", size)],
        [w.ParamValue("length", w.Quantity(Q, length))],
    )
    return w.PartInfo(part_id, f"{size}", [], [feature])


def placed(part_id, n):
    pose = w.Frame(v(0, 0, 0), v(1, 0, 0), v(0, 0, 1))
    return w.ComponentInfo([f"C{n}"], f"c{n}", w.ComponentOf.part(part_id), pose, [])


def expand(snapshot, holes, length=0.01, size="auto", context=None, ids=40):
    return commands.expand(
        snapshot,
        "fasteners.place",
        context or w.Context.assembly(ASSEMBLY),
        [w.ParamValue("length", w.Quantity(Q, length))],
        [w.ChoiceValue("size", size)],
        holes,
        [f"{n:013d}" for n in range(1, ids + 1)],
    )


def cases(group):
    return [(c.case, c.value) for c in group.commands]


class Table(unittest.TestCase):
    def test_the_rows_grow_with_the_thread(self):
        for a, b in zip(fasteners.TABLE, fasteners.TABLE[1:]):
            self.assertLess(a.thread, b.thread)
            self.assertLess(a.head, b.head)
            self.assertLess(a.clearance, b.clearance)
        for s in fasteners.TABLE:
            self.assertEqual(s.height, s.thread)
            self.assertGreater(s.head, s.clearance)
            self.assertLess(s.clearance, s.head)

    def test_a_size_is_the_largest_clearance_that_fits(self):
        self.assertEqual(fasteners.size_for(3.4).name, "M3")
        self.assertEqual(fasteners.size_for(4.0).name, "M3")
        self.assertEqual(fasteners.size_for(8.0).name, "M6")
        self.assertEqual(fasteners.size_for(100.0).name, "M12")
        self.assertIsNone(fasteners.size_for(2.3))


class Place(unittest.TestCase):
    def test_one_part_per_size_and_one_component_and_joint_per_screw(self):
        group = expand(Snapshot(), [hole(3.4), hole(3.4, "edge:other"), hole(4.5, "edge:big")])
        self.assertEqual(group.label, "Place screws")
        kinds = [k for k, _ in cases(group)]
        self.assertEqual(kinds.count("add-part"), 2)
        self.assertEqual(kinds.count("add-material"), 1)
        self.assertEqual(kinds.count("add-component"), 3)
        self.assertEqual(kinds.count("add-joint"), 3)
        names = [c.component.name for k, c in cases(group) if k == "add-component"]
        self.assertEqual(names, ["M3x10 1", "M3x10 2", "M4x10 1"])

    def test_a_part_made_before_is_reused_and_numbering_goes_on(self):
        snapshot = Snapshot([made_part("0000000000P01", "M3", 0.01)], [placed("0000000000P01", 1)])
        group = expand(snapshot, [hole(3.4)])
        kinds = [k for k, _ in cases(group)]
        self.assertNotIn("add-part", kinds)
        self.assertNotIn("add-material", kinds)
        component = next(c for k, c in cases(group) if k == "add-component")
        self.assertEqual(component.component.name, "M3x10 2")
        self.assertEqual(component.component.of.value, "0000000000P01")
        joint = next(c for k, c in cases(group) if k == "add-joint")
        self.assertEqual(
            joint.joint.child.axis.encoded,
            f"edge:sweep.0000000000P01F.start-edge.{fasteners.arrix.key(fasteners.HEAD)}",
        )

    def test_another_length_is_another_part(self):
        snapshot = Snapshot([made_part("0000000000P01", "M3", 0.01)])
        group = expand(snapshot, [hole(3.4)], length=0.012)
        self.assertEqual([k for k, _ in cases(group)].count("add-part"), 1)

    def test_a_given_size_overrides_the_hole(self):
        group = expand(Snapshot(), [hole(8.0)], size="M3")
        part = next(c for k, c in cases(group) if k == "add-part")
        self.assertEqual(part.name, "M3x10")

    def test_a_hole_no_size_fits_is_refused_by_name(self):
        with self.assertRaises(w.Failure) as caught:
            expand(Snapshot(), [hole(2.0, "edge:tiny")])
        self.assertEqual(caught.exception.diagnostic.code, "fasteners.size")
        self.assertIn("edge:tiny", caught.exception.diagnostic.message)

    def test_the_joint_flips_when_the_circle_opposes_the_faces_normal(self):
        down = expand(Snapshot(), [hole(3.4, axis=(0, 0, -1), normals=((0, 0, 1),))])
        up = expand(Snapshot(), [hole(3.4, axis=(0, 0, 1), normals=((0, 0, 1),))])
        flip = lambda g: next(c for k, c in cases(g) if k == "add-joint").joint.flip
        self.assertTrue(flip(down))
        self.assertFalse(flip(up))

    def test_an_edge_bounding_no_planar_face_is_refused(self):
        with self.assertRaises(w.Failure) as caught:
            expand(Snapshot(), [hole(3.4, normals=())])
        self.assertEqual(caught.exception.diagnostic.code, "fasteners.side")

    def test_a_part_context_a_missing_pick_and_a_bad_length_are_refused(self):
        for code, kwargs in [
            ("fasteners.context", dict(holes=[hole(3.4)], context=w.Context.part(BRACKET))),
            ("fasteners.holes", dict(holes=[])),
            ("fasteners.length", dict(holes=[hole(3.4)], length=0.0)),
        ]:
            with self.subTest(code):
                with self.assertRaises(w.Failure) as caught:
                    expand(Snapshot(), **kwargs)
                self.assertEqual(caught.exception.diagnostic.code, code)

    def test_the_expansion_is_a_pure_function(self):
        holes = [hole(3.4), hole(4.5, "edge:big")]
        self.assertEqual(expand(Snapshot(), holes), expand(Snapshot(), holes))


if __name__ == "__main__":
    unittest.main()
