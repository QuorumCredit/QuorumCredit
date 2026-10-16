#!/usr/bin/env python3
"""Enforce a size budget on the release WASM contract.

Usage:
  python3 scripts/wasm_size_budget.py <path/to/contract.wasm> [--budget wasm-size-budget.json]

The budget file holds the byte size of the last released contract
(`baseline_bytes`) and the allowed growth over it (`max_growth_percent`).
The job fails when the built WASM is more than `max_growth_percent` larger
than the baseline. When `baseline_bytes` is null (no release has been
recorded yet) the size is reported and the check passes.

After a release, set `baseline_bytes` to the size printed by the job for the
released commit, so the next budget is measured from that release.
"""
import argparse
import json
import os
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("wasm", type=Path)
    parser.add_argument("--budget", type=Path, default=ROOT / "wasm-size-budget.json")
    args = parser.parse_args()

    if not args.wasm.is_file():
        print(f"ERROR: {args.wasm} not found; build the release WASM first")
        return 1

    size = args.wasm.stat().st_size
    budget = json.loads(args.budget.read_text())
    baseline = budget["baseline_bytes"]
    max_pct = budget["max_growth_percent"]

    lines = [f"WASM size: {size} bytes"]
    status = 0

    if baseline is None:
        lines.append("No baseline recorded in wasm-size-budget.json; size reported, budget not enforced.")
        print(f"::warning::No WASM size baseline recorded; current size is {size} bytes")
    else:
        limit = baseline * (100 + max_pct) // 100
        growth = (size - baseline) * 100 / baseline
        lines.append(
            f"Baseline: {baseline} bytes, limit: {limit} bytes "
            f"(+{max_pct}%), change: {growth:+.2f}%"
        )
        if size > limit:
            lines.append(f"ERROR: WASM grew beyond the {max_pct}% budget over the last release")
            status = 1
        else:
            lines.append("OK: within budget")

    report = "\n".join(lines)
    print(report)

    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as fh:
            fh.write("```\n" + report + "\n```\n")

    return status


if __name__ == "__main__":
    sys.exit(main())
