"""ArriX's Python SDK for Tier 2 plugins: pure standard library, no Rust
(ADR-0021). The wire's types and the kernel's stubs are generated from the
plugin API's WIT (`_wire.py`).

    import arrix
    from arrix import KernelError, feature_type, param, input, slot

    @feature_type("block.block", "Block", params=[param("size", "Size", default="10 mm")],
                  inputs=[input("plane", "Plane", "plane")], outputs=[slot("body")])
    def block(kernel, args):
        body = kernel.extrude(profile, args.params["size"])
        return {"body": body}

Run with `python -m arrix.plugin <module>` (the manifest's `[tier2] command`).

The same package drives ArriX from outside: `arrix.session.Session` speaks
`arrix serve --stdio`, and `arrix.eval(paths)` returns `arrix eval`'s lines.
"""

from . import _wire as wire
from . import session  # noqa: F401
from ._wire import (  # noqa: F401
    API_VERSION,
    DocumentError,
    File,
    Failure,
    InputKind,
    KernelError,
    QuantityKind,
    Severity,
    SlotKind,
    SubjectKind,
)
from ._eval import EVAL_FORMAT, EvalError, eval  # noqa: F401
from .command import CommandArgs, command  # noqa: F401
from .exchange import ExportArgs, exporter  # noqa: F401
from .feature import Args, choice, diagnostic, feature_type, input, param, slot  # noqa: F401
from .ids import key  # noqa: F401
from .shapes import circle_loop, polygon_loop, profile  # noqa: F401
