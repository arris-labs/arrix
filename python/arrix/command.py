"""Commands (docs/PLUGINS.md §Contribution points, 3, ADR-0038): a function
registered under a spec, called with the document's snapshot and the
resolved arguments, returning the group of core commands the gesture
expands to. It reads the evaluated document through `snapshot` (the base
generation's) and never writes: the host checks the group, submits it as
one command, and inverts it for undo. The plugin needs `document-write`."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Any, Callable, Iterator, Mapping

from . import _wire as w
from .feature import diagnostic


@dataclass
class CommandArgs:
    """A command's resolved arguments. `context` says where it runs (a
    part or an assembly, `context.case`, with its id in `context.value`);
    quantities are SI floats and choices words, the spec's defaults where
    the user gave none. An `edge` input arrives resolved (`ResolvedEdge`,
    in the context's frame), the list one per pick, in order. `ids` are
    the fresh ids the host minted: `new_id()` takes the next, and raises
    when the spec's `max_new` is spent."""

    context: w.Context
    params: Mapping[str, float]
    choices: Mapping[str, str]
    _inputs: Mapping[str, list[w.CommandInputValue]]
    _lists: frozenset[str]
    _ids: Iterator[str]

    def input(self, name: str) -> Any:
        """The input's payload, or the list of them for a `list` input."""
        values = [v.value for v in self._inputs.get(name, [])]
        return values if name in self._lists else (values[0] if values else None)

    def new_id(self) -> str:
        try:
            return next(self._ids)
        except StopIteration:
            raise w.Failure(
                diagnostic("command.ids", "the command used more ids than the host gave it")
            ) from None


@dataclass(frozen=True)
class _Command:
    spec: w.CommandSpec
    fn: Callable[[w.DocumentStubs, CommandArgs], Any]


_REGISTRY: dict[str, _Command] = {}


def command(
    id: str,  # noqa: A002 - the spec's word
    title: str,
    *,
    group: str,
    params: list[w.ParamSpec] = (),  # type: ignore[assignment]
    choices: list[w.ChoiceSpec] = (),  # type: ignore[assignment]
    inputs: list[w.InputSpec] = (),  # type: ignore[assignment]
    max_new: int | None = None,
):
    """Registers `fn(snapshot, args)` as the command `id`
    (`<plugin-id>.<name>`), in the ribbon group `group` of the plugin's
    tab. It returns a `CommandGroup`, or `(label, [PluginCommand, …])`. It
    is a pure function of the snapshot and the arguments: no clock,
    randomness, files or network, and ids only from `args.new_id()`."""

    def register(fn):
        if id in _REGISTRY:
            raise ValueError(f"command {id} is registered twice")
        spec = w.CommandSpec(id, title, group, list(params), list(choices), list(inputs), max_new)
        _REGISTRY[id] = _Command(spec, fn)
        return fn

    return register


def registered() -> list[w.CommandSpec]:
    return [c.spec for c in _REGISTRY.values()]


def expand(
    snapshot: w.DocumentStubs,
    command_id: str,
    context: w.Context,
    params: list[w.ParamValue],
    choices: list[w.ChoiceValue],
    inputs: list[w.CommandInput],
    ids: list[str],
) -> w.CommandGroup:
    c = _REGISTRY.get(command_id)
    if c is None:
        raise w.Failure(diagnostic("plugin.unknown-type", f"this plugin has no command {command_id}"))
    grouped: dict[str, list[w.CommandInputValue]] = {}
    for i in inputs:
        grouped.setdefault(i.name, []).append(i.value)
    lists = frozenset(i.name for i in c.spec.inputs if i.list)
    args = CommandArgs(
        context,
        {p.name: p.value.si for p in params},
        {x.name: x.value for x in choices},
        grouped,
        lists,
        iter(ids),
    )
    made = c.fn(snapshot, args)
    if isinstance(made, w.CommandGroup):
        return made
    label, commands = made
    return w.CommandGroup(label, list(commands))
