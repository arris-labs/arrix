"""Exporters (docs/PLUGINS.md §Contribution points, 4): a function
registered under a spec, called with the document's snapshot and the
resolved options, returning the files it writes. It reads the evaluated
document through `snapshot` (parts, meshes, mass, assemblies) and never
touches a path: the host writes the files, the first one the main."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Mapping

from . import _wire as w
from .feature import diagnostic


@dataclass(frozen=True)
class ExportArgs:
    """An exporter's arguments. `subject` is the id of the part or assembly
    it is of (`subject_kind` says which); `params` are SI floats and
    `choices` words, the spec's defaults where the user gave none."""

    subject: str
    subject_kind: str
    params: Mapping[str, float]
    choices: Mapping[str, str]


@dataclass(frozen=True)
class _Exporter:
    spec: w.ExporterSpec
    fn: Callable[[w.DocumentStubs, ExportArgs], Any]


_REGISTRY: dict[str, _Exporter] = {}


def exporter(
    id: str,  # noqa: A002 - the spec's word
    title: str,
    extension: str,
    *,
    subject: str = w.SubjectKind.ASSEMBLY,
    params: list[w.ParamSpec] = (),  # type: ignore[assignment]
    choices: list[w.ChoiceSpec] = (),  # type: ignore[assignment]
):
    """Registers `fn(snapshot, args)` as the exporter `id`
    (`<plugin-id>.<name>`). It returns a list of `File`, or of `(name,
    bytes)` pairs, or a dict of name to bytes (the first is the main file);
    a name is a relative `/`-separated path. It is a pure function of the
    snapshot and the options: no clock, randomness, files or network."""

    def register(fn):
        if id in _REGISTRY:
            raise ValueError(f"exporter {id} is registered twice")
        spec = w.ExporterSpec(id, title, extension, subject, list(params), list(choices))
        _REGISTRY[id] = _Exporter(spec, fn)
        return fn

    return register


def registered() -> list[w.ExporterSpec]:
    return [e.spec for e in _REGISTRY.values()]


def run(
    snapshot: w.DocumentStubs,
    exporter_id: str,
    subject: w.Subject,
    params: list[w.ParamValue],
    choices: list[w.ChoiceValue],
) -> list[w.File]:
    e = _REGISTRY.get(exporter_id)
    if e is None:
        raise w.Failure(diagnostic("plugin.unknown-type", f"this plugin has no exporter {exporter_id}"))
    args = ExportArgs(
        subject.value,
        subject.case,
        {p.name: p.value.si for p in params},
        {c.name: c.value for c in choices},
    )
    made = e.fn(snapshot, args)
    if isinstance(made, Mapping):
        made = list(made.items())
    files = []
    for f in made or []:
        files.append(f if isinstance(f, w.File) else w.File(f[0], bytes(f[1])))
    return files
