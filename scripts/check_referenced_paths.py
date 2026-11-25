#!/usr/bin/env python3
"""Fail when a workflow or package script references a repository path that is missing.

Stdlib only; run from repo-guards.yml so broken references are caught on every PR.
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SCRIPT_REF = re.compile(
    r"(?:\./)?((?:[A-Za-z0-9_.-]+/)*(?:scripts|tools)/[A-Za-z0-9_./-]+)"
)
SKIP_REF = re.compile(r"[$<>*]|\$\{")


def workflow_refs():
    refs = {}
    workflows = sorted((ROOT / ".github" / "workflows").glob("*.yml"))
    for workflow in workflows:
        for match in SCRIPT_REF.finditer(workflow.read_text(encoding="utf-8")):
            path = match.group(1).rstrip("/.")
            if SKIP_REF.search(path) or path.startswith("GITHUB_"):
                continue
            refs.setdefault(path, set()).add(str(workflow.relative_to(ROOT)))
    return refs


def package_refs():
    refs = {}
    packages = [ROOT / "package.json"] + sorted(ROOT.glob("*/package.json"))
    for package in packages:
        if not package.is_file():
            continue
        scripts = json.loads(package.read_text(encoding="utf-8")).get("scripts", {})
        for name, command in scripts.items():
            for match in SCRIPT_REF.finditer(command):
                raw = match.group(1).rstrip("/.")
                if SKIP_REF.search(raw):
                    continue
                resolved = (package.parent / raw).resolve()
                try:
                    path = resolved.relative_to(ROOT).as_posix()
                except ValueError:
                    continue
                refs.setdefault(path, set()).add(
                    "{}:{}".format(package.relative_to(ROOT), name)
                )
    return refs


def main():
    refs = workflow_refs()
    for path, sources in package_refs().items():
        refs.setdefault(path, set()).update(sources)

    missing = []
    for path, sources in sorted(refs.items()):
        if not (ROOT / path).exists():
            missing.append((path, sorted(sources)))

    if missing:
        for path, sources in missing:
            print(
                "::error file={}::Referenced path does not exist: {}".format(
                    sources[0], path
                )
            )
        print(
            "Missing referenced paths: {}".format(", ".join(p for p, _ in missing)),
            file=sys.stderr,
        )
        return 1

    print("OK: all {} referenced script/tool paths exist".format(len(refs)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
