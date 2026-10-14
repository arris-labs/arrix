"""The Tier 2 stdio loop (ADR-0022): `python -m arrix.plugin <module>` imports
the plugin's module, which registers its feature types, then serves the
host's requests on stdin and stdout until stdin closes. A kernel call made
while answering `feature.evaluate` is a request to the host, answered
before the evaluation goes on."""

from __future__ import annotations

import importlib
import json
import os
import sys
import traceback
from typing import IO, Any

from . import _wire as w
from . import exchange, feature
from .command import expand as expand_command
from .command import registered as registered_commands

PROTOCOL = 1


class _Host(w.KernelStubs, w.DocumentStubs):
    """The kernel and the document, over the pipe."""

    def __init__(self, inp: IO[str], out: IO[str]):
        self._in, self._out, self._next = inp, out, 0

    def _request(self, method: str, params: dict[str, Any]) -> Any:
        self._next += 1
        _send(self._out, {"id": self._next, "method": method, "params": params})
        line = self._in.readline()
        if not line:
            raise SystemExit(0)
        answer = json.loads(line)
        if answer.get("id") != self._next or "result" not in answer:
            raise RuntimeError(f"{method} was answered with {answer}")
        return answer["result"]


class _Runtime(w.FeatureStubs, w.ExchangeStubs, w.CommandsStubs):
    def __init__(self, host: _Host):
        self._kernel = host
        self._document = host

    def describe(self) -> list[w.FeatureTypeSpec]:
        return feature.registered()

    def evaluate(self, type_id, params, choices, inputs) -> w.FeatureOutput:
        try:
            return feature.evaluate(self._kernel, type_id, params, choices, inputs)
        except w.Failure:
            raise
        except Exception as e:  # the feature's own bug is its diagnostic
            traceback.print_exc()
            raise w.Failure(
                feature.diagnostic("plugin.exception", f"{type(e).__name__}: {e}")
            ) from e


    def dispatch(self, method: str, params: dict[str, Any]) -> Any:
        # The generated interfaces each dispatch their own methods.
        if method.startswith("exchange."):
            return w.ExchangeStubs.dispatch(self, method, params)
        if method.startswith("commands."):
            return w.CommandsStubs.dispatch(self, method, params)
        return w.FeatureStubs.dispatch(self, method, params)

    def exporters(self) -> list[w.ExporterSpec]:
        return exchange.registered()

    def run(self, exporter, subject, params, choices) -> list[w.File]:
        try:
            return exchange.run(self._document, exporter, subject, params, choices)
        except w.Failure:
            raise
        except Exception as e:  # the exporter's own bug is its diagnostic
            traceback.print_exc()
            raise w.Failure(
                feature.diagnostic("plugin.exception", f"{type(e).__name__}: {e}")
            ) from e

    def commands(self) -> list[w.CommandSpec]:
        return registered_commands()

    def expand(self, command_id, context, params, choices, inputs, ids) -> w.CommandGroup:
        try:
            return expand_command(self._document, command_id, context, params, choices, inputs, ids)
        except w.Failure:
            raise
        except Exception as e:  # the command's own bug is its diagnostic
            traceback.print_exc()
            raise w.Failure(
                feature.diagnostic("plugin.exception", f"{type(e).__name__}: {e}")
            ) from e


def _send(out: IO[str], message: dict[str, Any]) -> None:
    message["jsonrpc"] = "2.0"
    out.write(json.dumps(message, separators=(",", ":")) + "\n")
    out.flush()


def serve(inp: IO[str], out: IO[str]) -> None:
    runtime = _Runtime(_Host(inp, out))
    for line in inp:
        request = json.loads(line)
        try:
            if request["method"] == "initialize":
                result = {"protocol": PROTOCOL, "api": w.API_VERSION}
            else:
                result = runtime.dispatch(request["method"], request.get("params") or {})
        except KeyError:
            _send(out, {"id": request["id"], "error": {
                "code": -32601, "message": f"no method `{request['method']}` here"}})
            continue
        _send(out, {"id": request["id"], "result": result})


def main(argv: list[str]) -> None:
    if len(argv) < 2:
        sys.exit("usage: python -m arrix.plugin <module> [args for the module]")
    # The wire is the real stdout; a stray print goes to stderr, not to it.
    out = os.fdopen(os.dup(sys.stdout.fileno()), "w", encoding="utf-8", newline="\n")
    sys.stdout = sys.stderr
    importlib.import_module(argv[1])
    serve(sys.stdin, out)


if __name__ == "__main__":
    main(sys.argv)
