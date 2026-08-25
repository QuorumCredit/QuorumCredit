#!/usr/bin/env python3
"""Tests for consensus_monitor.py."""

import json
import sys
import unittest
from unittest.mock import MagicMock, patch

import consensus_monitor


class TestConsensusMonitor(unittest.TestCase):
    """Test suite for consensus monitoring."""

    def test_compare_results_detects_divergence(self):
        """Test that compare_results correctly flags state mismatches."""
        result1 = consensus_monitor.ValidatorResult(
            name="validator-1",
            endpoint="https://rpc1.example.org",
            ledger=12345,
            values={"get_config": "result_value_1"},
        )
        result2 = consensus_monitor.ValidatorResult(
            name="validator-2",
            endpoint="https://rpc2.example.org",
            ledger=12345,
            values={"get_config": "result_value_2"},
        )

        problems = consensus_monitor.compare_results([result1, result2], ledger_tolerance=2)

        self.assertEqual(len(problems), 1)
        self.assertIn("state mismatch on 'get_config'", problems[0])
        self.assertIn("validator-1", problems[0])
        self.assertIn("validator-2", problems[0])

    def test_compare_results_no_divergence(self):
        """Test that compare_results returns empty when validators agree."""
        result1 = consensus_monitor.ValidatorResult(
            name="validator-1",
            endpoint="https://rpc1.example.org",
            ledger=12345,
            values={"get_config": "same_value"},
        )
        result2 = consensus_monitor.ValidatorResult(
            name="validator-2",
            endpoint="https://rpc2.example.org",
            ledger=12345,
            values={"get_config": "same_value"},
        )

        problems = consensus_monitor.compare_results([result1, result2], ledger_tolerance=2)

        self.assertEqual(len(problems), 0)

    def test_compare_results_ledger_lag(self):
        """Test that compare_results detects ledger lag."""
        result1 = consensus_monitor.ValidatorResult(
            name="validator-1",
            endpoint="https://rpc1.example.org",
            ledger=12350,
            values={"get_config": "value"},
        )
        result2 = consensus_monitor.ValidatorResult(
            name="validator-2",
            endpoint="https://rpc2.example.org",
            ledger=12340,
            values={"get_config": "value"},
        )

        problems = consensus_monitor.compare_results([result1, result2], ledger_tolerance=2)

        self.assertEqual(len(problems), 1)
        self.assertIn("ledgers behind", problems[0])

    def test_compare_results_unreachable_validator(self):
        """Test that compare_results flags unreachable validators."""
        result1 = consensus_monitor.ValidatorResult(
            name="validator-1",
            endpoint="https://rpc1.example.org",
            ledger=12345,
            values={"get_config": "value"},
        )
        result2 = consensus_monitor.ValidatorResult(
            name="validator-2",
            endpoint="https://rpc2.example.org",
            error="Connection timeout",
        )

        problems = consensus_monitor.compare_results([result1, result2], ledger_tolerance=2)

        self.assertEqual(len(problems), 1)
        self.assertIn("unreachable", problems[0])
        self.assertIn("Connection timeout", problems[0])

    def test_compare_results_insufficient_healthy(self):
        """Test that compare_results doesn't compare if < 2 validators are healthy."""
        result1 = consensus_monitor.ValidatorResult(
            name="validator-1",
            endpoint="https://rpc1.example.org",
            ledger=12345,
            values={"get_config": "value1"},
        )
        result2 = consensus_monitor.ValidatorResult(
            name="validator-2",
            endpoint="https://rpc2.example.org",
            ledger=12345,
            values={"get_config": "value2"},
            error="Connection timeout",
        )

        problems = consensus_monitor.compare_results([result1, result2], ledger_tolerance=2)

        self.assertEqual(len(problems), 1)
        self.assertIn("unreachable", problems[0])

    @patch("consensus_monitor.rpc_call")
    def test_poll_validator_handles_error(self, mock_rpc_call):
        """Test that poll_validator captures RPC errors."""
        mock_rpc_call.side_effect = ValueError("RPC connection failed")

        result = consensus_monitor.poll_validator(
            name="validator-1",
            endpoint="https://rpc1.example.org",
            contract_id="C1234567890",
            checks=["get_config"],
            network_passphrase="Test SDF Future Network ; October 2022",
        )

        self.assertIsNotNone(result.error)
        self.assertIn("RPC connection failed", result.error)

    def test_validator_result_dataclass(self):
        """Test ValidatorResult dataclass."""
        result = consensus_monitor.ValidatorResult(
            name="test-validator",
            endpoint="https://rpc.example.org",
            ledger=12345,
            values={"get_config": "test_value"},
        )

        self.assertEqual(result.name, "test-validator")
        self.assertEqual(result.endpoint, "https://rpc.example.org")
        self.assertEqual(result.ledger, 12345)
        self.assertEqual(result.values["get_config"], "test_value")
        self.assertIsNone(result.error)


if __name__ == "__main__":
    unittest.main()
