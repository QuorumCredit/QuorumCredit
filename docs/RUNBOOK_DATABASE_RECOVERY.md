# Runbook: Database Recovery from Backup

**Time Estimate:** 30-60 minutes  
**Severity:** Critical  
**Last Updated:** 2024-09-24

## Quick Reference

1. Detect corruption
2. Stop write operations
3. Restore from backup
4. Verify data integrity
5. Promote restored instance

---

## Detailed Steps

### Step 1: Detect Data Corruption (0-2 min)

```bash
# Alert symptoms:
# - Transaction validation failing
# - Duplicate key errors
# - Foreign key constraint violations
# - Unexpected NULL values

# Verify corruption
psql $DB_URL -c "
SELECT * FROM loans WHERE balance < 0;
SELECT * FROM pg_stat_user_tables WHERE n_dead_tup > n_live_tup * 2;
"

# If corrupted, proceed with recovery
```

### Step 2: Stop All Write Operations (2-5 min)

```bash
# Scale down services to read-only mode
kubectl scale deployment api-server --replicas=0 -n production
kubectl scale deployment webhook-worker --replicas=0 -n production

# Verify no connections to database
psql $DB_URL -c "
SELECT usename, application_name, state, COUNT(*) 
FROM pg_stat_activity 
WHERE datname = 'quorum' 
GROUP BY usename, application_name, state;
"

# Should show only 0-1 connections (monitoring)
```

### Step 3: Identify Backup Point (5-10 min)

```bash
# List available RDS snapshots
aws rds describe-db-snapshots \
  --db-instance-identifier quorum-credit-prod \
  --query 'DBSnapshots[*].[DBSnapshotIdentifier, SnapshotCreateTime, DBInstanceIdentifier]' \
  --output table

# Choose snapshot BEFORE corruption was detected
# Example: rds:quorum-credit-prod-2024-09-24-08-00

SNAPSHOT_ID="rds:quorum-credit-prod-2024-09-24-08-00"
```

### Step 4: Restore to New Instance (10-35 min)

```bash
# Create restore instance
aws rds restore-db-instance-from-db-snapshot \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --db-snapshot-identifier $SNAPSHOT_ID \
  --no-copy-tags-to-snapshot \
  --publicly-accessible false

# Monitor restore progress
watch -n 10 "aws rds describe-db-instances \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --query 'DBInstances[0].DBInstanceStatus'"

# Status: creating → backing-up → available
# This takes 15-20 minutes typically
```

### Step 5: Verify Data Integrity (35-45 min)

```bash
# Wait for restore to complete and get endpoint
RESTORE_ENDPOINT=$(aws rds describe-db-instances \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --query 'DBInstances[0].Endpoint.Address' \
  --output text)

# Connect to restored database
psql postgresql://admin:PASSWORD@$RESTORE_ENDPOINT/quorum -c "
-- Count records
SELECT 'loans' as table_name, COUNT(*) as row_count FROM loans
UNION ALL
SELECT 'users', COUNT(*) FROM users
UNION ALL
SELECT 'transactions', COUNT(*) FROM transactions
UNION ALL
SELECT 'vouches', COUNT(*) FROM vouches;

-- Check for data corruption indicators
SELECT * FROM loans WHERE balance < 0;
SELECT * FROM loans WHERE status NOT IN ('pending', 'active', 'repaid', 'defaulted');
SELECT * FROM transactions WHERE amount < 0 AND type != 'reversal';
"

# Expected output: All corruption checks should return empty

# Run comprehensive validation
psql postgresql://admin:PASSWORD@$RESTORE_ENDPOINT/quorum < /path/to/validation_queries.sql

# If all checks pass:
echo "DATA INTEGRITY: OK"
```

### Step 6: Promote Restored Instance (45-55 min)

```bash
# Option A: Update connection string in app secrets
kubectl set env deployment/api-server \
  DB_HOST=$RESTORE_ENDPOINT \
  -n production

# Option B: Update RDS CNAME
aws route53 change-resource-record-sets \
  --hosted-zone-id Z123EXAMPLE \
  --change-batch '{
    "Changes": [{
      "Action": "UPSERT",
      "ResourceRecordSet": {
        "Name": "quorum-db-prod.internal",
        "Type": "CNAME",
        "TTL": 60,
        "ResourceRecords": [{
          "Value": "'$RESTORE_ENDPOINT'"
        }]
      }
    }]
  }'

# Update parameter group if needed (minimum downtime)
aws rds modify-db-instance \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --db-parameter-group-name quorum-credit-prod \
  --apply-immediately

# Reboot if parameter changes applied
aws rds reboot-db-instance \
  --db-instance-identifier quorum-credit-prod-restore-20240924
```

### Step 7: Restart Services (55-60 min)

```bash
# Start API server
kubectl scale deployment api-server --replicas=3 -n production
kubectl rollout status deployment/api-server -n production

# Start webhook worker
kubectl scale deployment webhook-worker --replicas=2 -n production

# Verify services are healthy
curl -s https://api.quorumcredit.xyz/health | jq .

# Check recent logs for errors
kubectl logs deployment/api-server -n production --tail=50
```

---

## Clean Up Old Instance (After Verification)

```bash
# Wait 24 hours to ensure restored instance is stable

# Take final snapshot of restored instance
aws rds create-db-snapshot \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --db-snapshot-identifier quorum-credit-prod-recovered-20240924

# Delete corrupted instance
aws rds delete-db-instance \
  --db-instance-identifier quorum-credit-prod \
  --skip-final-snapshot

# Rename restored instance to production name
aws rds modify-db-instance \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --new-db-instance-identifier quorum-credit-prod \
  --apply-immediately

# Update Route53 and connection strings to point to new instance
aws rds describe-db-instances \
  --db-instance-identifier quorum-credit-prod \
  --query 'DBInstances[0].Endpoint.Address'
```

---

## Troubleshooting

### Restore Stuck in "Creating" State

```bash
# Check RDS event logs
aws rds describe-events \
  --source-identifier quorum-credit-prod-restore-20240924 \
  --source-type db-instance \
  --query 'Events[-10:].[EventCategories, SourceArn, Message]'

# If stuck, cancel and retry
aws rds delete-db-instance \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --skip-final-snapshot

# Wait 5 minutes, then retry restore
```

### Cannot Connect to Restored Instance

```bash
# Verify instance is available
aws rds describe-db-instances \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --query 'DBInstances[0].DBInstanceStatus'

# Check security group allows connections
aws ec2 describe-security-groups \
  --group-ids sg-xyz123 \
  --query 'SecurityGroups[0].IpPermissions'

# Add inbound rule if needed
aws ec2 authorize-security-group-ingress \
  --group-id sg-xyz123 \
  --protocol tcp \
  --port 5432 \
  --cidr 10.0.0.0/8
```

### Restore Endpoint is Empty

```bash
# Wait for RDS to fully initialize
sleep 60

# Force fetch endpoint
aws rds describe-db-instances \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --query 'DBInstances[0].Endpoint'

# If still empty, check instance status
aws rds describe-db-instances \
  --db-instance-identifier quorum-credit-prod-restore-20240924 \
  --query 'DBInstances[0].[DBInstanceStatus, AvailabilityZone]'
```

---

## Validation Queries

Create `/path/to/validation_queries.sql`:

```sql
-- Table row counts
SELECT COUNT(*) as total_loans FROM loans;
SELECT COUNT(*) as total_users FROM users;
SELECT COUNT(*) as total_transactions FROM transactions;

-- Check constraints
SELECT * FROM loans WHERE balance < 0 OR balance > principal;
SELECT * FROM users WHERE created_at > NOW();
SELECT * FROM transactions WHERE status NOT IN ('pending', 'complete', 'failed');

-- Referential integrity
SELECT COUNT(*) FROM loans WHERE user_id NOT IN (SELECT id FROM users);
SELECT COUNT(*) FROM transactions WHERE loan_id NOT IN (SELECT id FROM loans);

-- Latest data
SELECT MAX(updated_at) FROM loans;
SELECT MAX(created_at) FROM transactions;

-- Size check
SELECT pg_size_pretty(pg_database_size('quorum'));
```

---

## Success Criteria

Recovery is complete when:

- [ ] All data integrity checks pass
- [ ] Row counts match expected baseline
- [ ] No constraint violations
- [ ] API responds normally
- [ ] Webhook processing resumes
- [ ] Error rate < 0.1%

---

## Prevention

To prevent future corruption:

1. **Enable automatic backups** (daily + continuous replication)
2. **Schedule validation checks** (weekly integrity validation)
3. **Monitor replication lag** (alert if > 5 minutes)
4. **Test restores monthly** (to catch backup issues early)
5. **Track database changes** (for forensic analysis)

```bash
# Enable automated validation
0 2 * * 0 /usr/local/bin/validate-database-integrity.sh
```
