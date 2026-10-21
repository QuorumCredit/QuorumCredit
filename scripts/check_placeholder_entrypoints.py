#!/usr/bin/env python3
"""Flag public contract entrypoints that are placeholders.

A `pub fn` in src/lib.rs is flagged when its body:
  * contains a TODO, or
  * is a single literal return (for example `Ok(0)`, `Ok(String::from_str(&env, ""))`, `None`).

Flagged entrypoints must appear in scripts/placeholder_entrypoints.allowlist with
an issue number. A new, unlisted placeholder fails the check.

Usage:
  python3 scripts/check_placeholder_entrypoints.py          # check (exit 1 on new placeholders)
  python3 scripts/check_placeholder_entrypoints.py --list   # print all flagged entrypoints
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LIB = ROOT / "src" / "lib.rs"
ALLOWLIST = ROOT / "scripts" / "placeholder_entrypoints.allowlist"

# A body that is one literal expression, with an optional trailing semicolon.
LITERAL_RETURN = re.compile(
    r"""^(
        Ok\(\s*(-?\d+|None|true|false|String::from_str\(\s*&\w+\s*,\s*""\s*\)|)\s*\)  # Ok(literal)
      | -?\d+
      | None
      | true
      | false
      | ""
    )\s*;?$""",
    re.VERBOSE,
)


def strip_comments(text: str) -> str:
    text = re.sub(r"/\*.*?\*/", "", text, flags=re.S)
    return re.sub(r"//[^\n]*", "", text)


def function_bodies(source: str):
    """Yield (name, raw_body) for each `pub fn` with a brace-delimited body."""
    for m in re.finditer(r"pub fn (\w+)\s*\(", source):
        name = m.group(1)
        brace = source.find("{", m.end())
        if brace == -1:
            continue
        depth = 0
        for i in range(brace, len(source)):
            if source[i] == "{":
                depth += 1
            elif source[i] == "}":
                depth -= 1
                if depth == 0:
                    yield name, source[brace + 1 : i]
                    break


def is_placeholder(raw_body: str) -> bool:
    if "TODO" in raw_body:
        return True
    code = " ".join(strip_comments(raw_body).split())
    return bool(LITERAL_RETURN.match(code))


def flagged() -> list[str]:
    source = LIB.read_text(encoding="utf-8")
    seen = []
    for name, body in function_bodies(source):
        if is_placeholder(body) and name not in seen:
            seen.append(name)
    return seen


def load_allowlist() -> dict[str, str]:
    allowed = {}
    if not ALLOWLIST.exists():
        return allowed
    for line in ALLOWLIST.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        name, _, issue = line.partition(" ")
        allowed[name] = issue.strip()
    return allowed


def main() -> int:
    names = flagged()
    if "--list" in sys.argv:
        for n in names:
            print(n)
        return 0

    allowed = load_allowlist()
    new = [n for n in names if n not in allowed]
    for name, issue in allowed.items():
        if not re.fullmatch(r"#\d+", issue):
            print(f"allowlist entry {name} has no issue number (got {issue!r})")
            return 1
    if new:
        print("Placeholder public entrypoints without an allowlisted issue:")
        for n in new:
            print(f"  - {n}")
        print(f"Implement them, or add `<name> #<issue>` to {ALLOWLIST.relative_to(ROOT)}.")
        return 1
    print(f"OK: {len(names)} placeholder entrypoint(s), all allowlisted.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
