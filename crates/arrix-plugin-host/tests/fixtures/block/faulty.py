"""`block.py` misbehaving on purpose (plans/boundary-m1 step 6). The mode
is its first argument; `*-once` modes misbehave only while the marker file
named by the second argument is absent, and create it as they do, so the
process the host restarts behaves.

crash        exits during `feature.evaluate`
hang         says nothing during `feature.evaluate`
garbage      answers `feature.evaluate` with a line that is no JSON
bad-answer   answers it with JSON that is no `feature-output`
none         behaves as `block.py` does
stale        builds nothing and cuts handle 77, which no evaluation gave
"""

import os
import sys
import time

import block

base = block.answer


def once():
    marker = sys.argv[2]
    if os.path.exists(marker):
        return False
    open(marker, "w").close()
    return True


def answer(request):
    mode = sys.argv[1]
    if mode == "none" or request["method"] != "feature.evaluate" or (mode.endswith("-once") and not once()):
        return base(request)
    mode = mode.removesuffix("-once")
    if mode == "crash":
        sys.exit(3)
    if mode == "hang":
        time.sleep(60)
    if mode == "garbage":
        sys.stdout.write("this is not json\n")
        sys.stdout.flush()
        sys.exit(0)
    if mode == "bad-answer":
        return {"ok": "a string is no feature-output"}
    if mode == "stale":
        return block.kernel("kernel.cut", target=77, tool=77)
    raise KeyError(mode)


block.answer = answer
block.serve()
