#!/usr/bin/env python3
"""Fail if any `pub fn` name is defined more than once inside a `#[contractimpl]` block.

Duplicate method names in one impl block are a compile error, and the copies can
silently disagree on behavior (Issue #1898). Run from the repository root:

    python3 scripts/check_contract_duplicate_fns.py [path/to/lib.rs]
"""

import collections
import re
import sys

DEFAULT_PATH = "src/lib.rs"
FN_RE = re.compile(r"^    pub fn (\w+)")


def contractimpl_blocks(lines):
    """Yield (start, end) line indexes of each #[contractimpl] block body."""
    for start, line in enumerate(lines):
        if not line.startswith("#[contractimpl]"):
            continue
        end = next(
            (i for i in range(start + 1, len(lines)) if lines[i] == "}"),
            len(lines) - 1,
        )
        yield start, end


def find_duplicates(lines):
    """Return {name: [line numbers]} for every name defined more than once."""
    duplicates = {}
    for start, end in contractimpl_blocks(lines):
        seen = collections.defaultdict(list)
        for i in range(start, end + 1):
            match = FN_RE.match(lines[i])
            if match:
                seen[match.group(1)].append(i + 1)
        for name, locations in seen.items():
            if len(locations) > 1:
                duplicates[name] = locations
    return duplicates


def main(argv):
    path = argv[1] if len(argv) > 1 else DEFAULT_PATH
    with open(path, encoding="utf-8") as f:
        lines = f.read().split("\n")

    duplicates = find_duplicates(lines)
    if not duplicates:
        print(f"OK: no duplicate pub fn names in {path}")
        return 0

    for name, locations in sorted(duplicates.items()):
        joined = ", ".join(str(n) for n in locations)
        print(f"{path}: duplicate pub fn `{name}` at lines {joined}")
    print(f"FAIL: {len(duplicates)} duplicated pub fn name(s) in {path}")
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
