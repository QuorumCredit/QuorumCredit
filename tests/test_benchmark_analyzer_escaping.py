import unittest
import html
import tempfile
import os
import json
from tools.benchmark_analyzer import BenchmarkAnalyzer

class TestBenchmarkAnalyzerEscaping(unittest.TestCase):
    def test_notes_and_op_name_escaped(self):
        malicious_note = "<script>alert('pwned')</script>"
        malicious_op = "op_<img src=x onerror=alert(1)>"
        data = {
            "metadata": {"soroban_sdk_version": "26.0.0"},
            "regression_thresholds": {"baseline_multiplier": 1.5, "regression_detection_threshold": 0.33},
            "operations": {
                malicious_op: {
                    "description": "test <script>",
                    "expected_complexity": "O(1)",
                    "measurements": [
                        {
                            "timestamp": "2026-11-24T12:00:00",
                            "notes": malicious_note,
                            "size_variants": [
                                {"size": 1, "cpu_instructions": 100, "memory_bytes": 50}
                            ]
                        }
                    ]
                }
            }
        }
        with tempfile.NamedTemporaryFile("w+", suffix=".json", delete=False) as f:
            json.dump(data, f)
            f.flush()
            temp_json = f.name
        
        with tempfile.NamedTemporaryFile("w+", suffix=".html", delete=False) as f:
            temp_html = f.name

        try:
            analyzer = BenchmarkAnalyzer(temp_json)
            analyzer.generate_html_report(temp_html)
            with open(temp_html, "r", encoding="utf-8") as f:
                content = f.read()
            self.assertNotIn("<script>alert('pwned')</script>", content)
            self.assertIn(html.escape(malicious_note), content)
            self.assertNotIn("<img src=x onerror=alert(1)>", content)
            self.assertIn(html.escape(malicious_op), content)
        finally:
            if os.path.exists(temp_json):
                os.remove(temp_json)
            if os.path.exists(temp_html):
                os.remove(temp_html)

if __name__ == "__main__":
    unittest.main()
