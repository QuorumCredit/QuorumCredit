"""Shared fixture data and models for QuorumCredit vouch graphs, stake distributions, and state snapshots.

This module provides common data structures and synthetic scenarios used across:
- `scripts/sybil_simulation.py`
- `scripts/consensus_monitor.py`
- `scripts/state_anomaly_monitor.py`
- and their corresponding test suites.
"""

from __future__ import annotations

import time
from dataclasses import dataclass, field, asdict
from typing import Any

# Constants
XLM_TO_STROOP = 10_000_000
BPS_DENOMINATOR = 10_000


@dataclass
class VouchEdge:
    voucher: str
    borrower: str
    stake_stroops: int
    stake_xlm: float
    created_at: int = field(default_factory=lambda: int(time.time()))
    is_sybil: bool = False

    @classmethod
    def from_xlm(cls, voucher: str, borrower: str, stake_xlm: float, is_sybil: bool = False) -> VouchEdge:
        return cls(
            voucher=voucher,
            borrower=borrower,
            stake_stroops=int(stake_xlm * XLM_TO_STROOP),
            stake_xlm=stake_xlm,
            is_sybil=is_sybil,
        )


@dataclass
class VouchNode:
    address: str
    is_borrower: bool = False
    is_voucher: bool = False
    total_stake_stroops: int = 0
    reputation_score: int = 0
    is_sybil: bool = False


@dataclass
class VouchGraph:
    nodes: dict[str, VouchNode] = field(default_factory=dict)
    edges: list[VouchEdge] = field(default_factory=list)

    def add_edge(self, edge: VouchEdge) -> None:
        self.edges.append(edge)
        # Register voucher node
        if edge.voucher not in self.nodes:
            self.nodes[edge.voucher] = VouchNode(
                address=edge.voucher,
                is_voucher=True,
                total_stake_stroops=edge.stake_stroops,
                is_sybil=edge.is_sybil,
            )
        else:
            v_node = self.nodes[edge.voucher]
            v_node.is_voucher = True
            v_node.total_stake_stroops += edge.stake_stroops
            if edge.is_sybil:
                v_node.is_sybil = True

        # Register borrower node
        if edge.borrower not in self.nodes:
            self.nodes[edge.borrower] = VouchNode(
                address=edge.borrower,
                is_borrower=True,
                is_sybil=edge.is_sybil,
            )
        else:
            b_node = self.nodes[edge.borrower]
            b_node.is_borrower = True
            if edge.is_sybil:
                b_node.is_sybil = True

    @property
    def total_staked_stroops(self) -> int:
        return sum(e.stake_stroops for e in self.edges)

    @property
    def total_staked_xlm(self) -> float:
        return self.total_staked_stroops / XLM_TO_STROOP

    @property
    def active_vouches_count(self) -> int:
        return len(self.edges)

    @property
    def sybil_vouches_count(self) -> int:
        return sum(1 for e in self.edges if e.is_sybil)

    @property
    def sybil_nodes_count(self) -> int:
        return sum(1 for n in self.nodes.values() if n.is_sybil)

    def get_borrower_stake(self, borrower: str) -> int:
        return sum(e.stake_stroops for e in self.edges if e.borrower == borrower)

    def get_voucher_stake(self, voucher: str) -> int:
        return sum(e.stake_stroops for e in self.edges if e.voucher == voucher)

    def stake_distribution(self) -> dict[str, float]:
        """Returns normalized stake distribution per voucher."""
        total = self.total_staked_stroops
        if total == 0:
            return {}
        vouchers: dict[str, int] = {}
        for e in self.edges:
            vouchers[e.voucher] = vouchers.get(e.voucher, 0) + e.stake_stroops
        return {v: s / total for v, s in vouchers.items()}

    def to_state_snapshot_dict(
        self,
        captured_at: str | None = None,
        total_loans: int = 10,
        active_loans: int = 8,
        defaulted_loans: int = 1,
        repaid_loans: int = 1,
        average_interest_rate_bps: float = 850.0,
        storage_entries: int = 100,
        storage_limit: int = 1000,
        avg_gas_per_tx: float = 250_000.0,
    ) -> dict[str, Any]:
        """Convert vouch graph metrics into a StateSnapshot dictionary."""
        return {
            "captured_at": captured_at or time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            "total_loans": total_loans,
            "active_loans": active_loans,
            "defaulted_loans": defaulted_loans,
            "repaid_loans": repaid_loans,
            "average_interest_rate_bps": average_interest_rate_bps,
            "active_vouches": self.active_vouches_count,
            "total_staked": self.total_staked_stroops,
            "storage_entries": storage_entries,
            "storage_limit": storage_limit,
            "avg_gas_per_tx": avg_gas_per_tx,
        }


# ── Graph Generators ────────────────────────────────────────────────────────────

def generate_legitimate_vouch_graph(
    num_vouchers: int = 10,
    num_borrowers: int = 5,
    avg_stake_xlm: float = 100.0,
) -> VouchGraph:
    """Generates a realistic graph of organic users and diverse stakes."""
    graph = VouchGraph()
    for i in range(num_vouchers):
        voucher = f"G_LEGIT_VOUCHER_{i:03d}"
        borrower = f"G_LEGIT_BORROWER_{i % num_borrowers:03d}"
        # Vary stakes slightly
        stake = avg_stake_xlm * (0.8 + (i * 0.05))
        graph.add_edge(VouchEdge.from_xlm(voucher, borrower, stake, is_sybil=False))
    return graph


def generate_sybil_vouch_graph(
    target_borrower: str = "G_SYBIL_BORROWER_001",
    num_sybils: int = 50,
    stake_per_sybil_xlm: float = 0.1,
) -> VouchGraph:
    """Generates a Sybil attack cluster with minimal stakes targeting a single borrower."""
    graph = VouchGraph()
    for i in range(num_sybils):
        sybil_voucher = f"G_SYBIL_ATTACKER_{i:04d}"
        graph.add_edge(VouchEdge.from_xlm(sybil_voucher, target_borrower, stake_per_sybil_xlm, is_sybil=True))
    return graph


def generate_sybil_attack_scenario(
    scenario: str = "loan_spam_attack",
) -> tuple[list[dict[str, Any]], dict[str, Any]]:
    """Generates a baseline history and an anomalous snapshot resulting from a Sybil attack.

    Supported scenarios:
    - 'loan_spam_attack': Sybil ring spams loan requests exceeding max_loan_count_increase_per_poll.
    - 'mass_default_attack': Sybil accounts default en masse on all borrowed capital.
    - 'vouch_collapse_attack': Sybil cluster abruptly withdraws vouches after extracting capital.
    - 'interest_rate_distortion_attack': Sybil cluster manipulates dynamic interest rate out of range.
    - 'storage_exhaustion_attack': Sybil cluster spams state entries approaching storage capacity.
    - 'gas_spike_attack': Sybil transactions cause complex reentrancy spikes in gas consumption.
    """
    baseline_history: list[dict[str, Any]] = [
        {
            "captured_at": "2026-08-30T10:00:00Z",
            "total_loans": 100,
            "active_loans": 50,
            "defaulted_loans": 2,
            "repaid_loans": 48,
            "average_interest_rate_bps": 650.0,
            "active_vouches": 150,
            "total_staked": 150_000_000_000,  # 15,000 XLM
            "storage_entries": 300,
            "storage_limit": 1000,
            "avg_gas_per_tx": 200_000.0,
        },
        {
            "captured_at": "2026-08-30T11:00:00Z",
            "total_loans": 105,
            "active_loans": 52,
            "defaulted_loans": 2,
            "repaid_loans": 51,
            "average_interest_rate_bps": 660.0,
            "active_vouches": 155,
            "total_staked": 155_000_000_000,
            "storage_entries": 315,
            "storage_limit": 1000,
            "avg_gas_per_tx": 205_000.0,
        },
    ]

    if scenario == "loan_spam_attack":
        sybil_graph = generate_sybil_vouch_graph(num_sybils=1500, stake_per_sybil_xlm=0.1)
        anomalous_snapshot = sybil_graph.to_state_snapshot_dict(
            captured_at="2026-08-30T12:00:00Z",
            total_loans=1200,  # Jump of +1095 from 105 (exceeds default threshold 1000)
            active_loans=1100,
            defaulted_loans=2,
            repaid_loans=98,
            average_interest_rate_bps=670.0,
            storage_entries=550,
            storage_limit=1000,
            avg_gas_per_tx=210_000.0,
        )
    elif scenario == "mass_default_attack":
        sybil_graph = generate_sybil_vouch_graph(num_sybils=20, stake_per_sybil_xlm=10.0)
        anomalous_snapshot = sybil_graph.to_state_snapshot_dict(
            captured_at="2026-08-30T12:00:00Z",
            total_loans=110,
            active_loans=42,
            defaulted_loans=18,  # Jump of +16 defaults (exceeds default threshold 5)
            repaid_loans=50,
            average_interest_rate_bps=650.0,
            storage_entries=320,
            storage_limit=1000,
            avg_gas_per_tx=200_000.0,
        )
    elif scenario == "vouch_collapse_attack":
        # Sybils withdraw vouches: active_vouches drops from 155 down to 50 (>30% drop)
        graph = VouchGraph()
        for i in range(50):
            graph.add_edge(VouchEdge.from_xlm(f"G_REMAIN_{i}", "G_BORROWER", 10.0))
        anomalous_snapshot = graph.to_state_snapshot_dict(
            captured_at="2026-08-30T12:00:00Z",
            total_loans=105,
            active_loans=52,
            defaulted_loans=2,
            repaid_loans=51,
            average_interest_rate_bps=660.0,
            storage_entries=200,
            storage_limit=1000,
            avg_gas_per_tx=205_000.0,
        )
    elif scenario == "interest_rate_distortion_attack":
        legit_graph = generate_legitimate_vouch_graph()
        anomalous_snapshot = legit_graph.to_state_snapshot_dict(
            captured_at="2026-08-30T12:00:00Z",
            total_loans=106,
            active_loans=53,
            defaulted_loans=2,
            repaid_loans=51,
            average_interest_rate_bps=6500.0,  # Exceeds max range 5000 bps
            storage_entries=320,
            storage_limit=1000,
            avg_gas_per_tx=205_000.0,
        )
    elif scenario == "storage_exhaustion_attack":
        sybil_graph = generate_sybil_vouch_graph(num_sybils=100)
        anomalous_snapshot = sybil_graph.to_state_snapshot_dict(
            captured_at="2026-08-30T12:00:00Z",
            total_loans=108,
            active_loans=54,
            defaulted_loans=2,
            repaid_loans=52,
            average_interest_rate_bps=660.0,
            storage_entries=850,  # 85% of 1000 limit (exceeds 80% warn threshold)
            storage_limit=1000,
            avg_gas_per_tx=205_000.0,
        )
    elif scenario == "gas_spike_attack":
        legit_graph = generate_legitimate_vouch_graph()
        anomalous_snapshot = legit_graph.to_state_snapshot_dict(
            captured_at="2026-08-30T12:00:00Z",
            total_loans=106,
            active_loans=53,
            defaulted_loans=2,
            repaid_loans=51,
            average_interest_rate_bps=660.0,
            storage_entries=320,
            storage_limit=1000,
            avg_gas_per_tx=320_000.0,  # >50% increase from 205,000 (exceeds 25% threshold)
        )
    else:
        raise ValueError(f"Unknown scenario: {scenario}")

    return baseline_history, anomalous_snapshot


# ── Pre-built Static Fixtures ───────────────────────────────────────────────────

SAMPLE_LEGITIMATE_GRAPH: VouchGraph = generate_legitimate_vouch_graph(num_vouchers=8, num_borrowers=4, avg_stake_xlm=50.0)
SAMPLE_SYBIL_ATTACK_GRAPH: VouchGraph = generate_sybil_vouch_graph(target_borrower="G_SYBIL_COLLUSION", num_sybils=30, stake_per_sybil_xlm=0.1)

SAMPLE_STATE_HISTORY: list[dict[str, Any]] = [
    {
        "captured_at": "2026-08-30T00:00:00Z",
        "total_loans": 50,
        "active_loans": 30,
        "defaulted_loans": 1,
        "repaid_loans": 19,
        "average_interest_rate_bps": 500.0,
        "active_vouches": 75,
        "total_staked": 75_000_000_000,
        "storage_entries": 150,
        "storage_limit": 1000,
        "avg_gas_per_tx": 180_000.0,
    },
    {
        "captured_at": "2026-08-30T01:00:00Z",
        "total_loans": 52,
        "active_loans": 31,
        "defaulted_loans": 1,
        "repaid_loans": 20,
        "average_interest_rate_bps": 510.0,
        "active_vouches": 78,
        "total_staked": 78_000_000_000,
        "storage_entries": 155,
        "storage_limit": 1000,
        "avg_gas_per_tx": 182_000.0,
    },
]

SAMPLE_NORMAL_SNAPSHOT: dict[str, Any] = {
    "captured_at": "2026-08-30T02:00:00Z",
    "total_loans": 53,
    "active_loans": 31,
    "defaulted_loans": 1,
    "repaid_loans": 21,
    "average_interest_rate_bps": 515.0,
    "active_vouches": 80,
    "total_staked": 80_000_000_000,
    "storage_entries": 160,
    "storage_limit": 1000,
    "avg_gas_per_tx": 183_000.0,
}

SAMPLE_VALIDATOR_STATES: dict[str, list[dict[str, Any]]] = {
    "unanimous": [
        {
            "name": "validator-1",
            "endpoint": "https://rpc1.example.com",
            "ledger": 1000,
            "values": {"get_config": {"admin": "G_ADMIN"}, "get_fee_treasury": 5000, "get_slash_treasury": 10000},
            "error": None,
        },
        {
            "name": "validator-2",
            "endpoint": "https://rpc2.example.com",
            "ledger": 1000,
            "values": {"get_config": {"admin": "G_ADMIN"}, "get_fee_treasury": 5000, "get_slash_treasury": 10000},
            "error": None,
        },
        {
            "name": "validator-3",
            "endpoint": "https://rpc3.example.com",
            "ledger": 1000,
            "values": {"get_config": {"admin": "G_ADMIN"}, "get_fee_treasury": 5000, "get_slash_treasury": 10000},
            "error": None,
        },
    ],
    "divergent": [
        {
            "name": "validator-1",
            "endpoint": "https://rpc1.example.com",
            "ledger": 1000,
            "values": {"get_config": {"admin": "G_ADMIN"}, "get_fee_treasury": 5000, "get_slash_treasury": 10000},
            "error": None,
        },
        {
            "name": "validator-2",
            "endpoint": "https://rpc2.example.com",
            "ledger": 1000,
            "values": {"get_config": {"admin": "G_ADMIN"}, "get_fee_treasury": 5000, "get_slash_treasury": 99999},  # divergence
            "error": None,
        },
    ],
    "lagging": [
        {
            "name": "validator-1",
            "endpoint": "https://rpc1.example.com",
            "ledger": 1005,
            "values": {"get_config": {"admin": "G_ADMIN"}},
            "error": None,
        },
        {
            "name": "validator-2",
            "endpoint": "https://rpc2.example.com",
            "ledger": 1000,  # 5 ledgers behind (tolerance = 2)
            "values": {"get_config": {"admin": "G_ADMIN"}},
            "error": None,
        },
    ],
    "unreachable": [
        {
            "name": "validator-1",
            "endpoint": "https://rpc1.example.com",
            "ledger": 1000,
            "values": {"get_config": {"admin": "G_ADMIN"}},
            "error": None,
        },
        {
            "name": "validator-2",
            "endpoint": "https://unreachable.example.com",
            "ledger": None,
            "values": {},
            "error": "Connection refused",
        },
    ],
}
