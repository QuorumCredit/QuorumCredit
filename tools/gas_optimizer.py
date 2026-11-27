#!/usr/bin/env python3
"""Small stdlib-only gas profile helpers used by gas-benchmarks.yml.

Consumes artifacts produced by tools/benchmark_analyzer.py and benchmark
output text; all commands write deterministic JSON/Markdown artifacts.
"""

import argparse
import json
import os
import re
import sys
from datetime import datetime, timezone


def read_text(path, default=""):
    try:
        with open(path, "r", encoding="utf-8") as handle:
            return handle.read()
    except FileNotFoundError:
        return default


def load_json(path, default):
    try:
        with open(path, "r", encoding="utf-8") as handle:
            return json.load(handle)
    except (FileNotFoundError, ValueError):
        return default


def write_text(path, text):
    directory = os.path.dirname(path)
    if directory:
        os.makedirs(directory, exist_ok=True)
    with open(path, "w", encoding="utf-8") as handle:
        handle.write(text)


def write_json(path, data):
    write_text(path, json.dumps(data, indent=2, sort_keys=True) + "\n")


def timestamp():
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def profile(args):
    text = read_text(args.input)
    operations = {}

    for match in re.finditer(r"Fitted complexity:\s*(\S+)", text):
        name = match.group(1)
        entry = operations.setdefault(name, {"complexity": name, "samples": 0})
        entry["samples"] += 1

    entry = re.compile(
        r"(?:operation|op)[=:]\s*([\w:.\-]+).*?gas(?:_used)?[=:]\s*(\d+)",
        re.IGNORECASE,
    )
    for match in entry.finditer(text):
        name, gas = match.group(1), int(match.group(2))
        measurements = operations.setdefault(
            name, {"complexity": "Unknown", "samples": 0, "gas": []}
        )
        measurements.setdefault("gas", []).append(gas)
        measurements["samples"] += 1

    result = {
        "generated_at": timestamp(),
        "source": args.input,
        "operations": operations,
    }
    write_json(args.output, result)
    print("Profile written to {} ({} operations)".format(args.output, len(operations)))


def suggest(args):
    data = load_json(args.profile, {"operations": {}})
    operations = data.get("operations", {})
    lines = [
        "# Gas optimization suggestions",
        "",
        "Generated at {} from {}.".format(timestamp(), args.profile),
        "",
    ]
    if not operations:
        lines.append("_No benchmark measurements available._")
    for name, entry in sorted(operations.items()):
        complexity = entry.get("complexity", "Unknown")
        gas = entry.get("gas", [])
        note = (
            "Review loop bounds and storage access patterns."
            if complexity not in ("O(1)", "Unknown")
            else "No structural regression detected."
        )
        peak = max(gas) if gas else 0
        lines.extend(
            [
                "## {}".format(name),
                "",
                "- Fitted complexity: {}".format(complexity),
                "- Measurements: {}".format(entry.get("samples", 0)),
                "- Peak gas: {}".format(peak),
                "- Suggestion: {}".format(note),
                "",
            ]
        )
    write_text(args.output, "\n".join(lines))
    print("Suggestions written to {}".format(args.output))


def compare(args):
    profile_data = load_json(args.profile, {"operations": {}})
    baseline = load_json(args.baseline, {})
    current = profile_data.get("operations", {})
    previous = baseline.get("operations", baseline)

    lines = [
        "# Gas benchmark comparison",
        "",
        "| Operation | Complexity | Peak gas | Baseline | Delta (bps) |",
        "| --- | --- | ---: | ---: | ---: |",
    ]
    for name, entry in sorted(current.items()):
        gas = entry.get("gas", [])
        peak = max(gas) if gas else 0
        old = previous.get(name, {}) if isinstance(previous, dict) else {}
        old_gas = old.get("gas_consumed") or old.get("peak_gas") or old.get("gas") or 0
        if isinstance(old_gas, list):
            old_gas = max(old_gas) if old_gas else 0
        old_gas = float(old_gas)
        delta = int(round((peak - old_gas) / old_gas * 10000)) if old_gas else 0
        lines.append(
            "| {} | {} | {} | {} | {} |".format(
                name, entry.get("complexity", "Unknown"), peak, int(old_gas), delta
            )
        )
    write_text(args.output, "\n".join(lines) + "\n")
    print("Comparison written to {}".format(args.output))


def trend(args):
    profile_data = load_json(args.profile, {"operations": {}})
    history = load_json(args.history, [])
    if isinstance(history, dict):
        history = history.get("entries", [])
    entry = {
        "timestamp": timestamp(),
        "operation_count": len(profile_data.get("operations", {})),
    }
    history.append(entry)
    write_json(args.history, {"entries": history})
    print("Trend appended to {} ({} entries)".format(args.history, len(history)))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    p_profile = sub.add_parser("profile", help="parse benchmark output into a profile")
    p_profile.add_argument("--input", required=True)
    p_profile.add_argument("--output", required=True)
    p_profile.set_defaults(handler=profile)

    p_suggest = sub.add_parser("suggest", help="write optimization suggestions")
    p_suggest.add_argument("--profile", required=True)
    p_suggest.add_argument("--output", required=True)
    p_suggest.set_defaults(handler=suggest)

    p_compare = sub.add_parser("compare", help="compare a profile to the baseline")
    p_compare.add_argument("--profile", required=True)
    p_compare.add_argument("--baseline", required=True)
    p_compare.add_argument("--output", required=True)
    p_compare.set_defaults(handler=compare)

    p_trend = sub.add_parser("trend", help="append a point to the trend history")
    p_trend.add_argument("--profile", required=True)
    p_trend.add_argument("--history", required=True)
    p_trend.set_defaults(handler=trend)

    args = parser.parse_args(argv)
    args.handler(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
