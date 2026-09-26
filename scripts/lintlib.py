"""Shared helpers for the gate's lints (docs/ARCHITECTURE.md §Gates).

The lints read Rust source as written, not as resolved: comments and the
contents of string and char literals are blanked first, so a brace or a
path inside them neither moves a depth count nor trips a rule. Each lint
takes the repository root from its own location, so `scripts/gate-selftest`
can run it on a copy of the tree.
"""
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Where Rust source lives: the core crates and the first-party plugins.
SOURCE_DIRS = ("crates", "plugins")

TEST_MOD_RE = re.compile(r"^\s*mod\s+tests\b")


def rel(path):
    return path.relative_to(ROOT).as_posix()


def rust_files(include_test_dirs=True):
    """Every `.rs` file under the source dirs, sorted, `target/` excluded.

    With `include_test_dirs=False`, integration-test, bench and example
    directories are skipped: code that never ships.
    """
    skip = {"target"} if include_test_dirs else {"target", "tests", "benches", "examples"}
    for base in SOURCE_DIRS:
        for path in sorted((ROOT / base).rglob("*.rs")):
            if skip.intersection(path.relative_to(ROOT).parts):
                continue
            yield path


def strip_noise(line, in_block_comment):
    """Blank out string/char literal contents and comments on one line.

    Returns (clean_line, still_in_block_comment).
    """
    out = []
    i = 0
    n = len(line)
    in_block = in_block_comment
    while i < n:
        if in_block:
            end = line.find("*/", i)
            if end == -1:
                return "".join(out), True
            i = end + 2
            in_block = False
            continue
        c = line[i]
        if c == "/" and i + 1 < n and line[i + 1] == "/":
            break
        if c == "/" and i + 1 < n and line[i + 1] == "*":
            in_block = True
            i += 2
            continue
        if c == '"':
            j = i + 1
            while j < n and line[j] != '"':
                j += 2 if line[j] == "\\" else 1
            out.append('""')
            i = j + 1
            continue
        if c == "'":
            # A char literal ('a', '\n') closes shortly after; a lifetime
            # ('a) does not.
            if i + 3 < n and line[i + 1] == "\\" and line[i + 3] == "'":
                out.append("''")
                i += 4
                continue
            if i + 2 < n and line[i + 2] == "'":
                out.append("''")
                i += 3
                continue
        out.append(c)
        i += 1
    return "".join(out), in_block


def clean_lines(lines):
    clean = []
    in_block = False
    for line in lines:
        cl, in_block = strip_noise(line, in_block)
        clean.append(cl)
    return clean


def block_end(clean, start):
    """The line (0-based) closing the `{...}` block opened at or after
    `start`, or None. A `;` before any `{` means a body-less item, and
    returns that line."""
    depth = 0
    opened = False
    for k in range(start, len(clean)):
        for ch in clean[k]:
            if ch == "{":
                depth += 1
                opened = True
            elif ch == "}":
                depth -= 1
                if opened and depth == 0:
                    return k
            elif ch == ";" and not opened:
                return k
    return None


def test_ranges(path, clean):
    """Line ranges (0-based, inclusive) of test-only code in a file: an
    inline `mod tests { ... }`, or the whole of a `tests.rs` module file."""
    if path.name == "tests.rs":
        return [(0, len(clean))]
    ranges = []
    i = 0
    while i < len(clean):
        if TEST_MOD_RE.search(clean[i]):
            end = block_end(clean, i)
            if end is not None:
                ranges.append((i, end))
                i = end + 1
                continue
        i += 1
    return ranges


def in_ranges(line0, ranges):
    return any(s <= line0 <= e for s, e in ranges)


def report(name, violations, advice):
    """Print the result the same way for every lint; return the exit code."""
    import sys

    if not violations:
        print(f"{name}: clean")
        return 0
    print(f"{name}: {len(violations)} violation(s):", file=sys.stderr)
    for v in violations:
        print(f"  {v}", file=sys.stderr)
    print(f"\n{advice}", file=sys.stderr)
    return 1
