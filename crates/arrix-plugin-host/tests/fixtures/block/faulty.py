"""`block.py` misbehaving on purpose (plans/boundary-m1 step 6), run as
`python -m arrix.plugin faulty <mode> [marker]`. A `*-once` mode
misbehaves only while the marker file is absent, and creates it as it
does, so the process the host restarts behaves.

crash        exits during `feature.evaluate`
hang         says nothing during `feature.evaluate`
garbage      answers `feature.evaluate` with a line that is no JSON
bad-answer   answers it with JSON that is no `feature-output`
stale        builds nothing and cuts handle 77, which no evaluation gave
none         behaves as `block.py` does
"""

import os
import sys
import time

from arrix import feature

import block  # noqa: F401 - registers block.block

mode = sys.argv[2]
registered = feature._REGISTRY["block.block"]


def misbehaves():
    if not mode.endswith("-once"):
        return True
    marker = sys.argv[3]
    if os.path.exists(marker):
        return False
    open(marker, "w").close()
    return True


def fn(kernel, args):
    if mode == "stale":
        kernel.cut(77, 77)
    if misbehaves():
        what = mode.removesuffix("-once")
        if what == "crash":
            sys.exit(3)
        if what == "hang":
            time.sleep(60)
        if what == "garbage":
            os.write(1, b"this is not json\n")
            sys.exit(0)
    return registered.fn(kernel, args)


feature._REGISTRY["block.block"] = feature._Type(registered.spec, fn)

if mode == "bad-answer":
    # `python -m arrix.plugin` runs as `__main__`, whose runtime is the one serving.
    runtime = sys.modules["__main__"]._Runtime
    dispatch = runtime.dispatch

    def bad(self, method, params):
        if method == "feature.evaluate":
            return {"ok": "a string is no feature-output"}
        return dispatch(self, method, params)

    runtime.dispatch = bad
