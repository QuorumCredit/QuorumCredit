#!/usr/bin/env python3
"""Unit and integration tests for scripts/state_anomaly_monitor.py."""

from __future__ import annotations

import json
import os
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, os.path.abspath(os.path.join(os.path.dirname(__file__), "..")))

from scripts.state_anomaly_monitor import (
    StateSnapshot,
    detect_anomalies,
    load_history,
    save_history,
    generate_daily_report,
    send_alert,
)
from scripts.fixtures.vouch_graph import (
    VouchEdge,
    VouchGraph,
    generate_legitimate_vouch_graph,
    generate_sybil_vouch_graph,
    generate_sybil_attack_scenario,
    SAMPLE_STATE_HISTORY,
    SAMPLE_NORMAL_SNAPSHOT,
    SAMPLE_LEGITIMATE_GRAPH,
    SAMPLE_SYBIL_ATTACK_GRAPH,
)
from scripts import sybil_simulation


class TestStateAnomalyMonitor(unittest.TestCase):
    def setUp(self):
        self.default_thresholds = {
            "max_default_increase_per_poll": 5,
            "max_loan_count_increase_per_poll": 1000,
            "interest_rate_bps_min": 0,
            "interest_rate_bps_max": 5000,
            "storage_usage_warn_pct": 80,
            "max_gas_growth_pct": 25,
            "max_vouch_drop_pct": 30,
        }

    def test_normal_state_no_anomalies(self):
        """Standard healthy state against history produces zero anomalies."""
        snapshot = StateSnapshot(**SAMPLE_NORMAL_SNAPSHOT)
        anomalies = detect_anomalies(snapshot, SAMPLE_STATE_HISTORY, self.default_thresholds)
        self.assertEqual(anomalies, [])

    def test_empty_history_no_anomalies(self):
        """Initial run with empty history establishes baseline without false alarms."""
        snapshot = StateSnapshot(**SAMPLE_NORMAL_SNAPSHOT)
        anomalies = detect_anomalies(snapshot, [], self.default_thresholds)
        self.assertEqual(anomalies, [])

    def test_default_surge_detected(self):
        """A spike in defaulted loans above threshold is flagged."""
        snapshot = StateSnapshot(
            captured_at="2026-08-30T02:00:00Z",
            total_loans=60,
            active_loans=25,
            defaulted_loans=10,  # Jump of +9 from previous 1 (threshold is 5)
            repaid_loans=25,
            average_interest_rate_bps=520.0,
            active_vouches=80,
            total_staked=80_000_000_000,
            storage_entries=160,
            storage_limit=1000,
            avg_gas_per_tx=183_000.0,
        )
        anomalies = detect_anomalies(snapshot, SAMPLE_STATE_HISTORY, self.default_thresholds)
        self.assertEqual(len(anomalies), 1)
        self.assertIn("defaulted_loans jumped by 9", anomalies[0])

    def test_loan_growth_spike_detected(self):
        """A sudden surge in total_loans above threshold is flagged as possible spam."""
        snapshot = StateSnapshot(
            captured_at="2026-08-30T02:00:00Z",
            total_loans=1200,  # Jump of +1148 from previous 52 (threshold is 1000)
            active_loans=1100,
            defaulted_loans=1,
            repaid_loans=99,
            average_interest_rate_bps=520.0,
            active_vouches=80,
            total_staked=80_000_000_000,
            storage_entries=160,
            storage_limit=1000,
            avg_gas_per_tx=183_000.0,
        )
        anomalies = detect_anomalies(snapshot, SAMPLE_STATE_HISTORY, self.default_thresholds)
        self.assertEqual(len(anomalies), 1)
        self.assertIn("total_loans grew by 1148", anomalies[0])

    def test_interest_rate_out_of_bounds_detected(self):
        """Interest rate exceeding bounds is flagged."""
        snapshot = StateSnapshot(
            captured_at="2026-08-30T02:00:00Z",
            total_loans=53,
            active_loans=31,
            defaulted_loans=1,
            repaid_loans=21,
            average_interest_rate_bps=6500.0,  # Max is 5000 bps
            active_vouches=80,
            total_staked=80_000_000_000,
            storage_entries=160,
            storage_limit=1000,
            avg_gas_per_tx=183_000.0,
        )
        anomalies = detect_anomalies(snapshot, SAMPLE_STATE_HISTORY, self.default_thresholds)
        self.assertEqual(len(anomalies), 1)
        self.assertIn("average_interest_rate_bps 6500.0 is outside", anomalies[0])

    def test_storage_usage_warning_detected(self):
        """Storage utilization crossing threshold is flagged."""
        snapshot = StateSnapshot(
            captured_at="2026-08-30T02:00:00Z",
            total_loans=53,
            active_loans=31,
            defaulted_loans=1,
            repaid_loans=21,
            average_interest_rate_bps=515.0,
            active_vouches=80,
            total_staked=80_000_000_000,
            storage_entries=850,  # 85% of 1000 (threshold is 80%)
            storage_limit=1000,
            avg_gas_per_tx=183_000.0,
        )
        anomalies = detect_anomalies(snapshot, SAMPLE_STATE_HISTORY, self.default_thresholds)
        self.assertEqual(len(anomalies), 1)
        self.assertIn("storage usage at 85.0% of limit", anomalies[0])

    def test_gas_cost_spike_detected(self):
        """Gas consumption jump exceeding threshold percentage is flagged."""
        snapshot = StateSnapshot(
            captured_at="2026-08-30T02:00:00Z",
            total_loans=53,
            active_loans=31,
            defaulted_loans=1,
            repaid_loans=21,
            average_interest_rate_bps=515.0,
            active_vouches=80,
            total_staked=80_000_000_000,
            storage_entries=160,
            storage_limit=1000,
            avg_gas_per_tx=300_000.0,  # ~65% jump from 182,000 (threshold is 25%)
        )
        anomalies = detect_anomalies(snapshot, SAMPLE_STATE_HISTORY, self.default_thresholds)
        self.assertEqual(len(anomalies), 1)
        self.assertIn("avg_gas_per_tx increased", anomalies[0])

    def test_active_vouches_collapse_detected(self):
        """Sharp drop in active vouches (e.g. mass unstaking) is flagged."""
        snapshot = StateSnapshot(
            captured_at="2026-08-30T02:00:00Z",
            total_loans=53,
            active_loans=31,
            defaulted_loans=1,
            repaid_loans=21,
            average_interest_rate_bps=515.0,
            active_vouches=40,  # Drop from 78 to 40 (~48.7% drop, threshold is 30%)
            total_staked=40_000_000_000,
            storage_entries=160,
            storage_limit=1000,
            avg_gas_per_tx=183_000.0,
        )
        anomalies = detect_anomalies(snapshot, SAMPLE_STATE_HISTORY, self.default_thresholds)
        self.assertEqual(len(anomalies), 1)
        self.assertIn("active_vouches dropped", anomalies[0])

    def test_daily_report_generation(self):
        """Daily markdown report generates formatted summary."""
        snapshot = StateSnapshot(**SAMPLE_NORMAL_SNAPSHOT)
        report = generate_daily_report(snapshot, SAMPLE_STATE_HISTORY)
        self.assertIn("# QuorumCredit Daily State Report", report)
        self.assertIn("- Total loans: 53", report)
        self.assertIn("- Active vouches: 80", report)

    def test_history_persistence_and_trimming(self):
        """History saves and trims to max_entries properly."""
        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as tf:
            path = tf.name

        try:
            sample = [{"captured_at": f"2026-08-30T{i:02d}:00:00Z", "total_loans": i} for i in range(10)]
            save_history(path, sample, max_entries=5)
            loaded = load_history(path)
            self.assertEqual(len(loaded), 5)
            self.assertEqual(loaded[-1]["total_loans"], 9)
            self.assertEqual(loaded[0]["total_loans"], 5)
        finally:
            if os.path.exists(path):
                os.remove(path)

    # ── Sybil Simulation & Shared Fixture Integration Tests ────────────────────

    def test_state_anomaly_monitor_flags_sybil_loan_spam_attack(self):
        """Prove state_anomaly_monitor flags a loan spam attack generated by sybil_simulation scenario."""
        baseline_hist, sybil_snap_dict = generate_sybil_attack_scenario("loan_spam_attack")
        snapshot = StateSnapshot(**sybil_snap_dict)
        anomalies = detect_anomalies(snapshot, baseline_hist, self.default_thresholds)
        self.assertTrue(len(anomalies) > 0)
        self.assertTrue(any("total_loans grew by" in a and "possible spam or exploit" in a for a in anomalies))

    def test_state_anomaly_monitor_flags_sybil_mass_default_attack(self):
        """Prove state_anomaly_monitor flags a mass default attack generated by sybil_simulation scenario."""
        baseline_hist, sybil_snap_dict = generate_sybil_attack_scenario("mass_default_attack")
        snapshot = StateSnapshot(**sybil_snap_dict)
        anomalies = detect_anomalies(snapshot, baseline_hist, self.default_thresholds)
        self.assertTrue(len(anomalies) > 0)
        self.assertTrue(any("defaulted_loans jumped by" in a for a in anomalies))

    def test_state_anomaly_monitor_flags_sybil_vouch_collapse_attack(self):
        """Prove state_anomaly_monitor flags a mass vouch collapse generated by sybil_simulation scenario."""
        baseline_hist, sybil_snap_dict = generate_sybil_attack_scenario("vouch_collapse_attack")
        snapshot = StateSnapshot(**sybil_snap_dict)
        anomalies = detect_anomalies(snapshot, baseline_hist, self.default_thresholds)
        self.assertTrue(len(anomalies) > 0)
        self.assertTrue(any("active_vouches dropped" in a for a in anomalies))

    def test_state_anomaly_monitor_flags_sybil_storage_exhaustion_attack(self):
        """Prove state_anomaly_monitor flags a storage exhaustion attack from sybil simulation."""
        baseline_hist, sybil_snap_dict = generate_sybil_attack_scenario("storage_exhaustion_attack")
        snapshot = StateSnapshot(**sybil_snap_dict)
        anomalies = detect_anomalies(snapshot, baseline_hist, self.default_thresholds)
        self.assertTrue(len(anomalies) > 0)
        self.assertTrue(any("storage usage at" in a for a in anomalies))

    def test_state_anomaly_monitor_flags_sybil_gas_spike_attack(self):
        """Prove state_anomaly_monitor flags a gas spike attack from sybil simulation."""
        baseline_hist, sybil_snap_dict = generate_sybil_attack_scenario("gas_spike_attack")
        snapshot = StateSnapshot(**sybil_snap_dict)
        anomalies = detect_anomalies(snapshot, baseline_hist, self.default_thresholds)
        self.assertTrue(len(anomalies) > 0)
        self.assertTrue(any("avg_gas_per_tx increased" in a for a in anomalies))

    def test_legitimate_vouch_graph_produces_no_anomalies(self):
        """A normal vouch graph generated from shared fixtures produces no false positive anomalies."""
        legit_graph = generate_legitimate_vouch_graph()
        snapshot_dict = legit_graph.to_state_snapshot_dict(
            captured_at="2026-08-30T02:00:00Z",
            total_loans=53,
            active_loans=31,
            defaulted_loans=1,
            repaid_loans=21,
            average_interest_rate_bps=515.0,
            storage_entries=160,
            storage_limit=1000,
            avg_gas_per_tx=183_000.0,
        )
        snapshot = StateSnapshot(**snapshot_dict)
        anomalies = detect_anomalies(snapshot, SAMPLE_STATE_HISTORY, self.default_thresholds)
        self.assertEqual(anomalies, [])


if __name__ == "__main__":
    unittest.main()
