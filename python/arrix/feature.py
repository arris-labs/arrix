"""Feature types (docs/PLUGINS.md §Contribution points, 1): a function
registered under a spec, called with the host's kernel and the resolved
arguments, returning the slots it built."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Mapping

from . import _wire as w


def param(
    name: str,
    title: str,
    kind: str = w.QuantityKind.LENGTH,
    default: str = "0",
    shown_when: w.ShownWhen | None = None,
) -> w.ParamSpec:
    """A numeric field; `default` is an expression (`20`, `1 mm`)."""
    return w.ParamSpec(name, title, kind, default, shown_when)


def choice(
    name: str,
    title: str,
    options: Mapping[str, str],
    default: str | None = None,
) -> w.ChoiceSpec:
    """One of a few words: `options` maps each word to its title."""
    opts = [w.ChoiceOption(n, t) for n, t in options.items()]
    return w.ChoiceSpec(name, title, opts, default)


def input(  # noqa: A001 - the plugin API's word
    name: str,
    title: str,
    kind: str,
    list: bool = False,  # noqa: A002
    shown_when: w.ShownWhen | None = None,
) -> w.InputSpec:
    """What the feature reads: `kind` is an `InputKind`."""
    return w.InputSpec(name, title, kind, list, shown_when)


def slot(name: str, kind: str = w.SlotKind.BODY, modifies: str | None = None) -> w.SlotSpec:
    """An output slot; one that `modifies` a body input is its next version."""
    return w.SlotSpec(name, kind, modifies)


@dataclass(frozen=True)
class Args:
    """A feature's resolved arguments. Quantities are SI floats (metres,
    radians); an input is its payload (a `Frame` for a plane, a handle for
    a body); a `list` input is a list of them."""

    params: Mapping[str, float]
    choices: Mapping[str, str]
    _inputs: Mapping[str, list[w.InputValue]]
    _lists: frozenset[str]

    def input(self, name: str) -> Any:
        values = [v.value for v in self._inputs[name]]
        return values if name in self._lists else values[0]


@dataclass(frozen=True)
class _Type:
    spec: w.FeatureTypeSpec
    fn: Callable[[w.KernelStubs, Args], Any]


_REGISTRY: dict[str, _Type] = {}


def feature_type(
    id: str,  # noqa: A002 - the spec's word
    title: str,
    *,
    version: int = 1,
    params: list[w.ParamSpec] = (),  # type: ignore[assignment]
    choices: list[w.ChoiceSpec] = (),  # type: ignore[assignment]
    inputs: list[w.InputSpec] = (),  # type: ignore[assignment]
    outputs: list[w.SlotSpec] = (),  # type: ignore[assignment]
):
    """Registers `fn(kernel, args)` as the feature type `id`
    (`<plugin-id>.<name>`). It returns a `FeatureOutput`, or a dict of slot
    name to body handle, or raises `Failure` for a diagnostic of its own;
    the kernel's refusals raise `KernelError`, which ends it the same way."""

    def register(fn):
        if id in _REGISTRY:
            raise ValueError(f"feature type {id} is registered twice")
        spec = w.FeatureTypeSpec(
            id, version, title, list(params), list(choices), list(inputs), list(outputs)
        )
        _REGISTRY[id] = _Type(spec, fn)
        return fn

    return register


def registered() -> list[w.FeatureTypeSpec]:
    return [t.spec for t in _REGISTRY.values()]


def evaluate(
    kernel: w.KernelStubs,
    type_id: str,
    params: list[w.ParamValue],
    choices: list[w.ChoiceValue],
    inputs: list[w.ResolvedInput],
) -> w.FeatureOutput:
    t = _REGISTRY.get(type_id)
    if t is None:
        raise w.Failure(diagnostic("plugin.unknown-type", f"this plugin has no feature type {type_id}"))
    grouped: dict[str, list[w.InputValue]] = {}
    for i in inputs:
        grouped.setdefault(i.name, []).append(i.value)
    lists = frozenset(i.name for i in t.spec.inputs if i.list)
    args = Args({p.name: p.value.si for p in params}, {c.name: c.value for c in choices}, grouped, lists)
    made = t.fn(kernel, args)
    if isinstance(made, w.FeatureOutput):
        return made
    slots = [
        w.SlotOutput(name, w.OutputValue.body(h) if isinstance(h, int) else w.OutputValue.plane(h))
        for name, h in (made or {}).items()
    ]
    return w.FeatureOutput(slots, None)


def diagnostic(code: str, message: str, severity: str = w.Severity.ERROR) -> w.Diagnostic:
    return w.Diagnostic(severity, code, message, [], [])
