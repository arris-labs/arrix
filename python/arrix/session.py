"""A client of `arrix serve --stdio`, the session protocol over JSON-RPC lines
(ADR-0023, ADR-0026). Pure standard library. Commands are the plain dicts the
protocol schema (`arrix serve --schema`) describes.

    from arrix.session import Session

    with Session.spawn(plugins=["./boss"]) as s:
        s.initialize()
        result = s.submit(command, base=1)
        s.settle(result["applied"]["generation"])
        s.save("out/")

Every wait has a timeout, and a `serve` that exits is an error, never a hang.
"""

from __future__ import annotations

import base64
import json
import os
import queue
import subprocess
import threading
import time
from collections import deque
from pathlib import Path
from typing import Any, Callable, Iterator, Optional, Sequence

#: The session protocol this client speaks.
PROTOCOL = 1
#: The author a request is made as unless it says otherwise.
ME = "0000000000001"


class SessionError(Exception):
    """Base of what a session raises."""


class ProtocolError(SessionError):
    """A JSON-RPC error: the request was malformed, unknown or too early."""

    def __init__(self, code: int, message: str):
        super().__init__(f"{message} ({code})")
        self.code = code
        self.message = message


class Rejected(SessionError):
    """A request the session answered with `err`: `reason` is that value,
    such as `{"stale": {...}}` or `{"refused": {...}}`."""

    def __init__(self, reason: Any):
        super().__init__(json.dumps(reason))
        self.reason = reason

    @property
    def kind(self) -> str:
        """The rejection's name: `stale`, `refused`, ..."""
        if isinstance(self.reason, dict) and len(self.reason) == 1:
            return next(iter(self.reason))
        return str(self.reason)


class ServeExited(SessionError):
    """`serve` ended before the answer came."""

    def __init__(self, returncode: Optional[int], stderr: str):
        super().__init__(f"arrix serve exited (status {returncode}): {stderr.strip()}")
        self.returncode = returncode
        self.stderr = stderr


class Timeout(SessionError):
    """Nothing satisfying the wait arrived in time."""


class Session:
    """One `arrix serve --stdio` process and the conversation with it."""

    def __init__(self, process: subprocess.Popen, timeout: float = 60.0):
        self._process = process
        self.timeout = timeout
        #: Every notification heard so far (`session.event` params), in order.
        self.heard: list[dict] = []
        self._cursor = 0
        self._lines: queue.Queue = queue.Queue()
        self._stderr: deque = deque(maxlen=50)
        self._next_id = 0
        threading.Thread(target=_read_lines, args=(process.stdout, self._lines), daemon=True).start()
        threading.Thread(target=_keep_lines, args=(process.stderr, self._stderr), daemon=True).start()
        self._closed = False

    @classmethod
    def spawn(
        cls,
        arrix: Optional[str] = None,
        plugins: Sequence[str | os.PathLike] = (),
        doc: Optional[str | os.PathLike] = None,
        timeout: float = 60.0,
        env: Optional[dict] = None,
        cwd: Optional[str | os.PathLike] = None,
    ) -> "Session":
        """Starts `arrix serve --stdio [--plugin <dir>]... [<doc>]`. `arrix`
        is the binary: the argument, else `$ARRIX`, else `arrix` on PATH.
        Call `initialize` before anything else."""
        binary = arrix or os.environ.get("ARRIX") or "arrix"
        argv = [binary, "serve", "--stdio"]
        for plugin in plugins:
            argv += ["--plugin", os.fspath(plugin)]
        if doc is not None:
            argv.append(os.fspath(doc))
        try:
            process = subprocess.Popen(
                argv,
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=env,
                cwd=cwd,
            )
        except OSError as e:
            raise SessionError(f"cannot start `{binary}`: {e}") from e
        return cls(process, timeout)

    # -- the conversation -------------------------------------------------

    def initialize(self, protocol: int = PROTOCOL) -> dict:
        """The handshake; the result names the protocol and the methods."""
        return self._call("initialize", {"protocol": protocol})

    def submit(self, command: dict, base: int, author: str = ME) -> dict:
        """Submits `command` on generation `base`; the outcome is
        `{"applied": {"generation": n, ...}}`. A stale or refused command
        raises `Rejected`."""
        return self._request("submit", {"author": author, "base": base, "command": command})

    def undo(self, author: str = ME) -> dict:
        return self._request("undo", {"author": author})

    def redo(self, author: str = ME) -> dict:
        return self._request("redo", {"author": author})

    def export(self, format: str, bodies: Sequence[dict] = ()) -> dict:
        """Queues an export (`step`, `stl` or `obj`); the file follows as an
        `exported` event, its bytes in base64 (see `exported_bytes`)."""
        return self._request("export", {"bodies": list(bodies), "format": format})

    def load_plugin(self, directory: str | os.PathLike) -> dict:
        """Loads the Tier 2 plugin directory `directory` into the session."""
        return self._request("load_plugin", {"dir": os.fspath(directory)})

    def reload_plugin(self, plugin: str) -> dict:
        """Restarts the loaded `plugin`, taking its code and specs again."""
        return self._request("reload_plugin", {"plugin": plugin})

    def save(self, to: Optional[str | os.PathLike] = None) -> dict:
        """The document as `{"files": {path: text}, "blobs": {path: base64}}`;
        with `to`, also written there as a document directory."""
        saved = self._call("session.save", {})
        if to is not None:
            root = Path(to)
            for path, text in saved["files"].items():
                _write(root / path, text.encode("utf-8"))
            for path, blob in saved["blobs"].items():
                _write(root / path, base64.b64decode(blob))
        return saved

    # -- the events --------------------------------------------------------

    def events(self, idle: float = 1.0) -> Iterator[dict]:
        """The notifications not yet yielded, in order, then new ones as
        they arrive, until none comes within `idle` seconds or `serve`
        closes its output."""
        while True:
            if self._cursor < len(self.heard):
                self._cursor += 1
                yield self.heard[self._cursor - 1]
                continue
            try:
                self._pump(time.monotonic() + idle)
            except (Timeout, ServeExited):
                return

    def wait_event(
        self,
        match: Optional[Callable[[dict], bool]] = None,
        timeout: Optional[float] = None,
        **fields: Any,
    ) -> dict:
        """The first heard notification (earlier ones count) that equals
        `fields` and satisfies `match`, e.g. `wait_event(kind="finished",
        generation=3)`. Raises `Timeout`, or `ServeExited`."""
        deadline = time.monotonic() + (self.timeout if timeout is None else timeout)
        seen = 0
        while True:
            while seen < len(self.heard):
                event = self.heard[seen]
                seen += 1
                if all(event.get(k) == v for k, v in fields.items()) and (match is None or match(event)):
                    return event
            self._pump(deadline)

    def settle(self, generation: int, timeout: Optional[float] = None) -> dict:
        """Waits until the evaluation of `generation` (or a newer one)
        has finished; returns its `finished` event."""
        return self.wait_event(
            lambda e: e.get("kind") == "finished" and e.get("generation", -1) >= generation,
            timeout,
        )

    def outcome(self, generation: int, feature: str, timeout: Optional[float] = None) -> dict:
        """A feature's outcome in the evaluation of `generation`."""
        event = self.wait_event(
            lambda e: e.get("kind") == "evaluated"
            and e.get("generation") == generation
            and e.get("event", {}).get("feature") == feature,
            timeout,
        )
        return event["event"]["outcome"]

    @staticmethod
    def exported_bytes(event: dict) -> bytes:
        """The file an `exported` event carries."""
        return base64.b64decode(event["bytes"]["Ok"])

    # -- ending ------------------------------------------------------------

    def close(self) -> None:
        """Asks `serve` to exit and waits for it, killing it when it does not."""
        if self._closed:
            return
        self._closed = True
        try:
            self._write({"jsonrpc": "2.0", "method": "exit"})
            self._process.stdin.close()
        except (OSError, ValueError):
            pass
        try:
            self._process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self._process.kill()
            self._process.wait()
        for stream in (self._process.stdin, self._process.stdout, self._process.stderr):
            try:
                stream.close()
            except (OSError, ValueError):
                pass

    def __enter__(self) -> "Session":
        return self

    def __exit__(self, *exc: Any) -> None:
        self.close()

    # -- the wire ------------------------------------------------------------

    def _request(self, method: str, params: dict) -> dict:
        result = self._call(f"session.{method}", params)
        if "err" in result:
            raise Rejected(result["err"])
        return result["ok"]

    def _call(self, method: str, params: dict) -> Any:
        self._next_id += 1
        ident = self._next_id
        try:
            self._write({"jsonrpc": "2.0", "id": ident, "method": method, "params": params})
        except (OSError, ValueError) as e:
            raise self._exited() from e
        deadline = time.monotonic() + self.timeout
        while True:
            message = self._pump(deadline)
            if message is not None and message.get("id") == ident:
                if "error" in message:
                    raise ProtocolError(message["error"]["code"], message["error"]["message"])
                return message["result"]

    def _write(self, message: dict) -> None:
        self._process.stdin.write(json.dumps(message) + "\n")
        self._process.stdin.flush()

    def _pump(self, deadline: float) -> Optional[dict]:
        """Reads one line: a notification is kept in `heard`; a response is
        returned."""
        wait = max(0.0, deadline - time.monotonic())
        try:
            line = self._lines.get(timeout=wait)
        except queue.Empty:
            raise Timeout("nothing arrived from arrix serve in time") from None
        if line is None:
            self._lines.put(None)
            raise self._exited()
        message = json.loads(line)
        if message.get("method") == "session.event":
            self.heard.append(message["params"])
            return None
        return message

    def _exited(self) -> ServeExited:
        try:
            code = self._process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            code = None
        time.sleep(0.05)
        return ServeExited(code, "".join(self._stderr))


def _read_lines(stream, lines: queue.Queue) -> None:
    """Queues each line of `stream`, then `None` at its end."""
    try:
        for line in stream:
            lines.put(line.rstrip("\n"))
    except (ValueError, OSError):
        pass
    finally:
        lines.put(None)


def _keep_lines(stream, kept: deque) -> None:
    try:
        for line in stream:
            kept.append(line)
    except (ValueError, OSError):
        pass


def _write(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)
