# QuorumCredit Backup and Recovery Guide

## Overview

This guide documents backup and recovery procedures for QuorumCredit contract state and data. Data loss could be catastrophic for borrowers and vouchers, so comprehensive backup strategies are essential.

## Contract State Export

### Exporting Contract State

Contract state is stored on the Stellar ledger and can be exported using the Stellar RPC API:

```bash
# Export all contract storage entries
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network testnet \
  --source $ADMIN_KEY

# Export loan records
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_loan \
  --network testnet \
  --source $ADMIN_KEY \
  -- --borrower $BORROWER_ADDRESS

# Export vouches
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_vouches \
  --network testnet \
  --source $ADMIN_KEY \
  -- --borrower $BORROWER_ADDRESS
```

### Automated State Snapshots

Implement automated snapshots using a scheduled job:

```bash
#!/bin/bash
# backup-contract-state.sh

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="./backups/contract_state"
mkdir -p $BACKUP_DIR

# Export config
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network $NETWORK \
  --source $ADMIN_KEY > $BACKUP_DIR/config_$TIMESTAMP.json

# Export admin audit log
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_admin_audit_log \
  --network $NETWORK \
  --source $ADMIN_KEY > $BACKUP_DIR/audit_log_$TIMESTAMP.json

# Export slash treasury
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_slash_treasury_balance \
  --network $NETWORK \
  --source $ADMIN_KEY > $BACKUP_DIR/slash_treasury_$TIMESTAMP.json

echo "Backup completed: $BACKUP_DIR"
```

## Off-Chain Backup Strategy

### Database Backups

Maintain off-chain databases of critical contract state:

1. **Loan Records Database**
   - Store all loan records with timestamps
   - Track repayment history
   - Maintain default records

2. **Vouch Records Database**
   - Store all vouch records
   - Track stake changes
   - Maintain voucher history

3. **Admin Audit Log**
   - Store all admin actions
   - Track configuration changes
   - Maintain governance decisions

### Backup Schedule

- **Hourly**: Export contract balance and paused state
- **Daily**: Full export of all loan and vouch records
- **Weekly**: Full database backup with compression
- **Monthly**: Archive backups to cold storage

### Backup Storage

```
backups/
├── contract_state/
│   ├── config_*.json
│   ├── audit_log_*.json
│   └── slash_treasury_*.json
├── loan_records/
│   └── loans_*.json
├── vouch_records/
│   └── vouches_*.json
└── archives/
    └── backup_*.tar.gz
```

## Recovery Procedures

### Scenario 1: Contract Paused Unexpectedly

**Symptoms**: Contract is paused but no admin action was taken

**Recovery Steps**:
1. Verify contract status: `stellar contract invoke --id $CONTRACT_ID --fn get_paused`
2. Check admin audit log for unauthorized pause
3. If unauthorized, unpause immediately: `stellar contract invoke --id $CONTRACT_ID --fn unpause --source $ADMIN_KEY`
4. Investigate root cause in audit logs

### Scenario 2: Incorrect Configuration

**Symptoms**: Yield rate, slash rate, or other config is wrong

**Recovery Steps**:
1. Export current config: `stellar contract invoke --id $CONTRACT_ID --fn get_config`
2. Compare against backup config from before the change
3. Identify the incorrect parameter
4. Call `update_config` with correct values
5. Verify change: `stellar contract invoke --id $CONTRACT_ID --fn get_config`

### Scenario 3: Loan Record Corruption

**Symptoms**: Loan record is missing or has incorrect data

**Recovery Steps**:
1. Query loan from contract: `stellar contract invoke --id $CONTRACT_ID --fn get_loan --borrower $BORROWER`
2. Compare against backup loan records database
3. If contract record is corrupted:
   - Document the discrepancy
   - Contact affected borrower and vouchers
   - Manually reconcile using backup data
   - Update contract state if necessary (requires admin action)

### Scenario 4: Yield Reserve Depleted

**Symptoms**: Repayment fails with `InsufficientFunds` error

**Recovery Steps**:
1. Check contract balance: `stellar contract invoke --id $CONTRACT_ID --fn get_contract_balance`
2. Check pending repayments: Query all active loans
3. Calculate required yield: `sum(loan.total_yield for all active loans)`
4. If balance < required yield:
   - Transfer additional XLM to contract: `stellar contract invoke --id $CONTRACT_ID --fn transfer --to $CONTRACT_ID --amount $AMOUNT`
   - Verify balance increased
   - Retry repayment

### Scenario 5: Admin Key Compromise

**Symptoms**: Unauthorized admin actions in audit log

**Recovery Steps**:
1. Immediately pause contract: `stellar contract invoke --id $CONTRACT_ID --fn pause --source $SAFE_ADMIN_KEY`
2. Review audit log for unauthorized actions
3. Identify compromised admin key
4. Rotate admin keys:
   - Call `propose_admin` with new admin address
   - Call `accept_admin` from new admin account
   - Remove compromised admin from admins list
5. Unpause contract: `stellar contract invoke --id $CONTRACT_ID --fn unpause --source $SAFE_ADMIN_KEY`

## Disaster Recovery Runbook

### Full Contract Failure

If the contract becomes completely unusable:

1. **Assess Damage**
   - Determine what state is corrupted
   - Identify affected borrowers and vouchers
   - Calculate total exposure

2. **Pause Contract**
   ```bash
   stellar contract invoke \
     --id $CONTRACT_ID \
     --fn pause \
     --network $NETWORK \
     --source $ADMIN_KEY
   ```

3. **Prepare Upgrade**
   - Build fixed WASM
   - Validate upgrade: `stellar contract invoke --id $CONTRACT_ID --fn validate_upgrade --new_wasm_hash $HASH`
   - Test on testnet first

4. **Execute Upgrade**
   ```bash
   stellar contract invoke \
     --id $CONTRACT_ID \
     --fn upgrade \
     --network $NETWORK \
     --source $ADMIN_KEY \
     -- --new_wasm_hash $HASH
   ```

5. **Verify Recovery**
   - Check health: `stellar contract invoke --id $CONTRACT_ID --fn health_check`
   - Verify critical data: `stellar contract invoke --id $CONTRACT_ID --fn get_config`
   - Test basic operations on testnet

6. **Unpause and Communicate**
   ```bash
   stellar contract invoke \
     --id $CONTRACT_ID \
     --fn unpause \
     --network $NETWORK \
     --source $ADMIN_KEY
   ```
   - Notify all users of recovery
   - Provide status updates

### Data Loss Recovery

If contract data is lost:

1. **Restore from Backup**
   - Identify most recent valid backup
   - Extract loan and vouch records
   - Prepare migration script

2. **Recreate State**
   - For each loan record: Call `request_loan` with original parameters
   - For each vouch record: Call `vouch` with original parameters
   - Verify totals match backup

3. **Reconcile Repayments**
   - For each repaid loan: Call `repay` with original payment amount
   - Verify yield distribution matches backup
   - Check voucher balances

4. **Verify Integrity**
   - Compare restored state against backup
   - Check all balances match
   - Verify no data loss

## Monitoring and Alerts

### Health Check Monitoring

Regularly monitor contract health:

```bash
# Check health every 5 minutes
*/5 * * * * stellar contract invoke --id $CONTRACT_ID --fn health_check --network $NETWORK | jq '.is_healthy'
```

Alert if:
- `is_healthy` is false
- `initialized` is false
- `yield_reserve_solvent` is false
- Any issues are present

### Backup Verification

Verify backups are valid:

```bash
# Weekly backup verification
0 0 * * 0 ./verify-backups.sh
```

Verify:
- All backup files exist
- Files are not corrupted
- Data can be parsed
- Timestamps are recent

## Testing Recovery Procedures

### Monthly Disaster Recovery Drill

1. Restore from backup to testnet
2. Verify all data matches
3. Test critical operations
4. Document any issues
5. Update procedures as needed

### Backup Restoration Test

```bash
#!/bin/bash
# test-backup-restore.sh

# Restore latest backup
LATEST_BACKUP=$(ls -t backups/contract_state/config_*.json | head -1)
echo "Testing restore from: $LATEST_BACKUP"

# Parse and verify
jq . $LATEST_BACKUP > /dev/null || exit 1
echo "Backup is valid JSON"

# Check required fields
jq '.admins, .admin_threshold, .token' $LATEST_BACKUP > /dev/null || exit 1
echo "Backup contains required fields"

echo "Backup restoration test passed"
```

## Checklist

- [ ] Automated backup script deployed
- [ ] Backup storage configured
- [ ] Backup verification script deployed
- [ ] Disaster recovery runbook reviewed
- [ ] Team trained on recovery procedures
- [ ] Monthly disaster recovery drills scheduled
- [ ] Monitoring and alerts configured
- [ ] Off-chain database backups enabled
- [ ] Cold storage backups configured
- [ ] Recovery procedures tested on testnet
