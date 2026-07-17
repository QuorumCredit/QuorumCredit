# Monitoring and Alerting Setup Guide

Comprehensive monitoring for QuorumCredit protocol operations.

## Prerequisites

This guide assumes the **QuorumCredit indexer** (`tools/indexer/`) is deployed and serving Prometheus metrics at `/metrics`. The indexer derives all metrics from actual on-chain events — no fabricated contract-state calls.

See [tools/indexer/README.md](../tools/indexer/src/main.rs) or `cargo run -p quorum-credit-indexer -- --help` for deployment instructions.

## Prometheus Configuration

### Scrape the indexer's `/metrics` endpoint

```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'quorum-credit-indexer'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
```

### Available Metrics

The indexer exposes the following metrics sourced entirely from the Soroban event stream — no `get_contract_data` calls:

| Metric | Type | Labels | Description |
|--------|------|--------|-------------|
| `qc_indexer_ledger_height` | Gauge | — | Last processed ledger sequence |
| `qc_indexer_events_total` | Counter | `category`, `action` | Events indexed |
| `qc_indexer_gap_detected_total` | Counter | — | Retention-window gaps detected |
| `qc_indexer_reorgs_detected_total` | Counter | — | Ledger reorgs detected |
| `qc_indexer_errors_total` | Counter | `error_code` | Indexer-level errors |
| `qc_indexer_backfill_events_total` | Counter | — | Events indexed during backfill |
| `qc_loan_volume_total` | Counter | `token` | Total stroops loaned |
| `qc_loan_count_total` | Counter | — | Total loans created |
| `qc_active_loans` | Gauge | — | Currently active (unrepaid) loans |
| `qc_slash_events_total` | Counter | — | Total slash events |
| `qc_slash_amount_total` | Counter | `token` | Total stroops slashed |
| `qc_vouch_count` | Gauge | — | Currently active vouches |

### Metric Semantics

- **Counters** (`_total` suffix) are monotonic — they persist across indexer restarts via rebuild from the event store.
- **Gauges** (`qc_active_loans`, `qc_vouch_count`, `qc_indexer_ledger_height`) are set from the event stream and reset on restart.
- **Labels** (`token`, `category`, `action`) are fixed at metric creation time. The indexer records events with the appropriate label combinations as they arrive.

## Grafana Dashboards

### Dashboard 1: Protocol Overview

```json
{
  "dashboard": {
    "title": "QuorumCredit Protocol Overview",
    "panels": [
      {
        "title": "Active Loans",
        "targets": [
          {
            "expr": "qc_active_loans"
          }
        ]
      },
      {
        "title": "Total Loan Volume (XLM)",
        "targets": [
          {
            "expr": "qc_loan_volume_total / 10000000"
          }
        ]
      },
      {
        "title": "Active Vouches",
        "targets": [
          {
            "expr": "qc_vouch_count"
          }
        ]
      },
      {
        "title": "Indexer Ledger Height",
        "targets": [
          {
            "expr": "qc_indexer_ledger_height"
          }
        ]
      }
    ]
  }
}
```

### Dashboard 2: Risk Metrics

```json
{
  "dashboard": {
    "title": "QuorumCredit Risk Metrics",
    "panels": [
      {
        "title": "Slash Events (24h)",
        "targets": [
          {
            "expr": "increase(qc_slash_events_total[24h])"
          }
        ]
      },
      {
        "title": "Total Amount Slashed (XLM)",
        "targets": [
          {
            "expr": "qc_slash_amount_total / 10000000"
          }
        ]
      },
      {
        "title": "Error Rate (5m)",
        "targets": [
          {
            "expr": "rate(qc_indexer_errors_total[5m])"
          }
        ]
      }
    ]
  }
}
```

### Dashboard 3: Indexer Health

```json
{
  "dashboard": {
    "title": "Indexer Health",
    "panels": [
      {
        "title": "Events per Minute",
        "targets": [
          {
            "expr": "rate(qc_indexer_events_total[1m])"
          }
        ]
      },
      {
        "title": "Reorgs Detected",
        "targets": [
          {
            "expr": "rate(qc_indexer_reorgs_detected_total[1h])"
          }
        ]
      },
      {
        "title": "Backfill Events",
        "targets": [
          {
            "expr": "rate(qc_indexer_backfill_events_total[1h])"
          }
        ]
      }
    ]
  }
}
```

## Alerting Rules

### Alert Rules (Prometheus)

```yaml
# alerts.yml
groups:
  - name: quorum_credit
    interval: 30s
    rules:
      # Indexer down
      - alert: IndexerDown
        expr: up{job="quorum-credit-indexer"} == 0
        for: 1m
        annotations:
          summary: "QuorumCredit indexer is down"
          description: "No metrics received for 1 minute"

      # Ledger height stalled
      - alert: IndexerStalled
        expr: qc_indexer_ledger_height == 0
        for: 5m
        annotations:
          summary: "Indexer has not processed any ledgers"
          description: "Ledger height is 0 for 5 minutes"

      # Indexer errors
      - alert: IndexerErrors
        expr: rate(qc_indexer_errors_total[5m]) > 0.1
        for: 5m
        annotations:
          summary: "Indexer error rate elevated"
          description: "Error rate: {{ $value | humanizePercentage }}"

      # Reorg detected
      - alert: LedgerReorgDetected
        expr: increase(qc_indexer_reorgs_detected_total[5m]) > 0
        for: 1m
        annotations:
          summary: "Soroban ledger reorg detected"
          description: "Indexer rolled back and re-indexed affected events"

      # Excessive slashing
      - alert: ExcessiveSlashing
        expr: increase(qc_slash_events_total[1h]) > 10
        for: 5m
        annotations:
          summary: "Excessive slash events in 1 hour"
          description: "Slash events: {{ $value }}"

      # High active loan ratio vs total
      - alert: HighLoanUtilization
        expr: qc_active_loans > (qc_loan_count_total - qc_active_loans) * 5
        for: 5m
        annotations:
          summary: "Unusually high active-to-repaid loan ratio"
          description: "Active: {{ $value }} loans"
```

## Runbook for Common Alerts

### Alert: IndexerDown / IndexerStalled

**Severity:** Critical

**Symptoms:**
- No metrics from the indexer
- Dashboard data frozen

**Diagnosis:**
```bash
# Check process
systemctl status quorum-credit-indexer

# Check logs
journalctl -u quorum-credit-indexer --since "5 min ago"

# Check database
du -sh /data/indexer.db
sqlite3 /data/indexer.db "SELECT value FROM cursor WHERE key = 'last_ledger';"
```

**Resolution:**
1. Restart the indexer: `systemctl restart quorum-credit-indexer`
2. If the database is corrupted, restore from backup
3. If the RPC endpoint is down, check network connectivity

### Alert: IndexerErrors

**Severity:** High

**Symptoms:**
- Error rate > 10%

**Diagnosis:**
```bash
# Check error distribution
curl 'http://localhost:9090/metrics' | grep qc_indexer_errors_total

# Check indexer logs
journalctl -u quorum-credit-indexer --since "10 min ago" | grep ERROR
```

**Resolution:**
1. Check RPC endpoint health: `curl <rpc-url>/health`
2. Verify network connectivity
3. If persistent, consider rotating RPC endpoints

### Alert: LedgerReorgDetected

**Severity:** Medium

**Symptoms:**
- Spike in `qc_indexer_reorgs_detected_total`

**Diagnosis:**
```bash
# Query reorg audit log
sqlite3 /data/indexer.db "SELECT * FROM reorg_audit ORDER BY id DESC LIMIT 5;"
```

**Resolution:**
This is informational — the indexer automatically recovers. If reorgs are frequent, the Soroban network may be experiencing instability.

### Alert: ExcessiveSlashing

**Severity:** Medium

**Symptoms:**
- > 10 slash events in 1 hour

**Diagnosis:**
```bash
# Query recent slash events from the event store
sqlite3 /data/indexer.db "SELECT ledger, value_json FROM events WHERE category = 'loan' AND action = 'slash' ORDER BY ledger DESC LIMIT 20;"
```

**Resolution:**
1. Investigate borrower defaults
2. Check for coordinated attacks
3. Review voucher selection process
4. Consider adjusting slash threshold if legitimate

### Alert: HighLoanUtilization

**Severity:** Medium

**Symptoms:**
- Active loans >> repaid loans

**Diagnosis:**
```bash
# Check active vs total loan counts
curl 'http://localhost:9090/metrics' | grep -E 'qc_active_loans|qc_loan_count_total'
```

**Resolution:**
1. Check if borrowers are defaulting
2. Review repayment rates
3. Consider pausing new loans until existing ones are repaid

## Monitoring Setup Checklist

- [ ] Prometheus installed and configured
- [ ] QuorumCredit indexer deployed and scraping
- [ ] Grafana dashboards imported
- [ ] Alert rules configured
- [ ] Alert channels (Slack, PagerDuty) configured
- [ ] On-call rotation established
- [ ] Runbooks documented and accessible
- [ ] Monitoring tested with synthetic transactions
- [ ] Dashboards accessible to ops team
- [ ] Metrics retention policy set (30 days minimum)

## References

- [Event Indexing Guide](./event-indexing-guide.md) — full event schema and indexer documentation
- [tools/indexer/](../tools/indexer/) — indexer source code and integration tests
