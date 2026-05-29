# Production Deployment Guide

This guide covers deploying QuorumCredit to Stellar mainnet with security best practices and operational procedures.

## Environment Setup

### Prerequisites

- Rust 1.70+ with `wasm32-unknown-unknown` target
- Stellar CLI (latest)
- Hardware wallet or multisig setup for admin keys
- Mainnet XLM for deployment fees

### Network Configuration

```bash
# Add mainnet network
stellar network add mainnet \
  --rpc-url https://rpc.mainnet.stellar.org:443 \
  --network-passphrase "Public Global Stellar Network ; September 2015"

# Verify configuration
stellar network list
```

### Environment Variables

Create `.env.mainnet` (never commit):

```bash
NETWORK=mainnet
DEPLOYER_SECRET_KEY="S..."          # Hardware wallet or multisig signer
ADMIN_ADDRESSES="GB...,GB...,GB..."  # Comma-separated admin addresses
ADMIN_THRESHOLD=2                    # Minimum signatures required
TOKEN_CONTRACT="CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"  # XLM token
```

## Contract Deployment

### Step 1: Build Optimized WASM

```bash
cd QuorumCredit
cargo build --target wasm32-unknown-unknown --release

# Verify build
ls -lh target/wasm32-unknown-unknown/release/quorum_credit.wasm
```

### Step 2: Deploy Contract

```bash
CONTRACT_ID=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network mainnet \
  --source $DEPLOYER_SECRET_KEY | grep -oP 'Contract ID: \K\S+')

echo "Deployed: $CONTRACT_ID"
```

### Step 3: Initialize Contract

**Critical:** Use the same deployer key that signed the deploy transaction.

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn initialize \
  --network mainnet \
  --source $DEPLOYER_SECRET_KEY \
  -- \
  --deployer $DEPLOYER_ADDRESS \
  --admins '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --admin_threshold 2 \
  --token $TOKEN_CONTRACT
```

## Admin Initialization

### Multisig Setup

For production, use 2-of-3 or 3-of-5 multisig:

```bash
# Example: 2-of-3 multisig
ADMINS='["GBADMIN1...","GBADMIN2...","GBADMIN3..."]'
THRESHOLD=2
```

### Initial Configuration

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn set_config \
  --network mainnet \
  --source $ADMIN_1_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --config '{
    "admins": ["'$ADMIN_1'","'$ADMIN_2'","'$ADMIN_3'"],
    "admin_threshold": 2,
    "token": "'$TOKEN_CONTRACT'",
    "allowed_tokens": ["'$TOKEN_CONTRACT'"],
    "yield_bps": 200,
    "slash_bps": 5000,
    "max_vouchers": 100,
    "min_loan_amount": 100000,
    "loan_duration": 2592000,
    "max_loan_to_stake_ratio": 5000,
    "grace_period": 604800
  }'
```

## Security Checklist

### Key Management

- [ ] Admin keys stored in hardware wallets (Ledger/Trezor)
- [ ] No private keys in version control or CI/CD
- [ ] Multisig threshold ≥ 2 for all admin operations
- [ ] Key rotation procedure documented
- [ ] Backup keys stored securely offline

### Rate Limiting

- [ ] API rate limits configured (if using RPC proxy)
- [ ] Transaction queue monitoring enabled
- [ ] Spike detection alerts configured

### Audit Logging

- [ ] All admin operations logged with timestamps
- [ ] Loan disbursements logged with borrower/amount
- [ ] Slash events logged with voucher impacts
- [ ] Logs stored in immutable storage (e.g., S3 with versioning)

### Contract Verification

- [ ] WASM hash verified against source code
- [ ] Contract source published on GitHub
- [ ] Audit report available (if applicable)

## Operational Procedures

### Pre-Deployment Checklist

- [ ] All tests passing: `cargo test`
- [ ] Code review completed
- [ ] Security audit passed
- [ ] Testnet deployment verified
- [ ] Rollback plan documented

### Monitoring Setup

- [ ] Prometheus metrics configured
- [ ] Grafana dashboards deployed
- [ ] Alert rules active
- [ ] On-call rotation established

### Yield Reserve Funding

Before accepting loans, pre-fund the yield reserve:

```bash
# Calculate required reserve
# Reserve = (max_loan_amount * max_concurrent_loans) * (1 + yield_bps/10000)

# Transfer XLM to contract
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn transfer \
  --network mainnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --from $ADMIN_ADDRESS \
  --to $CONTRACT_ID \
  --amount 10000000000  # 1000 XLM
```

## Rollback Procedures

### Emergency Pause

If critical issue detected:

```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn pause \
  --network mainnet \
  --source $ADMIN_1_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]'
```

### Contract Upgrade

For non-emergency fixes:

```bash
# 1. Build new WASM
cargo build --target wasm32-unknown-unknown --release

# 2. Install new WASM
NEW_HASH=$(stellar contract install \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network mainnet \
  --source $ADMIN_1_SECRET_KEY)

# 3. Upgrade contract
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn upgrade \
  --network mainnet \
  --source $ADMIN_1_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --new_wasm_hash $NEW_HASH

# 4. Verify upgrade
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network mainnet
```

## Troubleshooting

### Deployment Fails: "InsufficientFunds"

- Ensure deployer account has sufficient XLM (>10 XLM recommended)
- Check network connectivity

### Initialize Fails: "UnauthorizedCaller"

- Verify `--source` key matches `--deployer` address
- Confirm deployer signed the deploy transaction

### Contract Paused

- Check pause status: `stellar contract invoke --id $CONTRACT_ID --fn get_config --network mainnet`
- Unpause with admin multisig: `stellar contract invoke --id $CONTRACT_ID --fn unpause --network mainnet --source $ADMIN_1_SECRET_KEY -- --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]'`

### Yield Reserve Depleted

- Monitor reserve balance continuously
- Set up alerts when reserve < 10% of max loan amount
- Replenish reserve immediately upon alert

## Post-Deployment

### Verification

```bash
# Verify contract initialized
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network mainnet

# Verify admin setup
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_admins \
  --network mainnet
```

### Documentation

- [ ] Contract ID recorded in secure location
- [ ] Deployment date and deployer documented
- [ ] Admin addresses and threshold documented
- [ ] Runbook updated with contract ID

### Monitoring

- [ ] Metrics collection started
- [ ] Dashboards accessible to ops team
- [ ] Alert channels configured
- [ ] On-call schedule active
