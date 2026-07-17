# Contract Event Indexing Guide

Guide for indexing and querying QuorumCredit contract events off-chain.

---

## Overview

QuorumCredit emits Soroban contract events for every state-changing operation. Off-chain systems (dashboards, notification services, analytics) can subscribe to these events via the Stellar RPC or Horizon API to track protocol state without querying contract storage directly.

A **persistent indexer** lives at `tools/indexer/` — a Rust binary that polls Soroban RPC, stores events in SQLite, detects ledger reorgs, handles Soroban's bounded event-retention window, and exports Prometheus metrics over HTTP.

---

## Event Structure

Every Soroban contract event has:

| Field | Description |
|-------|-------------|
| `type` | Always `"contract"` for contract events |
| `contractId` | The deployed QuorumCredit contract address |
| `topics` | Array of XDR-encoded values identifying the event |
| `value` | XDR-encoded event payload |
| `ledger` | Ledger sequence number when the event was emitted |
| `ledgerClosedAt` | ISO 8601 timestamp of ledger close |
| `txHash` | Transaction hash that triggered the event |

Topics are always a two-element array: `[category, action]`, both encoded as `Symbol`.

---

## All Contract Events

### `contract/init`

Emitted once when the contract is initialized.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"contract"` |
| topics[1] | Symbol | `"init"` |
| value | `(Address, Vec<Address>, u32, Address)` | `(deployer, admins, admin_threshold, token)` |

---

### `vouch/create`

Emitted when a new vouch is created.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"vouch"` |
| topics[1] | Symbol | `"create"` |
| value | `(Address, Address, i128, Address)` | `(voucher, borrower, stake_stroops, token)` |

---

### `vouch/increase`

Emitted when a voucher increases their stake.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"vouch"` |
| topics[1] | Symbol | `"increase"` |
| value | `(Address, Address, i128, Address)` | `(voucher, borrower, additional_stake_stroops, token)` |

---

### `vouch/decrease`

Emitted when a voucher decreases their stake.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"vouch"` |
| topics[1] | Symbol | `"decrease"` |
| value | `(Address, Address, i128, Address)` | `(voucher, borrower, new_stake_stroops, token)` |

---

### `vouch/withdraw`

Emitted when a voucher fully withdraws their vouch.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"vouch"` |
| topics[1] | Symbol | `"withdraw"` |
| value | `(Address, Address, i128, Address)` | `(voucher, borrower, returned_stake_stroops, token)` |

---

### `loan/request`

Emitted when a loan is disbursed to a borrower.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"loan"` |
| topics[1] | Symbol | `"request"` |
| value | `(Address, i128, i128, String, Address)` | `(borrower, amount_stroops, threshold_stroops, loan_purpose, token)` |

---

### `loan/repay`

Emitted when a borrower makes a repayment.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"loan"` |
| topics[1] | Symbol | `"repay"` |
| value | `(Address, i128)` | `(borrower, payment_stroops)` |

---

### `loan/slash`

Emitted when a borrower's loan is slashed.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"loan"` |
| topics[1] | Symbol | `"slash"` |
| value | `(Address, i128)` | `(borrower, total_slashed_stroops)` |

---

### `admin/config`

Emitted when the protocol configuration is updated.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"admin"` |
| topics[1] | Symbol | `"config"` |
| value | `(Address, Config)` | `(admin, new_config)` |

---

### `admin/pause` / `admin/unpause`

Emitted when the contract is paused or unpaused.

| Field | Type | Description |
|-------|------|-------------|
| topics[0] | Symbol | `"admin"` |
| topics[1] | Symbol | `"pause"` or `"unpause"` |
| value | `Address` | Admin address that triggered the action |

---

## Indexer: `tools/indexer/`

A production-grade Rust indexer that durably tracks processed ledgers, survives restarts, detects and recovers from Soroban network reorgs, and handles Soroban RPC's bounded event-retention window.

### Architecture

```
┌──────────────────────────┐     ┌──────────────────────────┐
│   Soroban RPC Client     │◄───►│   Core Indexer Loop      │
│   (src/rpc.rs)           │     │   (src/indexer.rs)       │
└──────────────────────────┘     └───────┬──────────────────┘
                                         │
                          ┌──────────────┼──────────────┐
                          │              │              │
                   ┌──────▼─────┐ ┌──────▼──────┐ ┌─────▼──────┐
                   │   SQLite   │ │  Prometheus │ │  /metrics  │
                   │  Store     │ │  Metrics    │ │  HTTP      │
                   │ (src/db.rs)│ │ (src/metrics)│ │  Server   │
                   └────────────┘ └─────────────┘ └────────────┘
```

### Database Schema (SQLite)

- **`cursor`** — Key-value store for `last_ledger` (the durable cursor). Survives process restarts so the indexer always resumes from where it left off.
- **`events`** — Every indexed event with typed `category`, `action` columns and a `value_json` JSON payload. A unique index on `(ledger, tx_hash, category, action)` provides deduplication.
- **`ledger_hashes`** — Tracks `(sequence, hash)` pairs for reorg detection. On each poll the indexer compares the RPC's reported hash against the stored hash for the same sequence.
- **`reorg_audit`** — Append-only log of every reorg event, recording the ledger, expected hash, and actual hash.
- **`vouch_events` / `loan_events`** — SQL views that extract typed columns from `events.value_json` for ergonomic querying.

### Crash Safety

`last_ledger` is written to the `cursor` table inside each poll cycle. SQLite uses WAL mode with `synchronous = NORMAL`. After an abrupt crash, the indexer reads `last_ledger` from the durable store on startup and resumes from that point. Metrics are rebuilt from the full event history.

### Event-Retention Window Handling

Soroban RPC only retains events for ~17 280 ledgers (~24 h). If the indexer has been offline longer than the configured `--retention-window`, a gap is detected on startup. The indexer then enters **backfill mode**: it walks forward from the stored cursor in configurable chunk sizes (`--backfill-chunk-size`), re-indexing any events the RPC still serves. Events that have fallen out of the retention window cannot be recovered — operators must provide a deploy ledger or snapshot to backfill from.

### Ledger Reorg Handling

On each poll, the indexer calls `getLatestLedger()` and stores the returned `(sequence, hash)`. If a subsequent call returns a different hash for a sequence we've already recorded, a reorg is detected:

1. All events at `>= reorg_sequence` are deleted from the store.
2. The cursor is rewound to `reorg_sequence - 1`.
3. An entry is written to the `reorg_audit` table.
4. In-memory metrics are rebuilt from the surviving events.
5. The next poll will re-fetch events from the rewound position.

### Building and Running

```bash
# Build the indexer
cargo build -p quorum-credit-indexer --release

# Run with defaults (testnet)
cargo run -p quorum-credit-indexer --release -- \
  --contract-id "C..." \
  --rpc-url "https://soroban-testnet.stellar.org" \
  --db-path "/data/indexer.db" \
  --metrics-port 9090

# Full options
cargo run -p quorum-credit-indexer --release -- --help
```

### CLI Options

| Flag | Default | Description |
|------|---------|-------------|
| `--rpc-url` | `https://soroban-testnet.stellar.org` | Soroban RPC endpoint |
| `--contract-id` | (required) | Deployed QuorumCredit contract ID |
| `--db-path` | `indexer.db` | Path to SQLite database file |
| `--metrics-port` | `9090` | Port for Prometheus `/metrics` endpoint |
| `--poll-interval-ms` | `5000` | Milliseconds between poll cycles |
| `--retention-window` | `15000` | Ledger gap threshold that triggers backfill |
| `--deploy-ledger` | (none) | Deploy ledger sequence (for fresh starts) |

### Metrics (served at `/metrics`)

| Metric | Type | Description |
|--------|------|-------------|
| `qc_indexer_ledger_height` | Gauge | Last processed ledger sequence |
| `qc_indexer_events_total` | Counter | Events indexed, by `category` / `action` |
| `qc_indexer_gap_detected_total` | Counter | Retention-window gaps detected |
| `qc_indexer_reorgs_detected_total` | Counter | Ledger reorgs detected |
| `qc_indexer_errors_total` | Counter | Indexer-level errors by code |
| `qc_indexer_backfill_events_total` | Counter | Events indexed during backfill |
| `qc_loan_volume_total` | Counter | Total stroops loaned, by `token` |
| `qc_loan_count_total` | Counter | Total loans created |
| `qc_active_loans` | Gauge | Currently active (unrepaid) loans |
| `qc_slash_events_total` | Counter | Total slash events |
| `qc_slash_amount_total` | Counter | Total stroops slashed, by `token` |
| `qc_vouch_count` | Gauge | Currently active vouches |

### Running Tests

```bash
cargo test -p quorum-credit-indexer
```

The integration tests cover:
- **Restart mid-backfill**: Indexer processes events, is dropped (simulated crash), restarts from the persisted cursor, and catches up.
- **Reorg handling**: A ledger hash changes between polls; the indexer rolls back affected events and re-indexes.
- **Gap detection**: A gap exceeding `--retention-window` triggers automatic backfill.
- **Event deduplication**: Repeated polls return the same events; only one copy is stored.

---

## Querying Indexed Events

### Example: Get all active vouches for a borrower

```sql
SELECT voucher, SUM(stake_stroops) AS total_stake
FROM vouch_events
WHERE borrower = $1
  AND action IN ('create', 'increase')
GROUP BY voucher
HAVING SUM(CASE WHEN action = 'withdraw' THEN -stake_stroops ELSE stake_stroops END) > 0;
```

### Example: Get loan history for a borrower

```sql
SELECT action, amount_stroops, ledger, tx_hash
FROM loan_events
WHERE borrower = $1
ORDER BY ledger ASC;
```

### Example: Get all vouchers who backed a specific borrower

```sql
SELECT DISTINCT voucher
FROM vouch_events
WHERE borrower = $1 AND action = 'create';
```

---

## Amount Conversion

All monetary values in events are in **stroops** (1 XLM = 10,000,000 stroops).

```typescript
const XLM_TO_STROOPS = 10_000_000n;
const stroopsToXlm = (stroops: bigint): number => Number(stroops) / 10_000_000;
const xlmToStroops = (xlm: number): bigint => BigInt(Math.round(xlm * 10_000_000));
```

---

## Notes

- Events are only available for a limited number of ledgers via the RPC `getEvents` endpoint (typically ~17 280 ledgers / ~24 hours on testnet). The indexer handles this with its backfill mechanism.
- `last_ledger` is stored persistently in SQLite — the indexer resumes correctly after restarts.
- On mainnet, use `https://rpc.mainnet.stellar.org` as the RPC URL.
- For the full indexer implementation, see `tools/indexer/`.
