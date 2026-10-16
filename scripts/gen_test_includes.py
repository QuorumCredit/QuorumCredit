#!/usr/bin/env python3
"""Generate and verify how src/*_test.rs files are compiled into the crate.

Rule (documented in CONTRIBUTING.md, "Test file discovery"):

  Every src/*_test.rs and src/*_tests.rs file must be compiled by exactly one of:
    1. a `mod <name>;` declaration in src/lib.rs, or
    2. a `#[path]` entry in src/tests.rs, generated from INCLUDED below, or
    3. a line in scripts/test_orphans.txt (compiled by nothing yet; needs triage).

A file in both 1 and 2 would compile twice, so that is an error too.

Usage:
  python3 scripts/gen_test_includes.py           # check (default), exits 1 on drift
  python3 scripts/gen_test_includes.py --write   # regenerate the block in src/tests.rs
"""
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SRC = ROOT / "src"
LIB_RS = SRC / "lib.rs"
TESTS_RS = SRC / "tests.rs"
ORPHANS_TXT = ROOT / "scripts" / "test_orphans.txt"

BEGIN = "// BEGIN GENERATED TEST PATHS (scripts/gen_test_includes.py --write)"
END = "// END GENERATED TEST PATHS"

# Test files compiled through src/tests.rs. Add a file here, then run --write.
INCLUDED = [
    "credit_score_test.rs",
    "dynamic_rate_oracle_test.rs",
    "merkle_tree_test.rs",
    "relay_test.rs",
]

TRAILER = """
// API-drifted; excluded so `cargo test --lib` compiles (issue #942):
//   cross_chain_attestation_test, withdrawal_queue_test
// rbac_enforcement_test excluded from lib.rs for the same reason.
"""


def test_files() -> set[str]:
    names = {p.name for p in SRC.glob("*.rs")}
    return {n for n in names if n.endswith("_test.rs") or n.endswith("_tests.rs")}


def declared_files() -> set[str]:
    """File names pulled in by a `mod x;` or `#[path = "..."]` anywhere in src/."""
    found: set[str] = set()
    for path in SRC.rglob("*.rs"):
        text = path.read_text()
        found.update(f"{m}.rs" for m in re.findall(r"^\s*(?:pub\s+)?mod\s+(\w+)\s*;", text, re.M))
        found.update(Path(p).name for p in re.findall(r'#\[path\s*=\s*"([^"]+)"\]', text))
    return found


def lib_declared_files() -> set[str]:
    text = LIB_RS.read_text()
    return {f"{m}.rs" for m in re.findall(r"^\s*(?:pub\s+)?mod\s+(\w+)\s*;", text, re.M)}


def generated_block() -> str:
    lines = [BEGIN]
    for name in INCLUDED:
        lines.append(f'#[path = "{name}"]')
        lines.append(f"mod {name[:-3]};")
    lines.append(END)
    return "\n".join(lines) + "\n"


def orphans_listed() -> set[str]:
    return {
        line.strip()
        for line in ORPHANS_TXT.read_text().splitlines()
        if line.strip() and not line.startswith("#")
    }


def write() -> int:
    text = TESTS_RS.read_text()
    block = generated_block()
    if BEGIN in text and END in text:
        head, rest = text.split(BEGIN, 1)
        _, tail = rest.split(END + "\n", 1)
        new = head + block + tail
    else:
        new = block + TRAILER
    TESTS_RS.write_text(new)
    print(f"Wrote {TESTS_RS.relative_to(ROOT)}")
    return 0


def check() -> int:
    errors: list[str] = []

    text = TESTS_RS.read_text()
    if generated_block() not in text:
        errors.append("src/tests.rs generated block is out of date; run gen_test_includes.py --write")

    lib_files = lib_declared_files()
    for name in INCLUDED:
        if name in lib_files:
            errors.append(f"{name} is in INCLUDED but src/lib.rs also declares it (would compile twice)")
        if not (SRC / name).exists():
            errors.append(f"{name} is in INCLUDED but does not exist")

    declared = declared_files()
    orphans = orphans_listed()
    for name in sorted(test_files()):
        if name in declared or name in INCLUDED:
            continue
        if name not in orphans:
            errors.append(f"src/{name} is not compiled by lib.rs or tests.rs and is not in scripts/test_orphans.txt")

    for name in sorted(orphans):
        if not (SRC / name).exists():
            errors.append(f"scripts/test_orphans.txt lists {name}, which does not exist; remove it")
        elif name in declared:
            errors.append(f"scripts/test_orphans.txt lists {name}, which is now compiled; remove it")

    for err in errors:
        print(f"ERROR: {err}")
    if errors:
        return 1
    print(f"OK: {len(INCLUDED)} included via tests.rs, {len(orphans)} tracked orphan(s), no undeclared test files.")
    return 0


if __name__ == "__main__":
    sys.exit(write() if "--write" in sys.argv else check())
