#!/usr/bin/env python3
"""
Gas Benchmark Analyzer
Analyzes parameterized gas budget measurements to detect complexity regressions.
Generates historical reports and complexity-class fitting.
"""

import json
import sys
import os
from datetime import datetime
from typing import List, Dict, Tuple, Optional
import math

class BenchmarkAnalyzer:
    def __init__(self, benchmarks_json_path: str):
        with open(benchmarks_json_path, 'r') as f:
            self.data = json.load(f)
        self.operations = self.data.get('operations', {})
        self.thresholds = self.data.get('regression_thresholds', {})

    def fit_complexity_class(self, sizes: List[int], costs: List[int]) -> Tuple[str, float]:
        """
        Fit empirical complexity class to measurements.
        Returns: (complexity_class: "O(1)|O(n)|O(n log n)|O(n²)", goodness_of_fit: float)
        """
        if len(sizes) < 2:
            return ("Unknown", 0.0)

        # Calculate cost-to-size ratios and slopes
        log_sizes = [math.log(s) for s in sizes]
        log_costs = [math.log(c) for c in costs]

        # Test different complexity models via linear regression on log-log scale
        models = {
            "O(1)": self._test_constant(costs),
            "O(n)": self._test_linear(sizes, costs),
            "O(n log n)": self._test_nlogn(sizes, costs),
            "O(n²)": self._test_quadratic(sizes, costs),
        }

        # Pick best fit by R² value
        best_fit = max(models.items(), key=lambda x: x[1])
        return best_fit

    def _test_constant(self, costs: List[int]) -> float:
        """R² for constant model: cost = c"""
        mean_cost = sum(costs) / len(costs)
        ss_tot = sum((c - mean_cost) ** 2 for c in costs)
        ss_res = sum((c - mean_cost) ** 2 for c in costs)
        if ss_tot == 0:
            return 1.0
        return 1.0 - (ss_res / ss_tot)

    def _test_linear(self, sizes: List[int], costs: List[int]) -> float:
        """R² for linear model: cost = a*n + b"""
        n = len(sizes)
        mean_x = sum(sizes) / n
        mean_y = sum(costs) / n

        numerator = sum((sizes[i] - mean_x) * (costs[i] - mean_y) for i in range(n))
        denominator_x = sum((sizes[i] - mean_x) ** 2 for i in range(n))

        if denominator_x == 0:
            return 0.0

        slope = numerator / denominator_x
        intercept = mean_y - slope * mean_x

        ss_tot = sum((costs[i] - mean_y) ** 2 for i in range(n))
        ss_res = sum((costs[i] - (slope * sizes[i] + intercept)) ** 2 for i in range(n))

        if ss_tot == 0:
            return 1.0
        return 1.0 - (ss_res / ss_tot)

    def _test_nlogn(self, sizes: List[int], costs: List[int]) -> float:
        """R² for n log n model: cost = a*n*log(n) + b"""
        n = len(sizes)
        x_vals = [s * math.log(s) if s > 0 else 0 for s in sizes]

        mean_x = sum(x_vals) / n
        mean_y = sum(costs) / n

        numerator = sum((x_vals[i] - mean_x) * (costs[i] - mean_y) for i in range(n))
        denominator_x = sum((x_vals[i] - mean_x) ** 2 for i in range(n))

        if denominator_x == 0:
            return 0.0

        slope = numerator / denominator_x
        intercept = mean_y - slope * mean_x

        ss_tot = sum((costs[i] - mean_y) ** 2 for i in range(n))
        ss_res = sum((costs[i] - (slope * x_vals[i] + intercept)) ** 2 for i in range(n))

        if ss_tot == 0:
            return 1.0
        return 1.0 - (ss_res / ss_tot)

    def _test_quadratic(self, sizes: List[int], costs: List[int]) -> float:
        """R² for quadratic model: cost = a*n² + b*n + c"""
        n = len(sizes)
        x_vals = [s * s for s in sizes]

        mean_x = sum(x_vals) / n
        mean_y = sum(costs) / n

        numerator = sum((x_vals[i] - mean_x) * (costs[i] - mean_y) for i in range(n))
        denominator_x = sum((x_vals[i] - mean_x) ** 2 for i in range(n))

        if denominator_x == 0:
            return 0.0

        slope = numerator / denominator_x
        intercept = mean_y - slope * mean_x

        ss_tot = sum((costs[i] - mean_y) ** 2 for i in range(n))
        ss_res = sum((costs[i] - (slope * x_vals[i] + intercept)) ** 2 for i in range(n))

        if ss_tot == 0:
            return 1.0
        return 1.0 - (ss_res / ss_tot)

    def detect_regressions(self) -> Dict[str, List[Dict]]:
        """
        Compare latest measurements against previous baseline.
        Returns dict of {operation: [regression findings]}
        """
        regressions = {}

        for op_name, op_data in self.operations.items():
            measurements = op_data.get('measurements', [])
            if len(measurements) < 2:
                continue

            latest = measurements[-1]
            baseline = measurements[-2]

            # Compare cost growth across size variants
            findings = []
            baseline_map = {v['size']: v for v in baseline['size_variants']}

            for variant in latest['size_variants']:
                size = variant['size']
                if size not in baseline_map:
                    continue

                baseline_variant = baseline_map[size]
                baseline_cpu = baseline_variant['cpu_instructions']
                latest_cpu = variant['cpu_instructions']

                regression_threshold = self.thresholds.get('regression_detection_threshold', 0.33)
                ratio = (latest_cpu - baseline_cpu) / baseline_cpu if baseline_cpu > 0 else 0

                if ratio > regression_threshold:
                    findings.append({
                        'size': size,
                        'baseline_cpu': baseline_cpu,
                        'latest_cpu': latest_cpu,
                        'increase_pct': ratio * 100,
                        'severity': 'CRITICAL' if ratio > 0.5 else 'WARNING'
                    })

            if findings:
                regressions[op_name] = findings

        return regressions

    def generate_html_report(self, output_path: str) -> None:
        """Generate comprehensive HTML report of benchmarks and complexity analysis."""
        html = """<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Gas Budget Complexity Analysis Report</title>
    <style>
        * { margin: 0; padding: 0; box-sizing: border-box; }
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
            background: #f5f5f5;
            color: #333;
            line-height: 1.6;
        }
        @media (prefers-color-scheme: dark) {
            body { background: #1e1e1e; color: #e0e0e0; }
            .container { background: #2d2d2d; }
            .operation { border-color: #444; }
            .measurement-table { border-color: #444; }
            .measurement-table th, .measurement-table td { border-color: #444; }
            .regression { background: rgba(255, 0, 0, 0.1); }
            .complexity-badge { background: #4a4a4a; color: #fff; }
        }
        .container {
            max-width: 1200px;
            margin: 0 auto;
            padding: 2rem;
            background: white;
            border-radius: 8px;
            box-shadow: 0 2px 8px rgba(0,0,0,0.1);
        }
        h1 {
            font-size: 2rem;
            margin-bottom: 0.5rem;
            color: #222;
        }
        @media (prefers-color-scheme: dark) { h1 { color: #e0e0e0; } }
        .metadata {
            display: flex;
            gap: 2rem;
            margin-bottom: 2rem;
            padding: 1rem;
            background: #f9f9f9;
            border-radius: 4px;
            font-size: 0.95rem;
        }
        @media (prefers-color-scheme: dark) { .metadata { background: #3a3a3a; } }
        .metadata-item strong { margin-right: 0.5rem; }
        .operation {
            margin-bottom: 2rem;
            padding: 1.5rem;
            border: 1px solid #ddd;
            border-radius: 4px;
            background: #fafafa;
        }
        @media (prefers-color-scheme: dark) { .operation { background: #262626; } }
        .operation-header {
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 1rem;
        }
        .operation-title {
            font-size: 1.3rem;
            font-weight: 600;
        }
        .complexity-badge {
            padding: 0.4rem 0.8rem;
            border-radius: 20px;
            font-size: 0.9rem;
            font-weight: 500;
            background: #e0e7ff;
            color: #3730a3;
        }
        .complexity-badge.detected-quadratic {
            background: #fee2e2;
            color: #991b1b;
            font-weight: bold;
        }
        .operation-description {
            font-size: 0.95rem;
            color: #666;
            margin-bottom: 1rem;
            font-style: italic;
        }
        @media (prefers-color-scheme: dark) { .operation-description { color: #999; } }
        .measurement-table {
            width: 100%;
            border-collapse: collapse;
            margin-bottom: 1rem;
            font-size: 0.9rem;
        }
        .measurement-table th {
            background: #f0f0f0;
            padding: 0.75rem;
            text-align: left;
            font-weight: 600;
            border-bottom: 2px solid #ddd;
        }
        @media (prefers-color-scheme: dark) { .measurement-table th { background: #3a3a3a; border-color: #555; } }
        .measurement-table td {
            padding: 0.75rem;
            border-bottom: 1px solid #eee;
        }
        @media (prefers-color-scheme: dark) { .measurement-table td { border-color: #444; } }
        .measurement-table tr:nth-child(even) { background: #f8f8f8; }
        @media (prefers-color-scheme: dark) { .measurement-table tr:nth-child(even) { background: #2a2a2a; } }
        .regression { background: rgba(255, 100, 100, 0.15); }
        .warning-box {
            padding: 1rem;
            background: #fff3cd;
            border-left: 4px solid #ffc107;
            margin-bottom: 1rem;
            border-radius: 4px;
        }
        @media (prefers-color-scheme: dark) { .warning-box { background: rgba(255, 193, 7, 0.1); border-color: #ff9800; } }
        .stats-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
            gap: 1rem;
            margin-bottom: 2rem;
        }
        .stat-card {
            padding: 1rem;
            background: #f0f4ff;
            border-radius: 4px;
            border-left: 4px solid #3730a3;
        }
        @media (prefers-color-scheme: dark) { .stat-card { background: rgba(55, 48, 163, 0.1); } }
        .stat-label {
            font-size: 0.85rem;
            color: #666;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }
        @media (prefers-color-scheme: dark) { .stat-label { color: #999; } }
        .stat-value {
            font-size: 1.5rem;
            font-weight: 600;
            margin-top: 0.25rem;
        }
        code {
            background: #f5f5f5;
            padding: 0.2rem 0.4rem;
            border-radius: 2px;
            font-family: "Monaco", "Menlo", "Courier New", monospace;
            font-size: 0.9em;
        }
        @media (prefers-color-scheme: dark) { code { background: #3a3a3a; } }
        .note {
            margin-top: 1rem;
            padding: 0.75rem;
            background: #f9f9f9;
            border-left: 3px solid #ccc;
            font-size: 0.9rem;
        }
        @media (prefers-color-scheme: dark) { .note { background: #2a2a2a; border-color: #555; } }
    </style>
</head>
<body>
    <div class="container">
        <h1>Gas Budget Complexity Analysis Report</h1>
        <p style="color: #666; margin-bottom: 2rem;">
            Parameterized benchmarks across varying input sizes with complexity-class fitting.
        </p>
"""

        # Metadata section
        html += self._generate_metadata_section()

        # Regression summary
        regressions = self.detect_regressions()
        if regressions:
            html += self._generate_regression_section(regressions)

        # Per-operation analysis
        for op_name, op_data in self.operations.items():
            html += self._generate_operation_section(op_name, op_data)

        html += """
    </div>
</body>
</html>
"""

        with open(output_path, 'w') as f:
            f.write(html)
        print(f"Report written to {output_path}")

    def _generate_metadata_section(self) -> str:
        metadata = self.data.get('metadata', {})
        return f"""
        <div class="metadata">
            <div class="metadata-item"><strong>Generated:</strong> {datetime.now().isoformat()}</div>
            <div class="metadata-item"><strong>Soroban SDK:</strong> {metadata.get('soroban_sdk_version', 'N/A')}</div>
            <div class="metadata-item"><strong>Budget Multiplier:</strong> {self.thresholds.get('baseline_multiplier', 1.5)}×</div>
            <div class="metadata-item"><strong>Regression Threshold:</strong> {self.thresholds.get('regression_detection_threshold', 0.33)*100:.0f}%</div>
        </div>
"""

    def _generate_regression_section(self, regressions: Dict) -> str:
        html = '<div class="warning-box">'
        html += '<strong>⚠️ Detected Regressions:</strong><ul style="margin-top: 0.5rem;">'

        for op_name, findings in regressions.items():
            for finding in findings:
                html += f"""
                <li><strong>{op_name}</strong> (n={finding['size']}):
                    +{finding['increase_pct']:.1f}% ({finding['severity']})</li>
"""

        html += '</ul></div>'
        return html

    def _generate_operation_section(self, op_name: str, op_data: Dict) -> str:
        html = f'<div class="operation">'

        description = op_data.get('description', 'N/A')
        expected = op_data.get('expected_complexity', 'Unknown')
        measurements = op_data.get('measurements', [])

        if measurements:
            latest = measurements[-1]
            sizes = [v['size'] for v in latest['size_variants']]
            costs = [v['cpu_instructions'] for v in latest['size_variants']]

            fitted_complexity, goodness = self.fit_complexity_class(sizes, costs)

            warning = ""
            if fitted_complexity != expected and expected != "Unknown":
                warning = f"""
                <div class="warning-box" style="margin-bottom: 1rem;">
                    <strong>⚠️ Complexity Mismatch:</strong> Expected {expected} but detected {fitted_complexity}
                </div>
"""

            html += f"""
            <div class="operation-header">
                <div>
                    <div class="operation-title">{op_name}</div>
                    <div class="operation-description">{description}</div>
                </div>
                <div class="complexity-badge {'detected-quadratic' if 'n²' in fitted_complexity else ''}">
                    Detected: {fitted_complexity}
                </div>
            </div>
            {warning}
            <div class="stats-grid">
                <div class="stat-card">
                    <div class="stat-label">Expected Complexity</div>
                    <div class="stat-value">{expected}</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Goodness of Fit</div>
                    <div class="stat-value">{goodness:.3f}</div>
                </div>
                <div class="stat-card">
                    <div class="stat-label">Latest Measurement</div>
                    <div class="stat-value">{latest['timestamp'][:10]}</div>
                </div>
            </div>
            <table class="measurement-table">
                <thead>
                    <tr>
                        <th>Input Size (n)</th>
                        <th>CPU Instructions</th>
                        <th>Memory Bytes</th>
                        <th>Cost per Unit</th>
                    </tr>
                </thead>
                <tbody>
"""

            for variant in latest['size_variants']:
                size = variant['size']
                cpu = variant['cpu_instructions']
                mem = variant['memory_bytes']
                cost_per_unit = cpu // size if size > 0 else 0

                html += f"""
                    <tr>
                        <td>{size}</td>
                        <td>{cpu:,}</td>
                        <td>{mem:,}</td>
                        <td>{cost_per_unit:,}</td>
                    </tr>
"""

            html += """
                </tbody>
            </table>
"""

            if latest.get('notes'):
                html += f'<div class="note"><strong>Notes:</strong> {latest["notes"]}</div>'

        html += '</div>'
        return html


def main():
    if len(sys.argv) > 1:
        benchmarks_path = sys.argv[1]
    else:
        benchmarks_path = "docs/benchmarks.json"

    if not os.path.exists(benchmarks_path):
        print(f"Error: {benchmarks_path} not found", file=sys.stderr)
        sys.exit(1)

    analyzer = BenchmarkAnalyzer(benchmarks_path)

    output_path = "docs/benchmark_report.html"
    analyzer.generate_html_report(output_path)

    regressions = analyzer.detect_regressions()
    if regressions:
        print("\n⚠️  REGRESSIONS DETECTED:\n")
        for op_name, findings in regressions.items():
            print(f"  {op_name}:")
            for finding in findings:
                print(f"    @ n={finding['size']}: +{finding['increase_pct']:.1f}% ({finding['severity']})")
        sys.exit(1)
    else:
        print(f"\n✓ No regressions detected. Report: {output_path}")
        sys.exit(0)


if __name__ == "__main__":
    main()
