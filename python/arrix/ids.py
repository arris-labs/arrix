"""Ids as the wire writes them (ADR-0022 §3): 13 Crockford base32 characters
for 64 bits, the first at most `F`. A curve key must be stable across
evaluations for the same logical curve, so a plugin makes its keys with
`key(n)` from numbers it can reproduce."""

_ALPHABET = "0123456789ABCDEFGHJKMNPQRSTVWXYZ"
_LENGTH = 13


def key(n: int) -> str:
    """The id text of the 64-bit number `n`."""
    if not 0 <= n < 1 << 64:
        raise ValueError(f"{n} is no 64-bit id")
    return "".join(_ALPHABET[(n >> (5 * (_LENGTH - 1 - i))) & 31] for i in range(_LENGTH))


def number(text: str) -> int:
    """The number of an id's text."""
    if len(text) != _LENGTH or any(c not in _ALPHABET for c in text):
        raise ValueError(f"{text!r} is no id")
    n = 0
    for c in text:
        n = n << 5 | _ALPHABET.index(c)
    if n >= 1 << 64:
        raise ValueError(f"{text!r} is no id")
    return n
