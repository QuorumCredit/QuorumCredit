# Testnet Integration Testing Guide

This guide explains how to run the full QuorumCredit loan lifecycle against the
Stellar Testnet using `scripts/testnet_integration_test.sh`.

---

## Prerequisites

| Tool | Install |
|------|---------|
| Rust + `wasm32-unknown-unknown` | `rustup target add wasm32-unknown-unknown` |
| Stellar CLI | `cargo install --locked stellar-cli` |
| `jq` | `apt install jq` / `brew install jq` |
| Funded testnet account | [Stellar Friendbot](https://friendbot.stellar.org) |

---

## Environment Setup

Create a `.env` file in the project root (never commit this):

```bash
NETWORK=testnet
DEPLOYER_SECRET_KEY="SB..."   # Deployer secret key (S...)
ADMIN_ADDRESS="GB..."         # Admin public key (G...)
TOKEN_CONTRACT="C..."         # Native XLM token contract on testnet
```

Fund your deployer account via Friendbot:

```bash
curl "https://friendbot.stellar.org?addr=$DEPLOYER_ADDRESS"
```

---

## Running the Integration Tests

```bash
chmod +x scripts/testnet_integration_test.sh
./scripts/testnet_integration_test.sh
```

The script executes the following steps in order:

| Step | Operation | Validates |
|------|-----------|-----------|
| 1 | Build WASM | Artifact exists |
| 2 | Deploy contract | Contract ID returned |
| 3 | Initialize contract | `get_config` returns expected values |
| 4 | Vouch (voucher → borrower) | `get_vouches` shows stake |
| 5 | Request loan | `loan_status` = Active, borrower balance increases |
| 6 | Repay loan | `loan_status` = Repaid, voucher balance increases |
| 7 | Vouch + slash flow | `loan_status` = Defaulted, slash treasury increases |
| 8 | Fee calculation check | Protocol fee deducted correctly |

---

## What the Tests Cover

### Full Loan Lifecycle
- Deploy → Initialize → Vouch → Request Loan → Repay
- Verifies token balances at each step

### Slash Flow
- Deploy → Initialize → Vouch → Request Loan → Vote Slash → Execute
- Verifies slash treasury accounting

### Fee Calculations
- Sets a non-zero protocol fee via `set_protocol_fee`
- Verifies fee is deducted from repayment and credited to fee treasury

### Concurrent Operations (manual)
For concurrent operation testing, run multiple instances of the script with
different borrower/voucher keypairs simultaneously. The contract's auth model
ensures each operation is isolated by address.

---

## Interpreting Results

Each step prints `[PASS]` or `[FAIL]` with the actual vs expected value.
A non-zero exit code means at least one step failed.

```
[PASS] Step 3: Contract initialized — config.yield_bps=200
[PASS] Step 4: Vouch recorded — total_vouched=10000000
[PASS] Step 5: Loan disbursed — borrower_balance_delta=5000000
[PASS] Step 6: Loan repaid — loan_status=Repaid
[PASS] Step 7: Slash executed — slash_treasury>0
[PASS] Step 8: Fee collected — fee_treasury>0
```

---

## Troubleshooting

| Error | Likely Cause | Fix |
|-------|-------------|-----|
| `Error: DEPLOYER_SECRET_KEY not set` | Missing `.env` | Create `.env` with required vars |
| `stellar: command not found` | CLI not installed | `cargo install --locked stellar-cli` |
| `InsufficientFunds` on loan request | Contract not pre-funded | Send XLM to contract address |
| `VouchTooRecent` | Vouch age < 60 s | Wait 60 s or use `--no-vouch-age` flag |
| `AlreadyInitialized` | Contract reused | Deploy a fresh contract |

---

## CI Integration

The testnet integration tests are **not** run on every PR (they require live
network access and funded accounts). They are triggered manually or on a
scheduled basis via `.github/workflows/deploy-testnet.yml`.

To run them in CI, set the following repository secrets:

- `TESTNET_DEPLOYER_SECRET_KEY`
- `TESTNET_ADMIN_ADDRESS`
- `TESTNET_TOKEN_CONTRACT`

---

## Recurring Payment Execution (Issue #1362 / #1490)

The server can execute on-chain recurring payments via the Soroban RPC client.

### Prerequisites

| Variable | Description |
|----------|-------------|
| `SOROBAN_RPC_URL` | RPC endpoint (e.g. `https://soroban-testnet.stellar.org`) |
| `QUORUM_CREDIT_CONTRACT_ID` | Deployed contract ID |
| `SOROBAN_KEEPER_SECRET_KEY` | Secret key of the keeper account (must be funded and authorized) |

### Setup

1. Fund the keeper account on testnet:
   ```bash
   curl "https://friendbot.stellar.org?addr=$KEEPER_PUBLIC_KEY"
   ```

2. Set the environment variables in `.env`:
   ```bash
   SOROBAN_RPC_URL="https://soroban-testnet.stellar.org"
   QUORUM_CREDIT_CONTRACT_ID="C..."
   SOROBAN_KEEPER_SECRET_KEY="S..."
   ```

### Execution

Trigger a recurring payment via the REST API:

```bash
curl -X POST http://localhost:4000/loans/<borrower-address>/recurring-payment/execute
```

The server will:
1. Derive the borrower address from the loanId (URL path parameter).
2. Build and sign a Soroban `invoke_host_function` transaction calling
   `execute_recurring_payment(borrower)`.
3. Submit the transaction via Soroban RPC.
4. Poll for confirmation and return the transaction hash.

### Monitoring

- `qc_recurring_payments_chain_attempts_total` — counter for every on-chain attempt.
- `qc_recurring_payments_chain_success_total` — counter for successful executions.
- `qc_recurring_payments_chain_failed_total` — counter for failed executions.
- `qc_recurring_payments_rpc_unconfigured_total` — counter when RPC client is missing.

## See Also

- [Deployment Guide](./deployment-guide.md)
- [Contract Invariants](./contract-invariants.md)
- [Threat Model](./threat-model.md)
