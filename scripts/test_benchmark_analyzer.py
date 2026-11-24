import json
import tempfile
from pathlib import Path

from tools.benchmark_analyzer import BenchmarkAnalyzer


def make_analyzer():
    with tempfile.TemporaryDirectory() as tmpdir:
        path = Path(tmpdir) / "benchmarks.json"
        path.write_text(json.dumps({"operations": {}, "regression_thresholds": {}}))
        yield BenchmarkAnalyzer(str(path))


def test_fit_complexity_class_skips_zero_cost():
    analyzer = next(make_analyzer())

    complexity, goodness = analyzer.fit_complexity_class([1, 2, 4], [0, 20, 40])

    assert complexity in {"O(1)", "O(n)", "O(n log n)", "O(n²)", "Unknown"}
    assert goodness >= 0.0
    assert analyzer.skipped_non_positive_values == [
        {"index": 0, "size": 1, "cost": 0}
    ]


def test_fit_complexity_class_skips_zero_size_and_cost():
    analyzer = next(make_analyzer())

    complexity, _ = analyzer.fit_complexity_class([0, 1, 2], [10, 0, 20])

    assert complexity in {"O(1)", "O(n)", "O(n log n)", "O(n²)", "Unknown"}
    assert analyzer.skipped_non_positive_values == [
        {"index": 0, "size": 0, "cost": 10},
        {"index": 1, "size": 1, "cost": 0},
    ]


def test_fit_complexity_class_reports_unknown_when_too_few_valid_pairs():
    analyzer = next(make_analyzer())

    assert analyzer.fit_complexity_class([0, 1], [10, 0]) == ("Unknown", 0.0)
