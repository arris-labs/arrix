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
"""

from . import _wire as wire
from ._wire import (  # noqa: F401
    API_VERSION,
    Failure,
    InputKind,
    KernelError,
    QuantityKind,
    Severity,
    SlotKind,
)
from .feature import Args, choice, diagnostic, feature_type, input, param, slot  # noqa: F401
from .ids import key  # noqa: F401
from .shapes import circle_loop, polygon_loop, profile  # noqa: F401
