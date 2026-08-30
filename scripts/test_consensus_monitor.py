#!/usr/bin/env python3
"""Unit and integration tests for scripts/consensus_monitor.py."""

from __future__ import annotations

import unittest
from unittest.mock import patch, MagicMock

import sys
import os
sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from scripts.consensus_monitor import (
    ValidatorResult,
    compare_results,
    send_alert,
    reconcile,
    DEFAULT_CHECKS,
)
from scripts.fixtures.vouch_graph import SAMPLE_VALIDATOR_STATES


class TestConsensusMonitor(unittest.TestCase):
    def test_unanimous_validators(self):
        """All validators agree on state and ledger height; no problems returned."""
        raw_states = SAMPLE_VALIDATOR_STATES["unanimous"]
        results = [
            ValidatorResult(
                name=v["name"],
                endpoint=v["endpoint"],
                ledger=v["ledger"],
                values=v["values"],
                error=v["error"],
            )
            for v in raw_states
        ]
        problems = compare_results(results, ledger_tolerance=2)
        self.assertEqual(problems, [])

    def test_divergent_validators(self):
        """State mismatch on get_slash_treasury is detected and reported."""
        raw_states = SAMPLE_VALIDATOR_STATES["divergent"]
        results = [
            ValidatorResult(
                name=v["name"],
                endpoint=v["endpoint"],
                ledger=v["ledger"],
                values=v["values"],
                error=v["error"],
            )
            for v in raw_states
        ]
        problems = compare_results(results, ledger_tolerance=2)
        self.assertEqual(len(problems), 1)
        self.assertIn("state mismatch on 'get_slash_treasury'", problems[0])

    def test_lagging_validators(self):
        """Validator lagging beyond ledger tolerance is flagged."""
        raw_states = SAMPLE_VALIDATOR_STATES["lagging"]
        results = [
            ValidatorResult(
                name=v["name"],
                endpoint=v["endpoint"],
                ledger=v["ledger"],
                values=v["values"],
                error=v["error"],
            )
            for v in raw_states
        ]
        problems = compare_results(results, ledger_tolerance=2)
        self.assertEqual(len(problems), 1)
        self.assertIn("5 ledgers behind", problems[0])

    def test_unreachable_validators(self):
        """Unreachable validator is reported as an error."""
        raw_states = SAMPLE_VALIDATOR_STATES["unreachable"]
        results = [
            ValidatorResult(
                name=v["name"],
                endpoint=v["endpoint"],
                ledger=v["ledger"],
                values=v["values"],
                error=v["error"],
            )
            for v in raw_states
        ]
        problems = compare_results(results, ledger_tolerance=2)
        self.assertEqual(len(problems), 1)
        self.assertIn("unreachable: Connection refused", problems[0])

    def test_insufficient_healthy_validators(self):
        """If fewer than 2 validators are healthy, state comparison is skipped."""
        results = [
            ValidatorResult(name="v1", endpoint="ep1", error="timeout"),
            ValidatorResult(name="v2", endpoint="ep2", error="connection refused"),
        ]
        problems = compare_results(results, ledger_tolerance=2)
        self.assertEqual(len(problems), 2)
        self.assertTrue(all("unreachable" in p for p in problems))

    def test_send_alert_dry_run(self):
        """In dry-run mode, no webhook HTTP request is dispatched."""
        with patch("urllib.request.urlopen") as mock_urlopen:
            config = {"alert_webhook": "https://hooks.example.com/alerts"}
            send_alert("test problem", config, dry_run=True)
            mock_urlopen.assert_not_called()

    @patch("scripts.consensus_monitor.poll_validator")
    def test_reconcile_recovers_after_retry(self, mock_poll):
        """Reconcile succeeds if transient issue resolves on retry."""
        validators = [
            {"name": "v1", "endpoint": "http://v1"},
            {"name": "v2", "endpoint": "http://v2"},
        ]
        # Attempt 1: divergent state; Attempt 2: matching state
        attempt1 = [
            ValidatorResult(name="v1", endpoint="http://v1", ledger=100, values={"check": 1}),
            ValidatorResult(name="v2", endpoint="http://v2", ledger=100, values={"check": 2}),
        ]
        attempt2 = [
            ValidatorResult(name="v1", endpoint="http://v1", ledger=100, values={"check": 1}),
            ValidatorResult(name="v2", endpoint="http://v2", ledger=100, values={"check": 1}),
        ]
        mock_poll.side_effect = [
            attempt1[0], attempt1[1],
            attempt2[0], attempt2[1],
        ]

        with patch("time.sleep"):
            results = reconcile(validators, "contract_123", ["check"], retries=2, delay=0.01)
            problems = compare_results(results, ledger_tolerance=2)
            self.assertEqual(problems, [])


if __name__ == "__main__":
    unittest.main()
