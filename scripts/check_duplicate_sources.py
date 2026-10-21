#!/usr/bin/env python3
"""Fail when a file under contracts/ has the same name as a file under src/.

Copies of root-module files that live elsewhere in the tree drift silently:
fixes land in one copy and never reach the other (see issue #1902). This guard
rejects new same-named copies. Existing copies that predate the guard are
listed in KNOWN_DUPLICATES and must be triaged and removed over time.
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Pre-existing unreferenced copies. Remove an entry once its copy is deleted.
KNOWN_DUPLICATES = {
    "contracts/loan/chaos_test.rs",
    "contracts/loan/concurrent_operations_test.rs",
    "contracts/slice/src/cache.rs",
}


def main() -> int:
    src_names = {p.name for p in (ROOT / "src").rglob("*.rs")}
    found = {
        p.relative_to(ROOT).as_posix()
        for p in (ROOT / "contracts").rglob("*.rs")
        if p.name in src_names
    }

    new = sorted(found - KNOWN_DUPLICATES)
    stale = sorted(KNOWN_DUPLICATES - found)

    for path in new:
        print(f"ERROR: {path} duplicates a file under src/ (not in KNOWN_DUPLICATES)")
    for path in stale:
        print(f"ERROR: {path} is listed in KNOWN_DUPLICATES but no longer duplicates src/; remove the entry")

    if new or stale:
        return 1
    print(f"OK: {len(found)} known duplicate(s), no new ones.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
