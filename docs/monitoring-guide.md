# Monitoring and Alerting Setup Guide

Comprehensive monitoring for QuorumCredit protocol operations.

## Prometheus Metrics

### Contract Metrics

Export metrics from contract events via Soroban RPC:

```yaml
# prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'quorum-credit'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
```

### Key Metrics to Track

| Metric | Type | Description |
|--------|------|-------------|
| `qc_loan_volume_total` | Counter | Total loan amount disbursed (stroops) |
| `qc_loan_count_total` | Counter | Total loans created |
| `qc_active_loans` | Gauge | Current active loans |
| `qc_yield_distributed_total` | Counter | Total yield paid to vouchers (stroops) |
| `qc_slash_events_total` | Counter | Total slash events |
| `qc_slash_amount_total` | Counter | Total amount slashed (stroops) |
| `qc_vouch_count` | Gauge | Total active vouches |
| `qc_yield_reserve_balance` | Gauge | Current yield reserve (stroops) |
| `qc_contract_errors_total` | Counter | Errors by code |
| `qc_transaction_latency_ms` | Histogram | Transaction confirmation time |

### Metric Collection Script

```python
import time
from prometheus_client import Counter, Gauge, Histogram, start_http_server
from stellar_sdk import SorobanServer

# Initialize metrics
loan_volume = Counter('qc_loan_volume_total', 'Total loan volume', ['token'])
loan_count = Counter('qc_loan_count_total', 'Total loans created')
active_loans = Gauge('qc_active_loans', 'Active loans')
yield_distributed = Counter('qc_yield_distributed_total', 'Yield distributed', ['token'])
slash_events = Counter('qc_slash_events_total', 'Slash events')
slash_amount = Counter('qc_slash_amount_total', 'Amount slashed', ['token'])
vouch_count = Gauge('qc_vouch_count', 'Active vouches')
yield_reserve = Gauge('qc_yield_reserve_balance', 'Yield reserve balance', ['token'])
contract_errors = Counter('qc_contract_errors_total', 'Contract errors', ['error_code'])
tx_latency = Histogram('qc_transaction_latency_ms', 'Transaction latency')

def collect_metrics(contract_id: str, token_address: str):
    server = SorobanServer("https://soroban-testnet.stellar.org")
    
    # Query contract state
    config = server.get_contract_data(contract_id, 'Config')
    
    # Update gauges
    active_loans.set(config.get('active_loans', 0))
    vouch_count.set(config.get('vouch_count', 0))
    yield_reserve.set(config.get('yield_reserve', 0), {'token': token_address})

if __name__ == '__main__':
    start_http_server(8000)
    while True:
        collect_metrics(CONTRACT_ID, TOKEN_ADDRESS)
        time.sleep(60)
```

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
        "title": "Yield Distributed (XLM)",
        "targets": [
          {
            "expr": "qc_yield_distributed_total / 10000000"
          }
        ]
      },
      {
        "title": "Yield Reserve Balance (XLM)",
        "targets": [
          {
            "expr": "qc_yield_reserve_balance / 10000000"
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
            "expr": "rate(qc_contract_errors_total[5m])"
          }
        ]
      },
      {
        "title": "Yield Reserve Health",
        "targets": [
          {
            "expr": "qc_yield_reserve_balance / (qc_loan_volume_total * 1.02)"
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
      # Yield reserve depletion
      - alert: YieldReserveLow
        expr: qc_yield_reserve_balance < (qc_loan_volume_total * 1.02)
        for: 5m
        annotations:
          summary: "Yield reserve below required level"
          description: "Reserve: {{ $value | humanize }} stroops"

      # High error rate
      - alert: HighErrorRate
        expr: rate(qc_contract_errors_total[5m]) > 0.1
        for: 5m
        annotations:
          summary: "High contract error rate"
          description: "Error rate: {{ $value | humanizePercentage }}"

      # Excessive slashing
      - alert: ExcessiveSlashing
        expr: increase(qc_slash_events_total[1h]) > 10
        for: 5m
        annotations:
          summary: "Excessive slash events in 1 hour"
          description: "Slash events: {{ $value }}"

      # Transaction latency
      - alert: HighTransactionLatency
        expr: histogram_quantile(0.95, qc_transaction_latency_ms) > 5000
        for: 10m
        annotations:
          summary: "High transaction latency"
          description: "P95 latency: {{ $value }}ms"

      # Contract paused
      - alert: ContractPaused
        expr: qc_contract_paused == 1
        for: 1m
        annotations:
          summary: "QuorumCredit contract is paused"
          description: "Contract paused at {{ $timestamp }}"
```

## Runbook for Common Alerts

### Alert: YieldReserveLow

**Severity:** Critical

**Symptoms:**
- Yield reserve balance < required level
- Repayment transactions failing with `InsufficientFunds`

**Diagnosis:**
```bash
# Check reserve balance
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_fee_treasury \
  --network mainnet

# Check active loans
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network mainnet
```

**Resolution:**
1. Pause contract immediately
2. Calculate required reserve: `max_loan_amount * max_concurrent_loans * 1.02`
3. Transfer XLM to contract
4. Verify reserve balance
5. Unpause contract

```bash
# Pause
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn pause \
  --network mainnet \
  --source $ADMIN_1_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]'

# Transfer XLM (example: 1000 XLM)
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn transfer \
  --network mainnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --from $ADMIN_ADDRESS \
  --to $CONTRACT_ID \
  --amount 10000000000

# Unpause
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn unpause \
  --network mainnet \
  --source $ADMIN_1_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]'
```

### Alert: HighErrorRate

**Severity:** High

**Symptoms:**
- Error rate > 10% for 5 minutes
- Users reporting failed transactions

**Diagnosis:**
```bash
# Check error distribution
curl 'http://prometheus:9090/api/v1/query?query=qc_contract_errors_total'

# Check contract status
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network mainnet
```

**Resolution:**
1. Identify error codes from metrics
2. Check if contract is paused
3. Review recent transactions
4. If systematic issue, pause and investigate

### Alert: ExcessiveSlashing

**Severity:** Medium

**Symptoms:**
- > 10 slash events in 1 hour
- Unusual default pattern

**Diagnosis:**
```bash
# Query recent slash events
curl 'http://prometheus:9090/api/v1/query?query=increase(qc_slash_events_total[1h])'

# Check for compromised borrowers
# Review slash vote records
```

**Resolution:**
1. Investigate borrower defaults
2. Check for coordinated attacks
3. Review voucher selection process
4. Consider adjusting slash threshold if legitimate

### Alert: HighTransactionLatency

**Severity:** Medium

**Symptoms:**
- P95 latency > 5 seconds
- Users experiencing slow confirmations

**Diagnosis:**
```bash
# Check Soroban RPC health
curl https://soroban-testnet.stellar.org/health

# Check network congestion
# Review transaction queue
```

**Resolution:**
1. Check Stellar network status
2. Verify RPC endpoint availability
3. Consider increasing transaction fee
4. Contact Stellar support if persistent

### Alert: ContractPaused

**Severity:** Critical

**Symptoms:**
- All state-changing operations fail with `ContractPaused`
- Users cannot vouch, request loans, or repay

**Diagnosis:**
```bash
# Verify pause status
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network mainnet | grep paused
```

**Resolution:**
1. Determine why contract was paused
2. Review admin logs
3. If safe, unpause:

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn unpause \
  --network mainnet \
  --source $ADMIN_1_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]'
```

## Monitoring Setup Checklist

- [ ] Prometheus installed and configured
- [ ] Metrics collection script deployed
- [ ] Grafana dashboards created
- [ ] Alert rules configured
- [ ] Alert channels (Slack, PagerDuty) configured
- [ ] On-call rotation established
- [ ] Runbooks documented and accessible
- [ ] Monitoring tested with synthetic transactions
- [ ] Dashboards accessible to ops team
- [ ] Metrics retention policy set (30 days minimum)

## Synthetic Monitoring

Test protocol health with periodic transactions:

```python
import schedule
import time
from stellar_sdk import Keypair

def synthetic_test():
    """Run synthetic vouch -> loan -> repay cycle"""
    try:
        # Create test accounts
        voucher = Keypair.random()
        borrower = Keypair.random()
        
        # Fund accounts (testnet only)
        # ...
        
        # Vouch
        vouch(CONTRACT_ID, voucher, borrower.public_key, 100 * 10_000_000, TOKEN_ADDRESS)
        
        # Request loan
        request_loan(CONTRACT_ID, borrower, 50 * 10_000_000, 100 * 10_000_000, "Test", TOKEN_ADDRESS)
        
        # Repay
        repay(CONTRACT_ID, borrower, 51 * 10_000_000)
        
        print("Synthetic test passed")
    except Exception as e:
        print(f"Synthetic test failed: {e}")

schedule.every(1).hours.do(synthetic_test)

while True:
    schedule.run_pending()
    time.sleep(60)
```
