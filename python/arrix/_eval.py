"""`arrix.eval`: `arrix eval` through the Python API (ADR-0026). The same
lines, parsed; a line whose `format` this SDK does not know is an error."""

from __future__ import annotations

import json
import os
import subprocess
from typing import Any, Optional, Sequence

#: The eval line `format` this SDK reads.
EVAL_FORMAT = 1


class EvalError(Exception):
    """`arrix eval` could not read a document, or printed what this SDK
    does not understand."""


def eval(  # noqa: A001 - the SDK's name for it
    paths: str | os.PathLike | Sequence[str | os.PathLike],
    *,
    arrix: Optional[str] = None,
    plugins: Sequence[str | os.PathLike] = (),
    without_plugins: Sequence[str] = (),
    sweep: Optional[str] = None,
    summary: bool = False,
    timings: bool = False,
    jobs: Optional[int] = None,
    budget: Optional[int] = None,
    env: Optional[dict] = None,
    cwd: Optional[str | os.PathLike] = None,
    timeout: float = 600.0,
) -> list[dict[str, Any]]:
    """Evaluates the documents (directories or `.arrx` files) and returns
    their eval lines, parsed, in `arrix eval`'s order; with `summary`, the
    last is the failure histogram (`{"summary": [...]}`). A document that
    fails to evaluate is a line with `"status": "failed"`, not an error."""
    if isinstance(paths, (str, os.PathLike)):
        paths = [paths]
    argv = [arrix or os.environ.get("ARRIX") or "arrix", "eval"]
    for plugin in plugins:
        argv += ["--plugin", os.fspath(plugin)]
    for plugin in without_plugins:
        argv += ["--without-plugin", plugin]
    if sweep is not None:
        argv += ["--sweep", sweep]
    argv += ["--summary"] * summary + ["--timings"] * timings
    if jobs is not None:
        argv += ["--jobs", str(jobs)]
    if budget is not None:
        argv += ["--budget", str(budget)]
    argv += [os.fspath(p) for p in paths]
    try:
        done = subprocess.run(
            argv, capture_output=True, text=True, env=env, cwd=cwd, timeout=timeout
        )
    except (OSError, subprocess.TimeoutExpired) as e:
        raise EvalError(f"cannot run `{argv[0]} eval`: {e}") from e
    if done.returncode not in (0, 1):
        raise EvalError(f"arrix eval exited {done.returncode}: {done.stderr.strip()}")
    lines = []
    for text in done.stdout.splitlines():
        line = json.loads(text)
        if "summary" not in line and line.get("format") != EVAL_FORMAT:
            raise EvalError(
                f"an eval line of format {line.get('format')!r}; this SDK reads {EVAL_FORMAT}"
            )
        lines.append(line)
    return lines
