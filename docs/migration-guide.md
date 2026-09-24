# Migration Guide

This guide covers migrating QuorumCredit data and contracts across versions, networks, and environments. It includes procedures for data transformations, schema changes, and network migrations.

## Table of Contents

1. [Migration Planning](#migration-planning)
2. [Pre-Migration Checklist](#pre-migration-checklist)
3. [Data Migration](#data-migration)
4. [Contract Migration](#contract-migration)
5. [Network Migration](#network-migration)
6. [Rollback Procedures](#rollback-procedures)
7. [Validation & Verification](#validation--verification)

---

## Migration Planning

### Types of Migrations

| Type | Scope | Examples | Downtime |
|------|-------|----------|----------|
| **Code Upgrade** | Contract WASM | Bug fixes, new features | 5-30 min |
| **Schema Change** | Storage structure | New fields, renamed keys | 30 min - 2 hours |
| **Data Transformation** | Record values | Rate changes, fee adjustments | 1-24 hours |
| **Network Migration** | Environment | Testnet → Mainnet | 2-8 hours |
| **Multi-Version Support** | Backward compat | Version coexistence | None (if implemented) |

### Risk Assessment

Before any migration, assess risks:

```yaml
# migration/plan_template.yml

migration:
  id: MIG-2026-001
  title: "Add co-borrower support"
  version_from: "1.0.0"
  version_to: "1.1.0"
  
  scope:
    affected_contracts: ["core", "governance"]
    affected_features: ["loan_creation", "loan_query"]
    estimated_records: 5000
    
  risk_assessment:
    data_loss_risk: "low"        # Backup available?
    downtime_risk: "medium"      # Can be paused?
    compatibility_risk: "high"   # API changes?
    user_impact: "high"          # Active users affected?
    
  rollback_plan: "Contract downgrade to v1.0.0"
  
  timeline:
    planning: "2026-06-29"
    testing: "2026-07-06"
    staging: "2026-07-10"
    production: "2026-07-15"
    rollback_deadline: "2026-07-20"
```

---

## Pre-Migration Checklist

### Week Before Migration

- [ ] **Announce migration** - Notify users of maintenance window
- [ ] **Finalize plan** - Complete risk assessment and get approval
- [ ] **Prepare rollback** - Test rollback procedure on testnet
- [ ] **Backup current state** - Full backup of contract and database
- [ ] **Test on testnet** - Execute full migration procedure on testnet
- [ ] **Prepare team** - Ensure all team members know their roles

### Day Before Migration

- [ ] **Final testing** - Run smoke tests on testnet migration
- [ ] **Verify backup** - Confirm backup integrity and recoverability
- [ ] **Review runbook** - Walk through migration steps with team
- [ ] **Ensure access** - Verify all necessary keys and access available
- [ ] **Set up monitoring** - Configure alerts for migration window
- [ ] **Prepare communications** - Draft status updates for users

### Hour Before Migration

- [ ] **Team briefing** - Review critical steps and escalation path
- [ ] **Backup verification** - Final backup creation
- [ ] **Freeze deployments** - Halt all other deployments
- [ ] **Record baseline metrics** - Capture pre-migration state
- [ ] **Start incident tracking** - Create migration ticket

---

## Data Migration

### Scenario 1: Adding New Fields

When adding new fields to existing data structures:

```rust
// BEFORE: LoanRecord v1
pub struct LoanRecord {
    pub id: Bytes,
    pub borrower: Address,
    pub amount: i128,
    pub status: u32,
}

// AFTER: LoanRecord v2
pub struct LoanRecord {
    pub id: Bytes,
    pub borrower: Address,
    pub amount: i128,
    pub status: u32,
    pub co_borrower: Option<Address>,  // New field
    pub created_at: u64,                // New field
}
```

**Migration Script:**

```bash
#!/bin/bash
# migrate_add_fields.sh

CONTRACT_ID=$1
NETWORK=$2

echo "Starting data migration: Add new fields"
echo "Contract: $CONTRACT_ID"
echo "Network: $NETWORK"

# Step 1: Pause contract
echo "1. Pausing contract..."
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn pause \
  --network $NETWORK \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]'

# Step 2: Export current state
echo "2. Exporting current state..."
mkdir -p migration/data
./scripts/export_state.sh $CONTRACT_ID > migration/data/loans_v1.json

# Step 3: Transform data
echo "3. Transforming data..."
python3 migration/transform_add_fields.py \
  migration/data/loans_v1.json \
  migration/data/loans_v2.json

# Step 4: Verify transformation
echo "4. Verifying transformation..."
python3 migration/verify_transformation.py migration/data/loans_v2.json
if [ $? -ne 0 ]; then
  echo "Transformation verification failed!"
  exit 1
fi

# Step 5: Deploy new contract version
echo "5. Deploying new contract version..."
cargo build --target wasm32-unknown-unknown --release
NEW_WASM_HASH=$(stellar contract install \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network $NETWORK \
  --source $ADMIN_SECRET_KEY)

# Step 6: Upgrade contract
echo "6. Upgrading contract..."
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn upgrade \
  --network $NETWORK \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --new_wasm_hash $NEW_WASM_HASH

# Step 7: Migrate data in-contract
echo "7. Migrating data in contract..."
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn migrate_v1_to_v2 \
  --network $NETWORK \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --migration_data "$(cat migration/data/loans_v2.json)"

# Step 8: Verify migration
echo "8. Verifying migration..."
./scripts/verify_migration.sh $CONTRACT_ID migration/data/loans_v2.json
if [ $? -ne 0 ]; then
  echo "Migration verification failed! Rolling back..."
  ./scripts/rollback_migration.sh $CONTRACT_ID
  exit 1
fi

# Step 9: Unpause contract
echo "9. Unpausing contract..."
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn unpause \
  --network $NETWORK \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]'

# Step 10: Run smoke tests
echo "10. Running smoke tests..."
./scripts/smoke_tests.sh $CONTRACT_ID
if [ $? -eq 0 ]; then
  echo "✓ Migration completed successfully"
else
  echo "✗ Smoke tests failed! Rolling back..."
  ./scripts/rollback_migration.sh $CONTRACT_ID
  exit 1
fi
```

**Transformation Script (Python):**

```python
#!/usr/bin/env python3
# migrate/transform_add_fields.py

import json
import sys
from datetime import datetime

def migrate_loan_record(loan_v1):
    """Transform LoanRecord from v1 to v2"""
    return {
        "id": loan_v1["id"],
        "borrower": loan_v1["borrower"],
        "amount": loan_v1["amount"],
        "status": loan_v1["status"],
        "co_borrower": None,              # New field: default to None
        "created_at": int(datetime.now().timestamp()),  # New field: current time
    }

def main():
    if len(sys.argv) != 3:
        print("Usage: transform_add_fields.py <input.json> <output.json>")
        sys.exit(1)
    
    input_file = sys.argv[1]
    output_file = sys.argv[2]
    
    # Read v1 data
    with open(input_file, 'r') as f:
        loans_v1 = json.load(f)
    
    # Transform to v2
    loans_v2 = [migrate_loan_record(loan) for loan in loans_v1]
    
    # Write v2 data
    with open(output_file, 'w') as f:
        json.dump(loans_v2, f, indent=2)
    
    print(f"Migrated {len(loans_v2)} records from v1 to v2")

if __name__ == "__main__":
    main()
```

### Scenario 2: Rate and Fee Changes

When adjusting yields, slashing rates, or fee structures:

```bash
#!/bin/bash
# migrate_rates.sh

CONTRACT_ID=$1
NEW_YIELD_BPS=$2   # New yield rate in basis points
NEW_SLASH_BPS=$3   # New slash rate in basis points

echo "Updating contract rates and fees"
echo "Contract: $CONTRACT_ID"
echo "New yield: ${NEW_YIELD_BPS} bps (${NEW_YIELD_BPS/100}%)"
echo "New slash: ${NEW_SLASH_BPS} bps (${NEW_SLASH_BPS/100}%)"

# No pause needed for config changes
# Just update configuration

stellar contract invoke \
  --id $CONTRACT_ID \
  --fn set_config \
  --network mainnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --yield_bps $NEW_YIELD_BPS \
  --slash_bps $NEW_SLASH_BPS

# Verify update
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network mainnet | grep -E "(yield|slash)_bps"

echo "✓ Rate update completed"
```

### Scenario 3: Bulk Data Export/Import

For complex migrations or cross-contract transfers:

```bash
#!/bin/bash
# migrate_export_import.sh

SOURCE_CONTRACT=$1
DEST_CONTRACT=$2
NETWORK=$3

echo "Exporting data from $SOURCE_CONTRACT..."

# Export all data
stellar contract invoke \
  --id $SOURCE_CONTRACT \
  --fn get_config \
  --network $NETWORK > export/config.json

stellar contract invoke \
  --id $SOURCE_CONTRACT \
  --fn get_admins \
  --network $NETWORK > export/admins.json

# Export all loans
./scripts/export_all_loans.sh $SOURCE_CONTRACT > export/loans.jsonl

# Export all vouches
./scripts/export_all_vouches.sh $SOURCE_CONTRACT > export/vouches.jsonl

echo "Importing data into $DEST_CONTRACT..."

# Import configuration
stellar contract invoke \
  --id $DEST_CONTRACT \
  --fn set_config \
  --network $NETWORK \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --config "$(cat export/config.json)"

# Batch import loans and vouches
python3 migration/batch_import.py \
  --contract $DEST_CONTRACT \
  --network $NETWORK \
  --loans export/loans.jsonl \
  --vouches export/vouches.jsonl

echo "✓ Data import completed"
```

---

## Contract Migration

### Version-to-Version Upgrade

When upgrading to a new contract version with schema changes:

```bash
#!/bin/bash
# contract_version_upgrade.sh

CONTRACT_ID=$1
FROM_VERSION="1.0.0"
TO_VERSION="1.1.0"

echo "Contract Version Migration: $FROM_VERSION → $TO_VERSION"

# 1. Pause contract
./scripts/pause_contract.sh $CONTRACT_ID

# 2. Verify data compatibility
echo "Checking data compatibility..."
./scripts/check_compatibility.sh $CONTRACT_ID $FROM_VERSION $TO_VERSION
if [ $? -ne 0 ]; then
  echo "Data compatibility check failed!"
  ./scripts/unpause_contract.sh $CONTRACT_ID
  exit 1
fi

# 3. Build and deploy new version
cargo build --target wasm32-unknown-unknown --release
NEW_WASM=$(stellar contract install \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network mainnet \
  --source $ADMIN_SECRET_KEY)

# 4. Execute upgrade
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn upgrade \
  --network mainnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --new_wasm_hash $NEW_WASM

# 5. Run migration-specific functions
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn migrate_data \
  --network mainnet \
  --source $ADMIN_SECRET_KEY

# 6. Unpause and verify
./scripts/unpause_contract.sh $CONTRACT_ID
./scripts/verify_migration.sh $CONTRACT_ID

echo "✓ Contract upgraded to $TO_VERSION"
```

---

## Network Migration

### Testnet to Mainnet

When promoting from testnet to mainnet:

```bash
#!/bin/bash
# migrate_testnet_to_mainnet.sh

# Configuration
TESTNET_CONTRACT=$1
MAINNET_DEPLOYER=$2

echo "Migrating QuorumCredit from testnet to mainnet"

# Step 1: Export testnet configuration and state
echo "1. Exporting testnet state..."
stellar contract invoke \
  --id $TESTNET_CONTRACT \
  --fn get_config \
  --network testnet > migration/testnet_config.json

# Step 2: Prepare mainnet deployment
echo "2. Building contract for mainnet..."
cargo build --target wasm32-unknown-unknown --release

# Step 3: Deploy to mainnet
echo "3. Deploying to mainnet..."
MAINNET_CONTRACT=$(stellar contract deploy \
  --wasm target/wasm32-unknown-unknown/release/quorum_credit.wasm \
  --network mainnet \
  --source $MAINNET_DEPLOYER | grep -oP 'Contract ID: \K.*')

echo "Mainnet contract: $MAINNET_CONTRACT"

# Step 4: Initialize mainnet contract
echo "4. Initializing mainnet contract..."
TESTNET_CONFIG=$(cat migration/testnet_config.json)

stellar contract invoke \
  --id $MAINNET_CONTRACT \
  --fn initialize \
  --network mainnet \
  --source $MAINNET_DEPLOYER \
  -- \
  --deployer $MAINNET_DEPLOYER \
  --admins '["'$MAINNET_ADMIN_1'","'$MAINNET_ADMIN_2'"]' \
  --admin_threshold 2 \
  --token $MAINNET_TOKEN_CONTRACT

# Step 5: Import testnet data if needed
echo "5. Importing testnet data..."
python3 migration/import_testnet_data.py \
  --testnet-contract $TESTNET_CONTRACT \
  --mainnet-contract $MAINNET_CONTRACT

# Step 6: Verify mainnet deployment
echo "6. Verifying mainnet deployment..."
./scripts/verify_migration.sh $MAINNET_CONTRACT migration/testnet_config.json

echo "✓ Mainnet deployment successful"
```

---

## Rollback Procedures

### Automatic Rollback Triggers

Automatically rollback if:

- Error rate > 5% for > 5 minutes
- Response time > 10 seconds for > 10% of requests
- Contract unresponsive for > 1 minute
- Critical feature unavailable for > 5 minutes

```bash
#!/bin/bash
# monitor_and_rollback.sh

CONTRACT_ID=$1
PREVIOUS_VERSION=$2

while true; do
  # Check error rate
  ERROR_RATE=$(curl -s http://prometheus:9090/api/v1/query \
    --data-urlencode 'query=rate(qc_errors_total[5m])' | \
    jq '.data.result[0].value[1]' -r)
  
  if (( $(echo "$ERROR_RATE > 0.05" | bc -l) )); then
    echo "Error rate critical ($ERROR_RATE). Triggering rollback..."
    ./scripts/rollback_migration.sh $CONTRACT_ID $PREVIOUS_VERSION
    exit 1
  fi
  
  sleep 10
done
```

### Manual Rollback

```bash
#!/bin/bash
# rollback_migration.sh

CONTRACT_ID=$1
PREVIOUS_VERSION=$2

echo "Rolling back contract to $PREVIOUS_VERSION"

# Step 1: Pause contract
./scripts/pause_contract.sh $CONTRACT_ID

# Step 2: Get previous WASM hash
PREVIOUS_WASM=$(grep -A 1 "version: $PREVIOUS_VERSION" deployment/history.json | \
  grep wasm_hash | cut -d'"' -f4)

# Step 3: Restore previous version
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn upgrade \
  --network mainnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --new_wasm_hash $PREVIOUS_WASM

# Step 4: Restore previous state if needed
if [ -f "migration/backup_v$(echo $PREVIOUS_VERSION | cut -d. -f1-2).json" ]; then
  echo "Restoring previous state..."
  ./scripts/restore_state.sh $CONTRACT_ID \
    migration/backup_v$(echo $PREVIOUS_VERSION | cut -d. -f1-2).json
fi

# Step 5: Unpause and verify
./scripts/unpause_contract.sh $CONTRACT_ID
./scripts/smoke_tests.sh $CONTRACT_ID

echo "✓ Rollback to $PREVIOUS_VERSION completed"
```

---

## Validation & Verification

### Pre-Migration Validation

```bash
#!/bin/bash
# validate_migration.sh

CONTRACT_ID=$1

echo "=== Pre-Migration Validation ==="

# Check contract state
echo "1. Contract state valid..."
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn validate_state \
  --network mainnet > /dev/null && echo "✓ Passed" || echo "✗ Failed"

# Check data consistency
echo "2. Data consistency valid..."
./scripts/check_data_consistency.sh $CONTRACT_ID > /dev/null && echo "✓ Passed" || echo "✗ Failed"

# Check admin configuration
echo "3. Admin configuration valid..."
ADMINS=$(stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_admins \
  --network mainnet)
[ ! -z "$ADMINS" ] && echo "✓ Passed" || echo "✗ Failed"

# Check balance consistency
echo "4. Balance consistency valid..."
./scripts/check_balance_consistency.sh $CONTRACT_ID > /dev/null && echo "✓ Passed" || echo "✗ Failed"

echo "=== Validation Complete ==="
```

### Post-Migration Verification

```bash
#!/bin/bash
# verify_migration_complete.sh

CONTRACT_ID=$1
EXPECTED_VERSION=$2

echo "=== Post-Migration Verification ==="

# Verify version
echo "1. Checking version..."
ACTUAL_VERSION=$(stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_version \
  --network mainnet)

if [ "$ACTUAL_VERSION" = "$EXPECTED_VERSION" ]; then
  echo "✓ Version matches: $EXPECTED_VERSION"
else
  echo "✗ Version mismatch: expected $EXPECTED_VERSION, got $ACTUAL_VERSION"
  exit 1
fi

# Test all critical operations
echo "2. Testing loan operations..."
./scripts/test_loan_operations.sh $CONTRACT_ID

echo "3. Testing vouch operations..."
./scripts/test_vouch_operations.sh $CONTRACT_ID

echo "4. Testing admin operations..."
./scripts/test_admin_operations.sh $CONTRACT_ID

# Verify data integrity
echo "5. Verifying data integrity..."
./scripts/verify_data_integrity.sh $CONTRACT_ID

# Check audit trail
echo "6. Checking audit trail..."
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_migration_audit \
  --network mainnet | jq .

echo "=== Verification Complete ✓ ==="
```

---

## Migration Checklist

```markdown
# Migration Checklist: [Migration ID]

## Pre-Migration (Week Before)
- [ ] Migration plan reviewed and approved
- [ ] Risk assessment completed
- [ ] Rollback procedure tested on testnet
- [ ] Full backup of current state
- [ ] Testnet migration completed successfully
- [ ] Team trained on procedures

## Day Before Migration
- [ ] Final testnet validation
- [ ] Backup verified for recoverability
- [ ] Runbook reviewed with team
- [ ] All keys and access verified
- [ ] Monitoring configured
- [ ] User communications prepared

## Migration Execution
- [ ] Announce maintenance window (users)
- [ ] Create incident ticket
- [ ] Capture baseline metrics
- [ ] Pause contract (if needed)
- [ ] Execute migration steps
- [ ] Verify each step
- [ ] Unpause contract (if needed)
- [ ] Run smoke tests
- [ ] Confirm success with team

## Post-Migration
- [ ] Monitor for 24+ hours
- [ ] Verify no data loss
- [ ] Update documentation
- [ ] Notify users of completion
- [ ] Schedule post-migration review
- [ ] Archive migration logs

## Post-Migration Review
- [ ] Conduct team review
- [ ] Document lessons learned
- [ ] Update procedures based on findings
- [ ] Celebrate successful migration 🎉
```

---

## Tools & Scripts

- **Export/Import:** `scripts/export_state.sh`, `scripts/import_state.sh`
- **Transformation:** `migration/transform_*.py`
- **Validation:** `scripts/validate_*.sh`, `scripts/verify_*.sh`
- **Rollback:** `scripts/rollback_migration.sh`
- **Monitoring:** `scripts/monitor_migration.sh`

---

## Support & Questions

For migration-related questions:
- **Slack:** #migrations
- **Email:** migrations@quorumcredit.io
- **Documentation:** https://wiki.quorumcredit.io/migrations
