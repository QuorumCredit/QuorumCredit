# QuorumCredit Disaster Recovery Plan

## Document Information

| Item | Value |
|------|-------|
| **Version** | 1.0 |
| **Last Updated** | 2024-09-24 |
| **Maintained By** | DevOps Team |
| **Review Cycle** | Quarterly |
| **Next Review** | Q4 2024 |

---

## 1. Executive Summary

This Disaster Recovery (DR) Plan outlines the procedures, processes, and responsibilities for recovering QuorumCredit services in the event of a catastrophic failure. The plan aims to minimize downtime, data loss, and impact to users.

### Key Objectives

- Minimize Recovery Time Objective (RTO)
- Minimize Recovery Point Objective (RPO)
- Ensure data integrity during recovery
- Provide clear escalation paths
- Enable rapid decision-making

---

## 2. Recovery Targets

### Recovery Time Objective (RTO)

| Service Component | RTO | Priority |
|------------------|-----|----------|
| API Server | 15 minutes | Critical |
| Database | 30 minutes | Critical |
| Webhook Service | 30 minutes | High |
| Dashboard/Frontend | 1 hour | Medium |
| Monitoring/Logging | 4 hours | Low |
| Full System | 2 hours | Critical |

### Recovery Point Objective (RPO)

| Service Component | RPO | Strategy |
|------------------|-----|----------|
| Blockchain Data | Real-time | Replicated from Stellar |
| User Database | 5 minutes | Continuous replication |
| Transaction Logs | 1 minute | Transaction log backups |
| Application Config | 15 minutes | Version control + backups |
| State Data | 5 minutes | Real-time replication |

---

## 3. Disaster Categories and Triggers

### Category 1: Data Center Failure
**Trigger:** Complete loss of primary data center connectivity
- All services unreachable from primary location
- Network connectivity down for > 5 minutes

**Response:** Activate failover to secondary region
**Estimated Recovery:** 15-30 minutes

### Category 2: Database Corruption
**Trigger:** Data integrity checks fail, corruption detected
- Automated alerts from monitoring system
- Failed transaction validation

**Response:** Restore from latest clean backup
**Estimated Recovery:** 30-60 minutes

### Category 3: Security Breach
**Trigger:** Unauthorized access detected
- Security alerts triggered
- API key compromise
- Unauthorized data access

**Response:** Immediate containment and forensic analysis
**Estimated Recovery:** 2-4 hours

### Category 4: Denial of Service (DoS)
**Trigger:** Abnormal traffic volume
- Traffic > 10x normal baseline
- CPU/Memory usage > 95% sustained for 5 minutes

**Response:** Enable rate limiting, activate DDoS mitigation
**Estimated Recovery:** 15-60 minutes

### Category 5: Service Degradation
**Trigger:** Multiple services experiencing issues
- Error rate > 5%
- API latency > 10 seconds (p99)
- Task queue backlog > 10,000 items

**Response:** Scale up resources, investigate root cause
**Estimated Recovery:** 5-30 minutes

### Category 6: Complete Data Loss
**Trigger:** All data unavailable, backup verification fails
- Database entirely inaccessible
- All replicas unavailable

**Response:** Restore from off-site backup
**Estimated Recovery:** 4-8 hours

---

## 4. Disaster Recovery Procedures

### 4.1 Data Center Failover (Category 1)

**Severity Level:** Critical  
**Estimated Duration:** 15-30 minutes

#### Prerequisites
- Secondary data center/region configured and tested
- Database replication active
- DNS failover configured
- Load balancer with health checks active

#### Step-by-Step Procedure

1. **Detection (0-5 min)**
   - Monitoring system detects primary region outage
   - Automated alerts sent to on-call engineer
   - PagerDuty escalation triggered

2. **Initial Assessment (5-10 min)**
   ```bash
   # Check primary region status
   $ kubectl get nodes -n production
   $ kubectl get pods -n production
   $ curl -s https://api.quorumcredit.xyz/health
   
   # Verify secondary region health
   $ kubectl --context secondary get nodes -n production
   $ kubectl --context secondary get pods -n production
   ```

3. **Verify Replication (10-12 min)**
   ```bash
   # Check database replication lag
   $ psql ${REPLICA_DB} -c "SELECT NOW() - pg_last_xact_replay_timestamp();"
   # Should be < 5 seconds
   ```

4. **DNS Failover (12-15 min)**
   ```bash
   # Update Route53 weighted routing policy
   # Shift 100% of traffic to secondary region
   $ aws route53 change-resource-record-sets \
     --hosted-zone-id Z123... \
     --change-batch file://failover.json
   ```

5. **Validation (15-20 min)**
   ```bash
   # Verify connectivity from secondary
   $ curl -s https://api.quorumcredit.xyz/health
   # Response: { "status": "ok" }
   
   # Check database connection
   $ psql $DB_URL -c "SELECT COUNT(*) FROM loans;"
   
   # Verify webhooks are queued
   $ psql $DB_URL -c "SELECT COUNT(*) FROM webhook_queue WHERE delivered_at IS NULL;"
   ```

6. **Notification (20-25 min)**
   - Update status page
   - Notify stakeholders
   - Create incident ticket
   - Begin root cause analysis

7. **Failback Planning (25-30 min)**
   - Assess primary region recovery time
   - Schedule failback if primary recovers
   - Document what went wrong

#### Rollback Procedure

```bash
# Once primary region is operational again
# 1. Verify primary region is healthy
$ kubectl --context primary get nodes
$ kubectl --context primary get pods -n production

# 2. Ensure databases are in sync
$ psql ${PRIMARY_DB} -c "SELECT slot_name, restart_lsn FROM pg_replication_slots;"

# 3. Route 50% traffic to primary
$ aws route53 change-resource-record-sets \
  --hosted-zone-id Z123... \
  --change-batch file://failback-50.json

# 4. Monitor for 10 minutes
# If stable, route 100% to primary

# 5. Final verification
$ curl -s https://api.quorumcredit.xyz/health
```

---

### 4.2 Database Recovery from Backup (Category 2)

**Severity Level:** Critical  
**Estimated Duration:** 30-60 minutes

#### Prerequisites
- Point-in-time backups enabled
- Backup testing performed monthly
- Restore runbook documented and tested
- Database credentials secured

#### Step-by-Step Procedure

1. **Stop Services (0-2 min)**
   ```bash
   # Scale down services to prevent writes
   $ kubectl scale deployment api-server --replicas=0 -n production
   $ kubectl scale deployment webhook-worker --replicas=0 -n production
   ```

2. **Identify Backup Point (2-5 min)**
   ```bash
   # List available backups
   $ aws rds describe-db-snapshots \
     --db-instance-identifier quorum-credit-prod
   
   # Select most recent backup before corruption detected
   # Note the snapshot identifier: rds:quorum-credit-prod-2024-09-24-12-00
   ```

3. **Restore to New Instance (5-25 min)**
   ```bash
   # Create new database instance from snapshot
   $ aws rds restore-db-instance-from-db-snapshot \
     --db-instance-identifier quorum-credit-prod-restore \
     --db-snapshot-identifier rds:quorum-credit-prod-2024-09-24-12-00 \
     --no-copy-tags-to-snapshot
   
   # Wait for restore to complete (check status)
   $ aws rds describe-db-instances \
     --db-instance-identifier quorum-credit-prod-restore \
     --query 'DBInstances[0].DBInstanceStatus'
   # Status should be: available
   ```

4. **Verify Data Integrity (25-35 min)**
   ```bash
   # Connect to restored instance
   $ psql postgresql://admin@quorum-credit-prod-restore.c9akciq32.us-east-1.rds.amazonaws.com/quorum -c "
   SELECT COUNT(*) as loan_count FROM loans;
   SELECT COUNT(*) as user_count FROM users;
   SELECT MAX(created_at) FROM transactions;
   "
   
   # Compare with known baseline
   # Run data validation queries
   $ psql postgresql://..../quorum < validation_queries.sql
   ```

5. **Promote Restored Instance (35-45 min)**
   ```bash
   # Update RDS endpoint alias
   $ aws route53 change-resource-record-sets \
     --hosted-zone-id Z123... \
     --change-batch file://database-failover.json
   
   # Or update application connection strings
   $ kubectl set env deployment/api-server \
     DB_HOST=quorum-credit-prod-restore.c9akciq32.us-east-1.rds.amazonaws.com \
     -n production
   ```

6. **Restart Services (45-50 min)**
   ```bash
   $ kubectl scale deployment api-server --replicas=3 -n production
   $ kubectl scale deployment webhook-worker --replicas=2 -n production
   $ kubectl rollout status deployment/api-server -n production
   ```

7. **Post-Recovery Steps (50-60 min)**
   - Verify all services are operational
   - Check webhook queue for missed events
   - Re-process pending transactions
   - Update monitoring dashboards
   - Document incident

---

### 4.3 Security Breach Response (Category 3)

**Severity Level:** Critical  
**Estimated Duration:** 2-4 hours

#### Step-by-Step Procedure

1. **Immediate Containment (0-10 min)**
   ```bash
   # Isolate affected systems
   $ kubectl delete pods -l app=api-server -n production
   
   # Revoke compromised credentials
   $ aws iam delete-access-key --access-key-id AKIAIOSFODNN7EXAMPLE
   
   # Enable read-only mode if partial compromise
   $ export API_READ_ONLY=true
   $ kubectl set env deployment/api-server API_READ_ONLY=true -n production
   ```

2. **Forensic Analysis (10-60 min)**
   ```bash
   # Retrieve logs from CloudWatch
   $ aws logs filter-log-events \
     --log-group-name /ecs/api-server \
     --filter-pattern "[ERROR, SECURITY]"
   
   # Check for unauthorized database access
   $ psql $DB_URL -c "
   SELECT * FROM pg_stat_statements 
   WHERE query LIKE '%DROP%' OR query LIKE '%DELETE%'
   ORDER BY query_start DESC LIMIT 10;
   "
   
   # List recent API access
   $ aws s3 cp s3://quorum-credit-logs/api-access.log - | tail -1000
   ```

3. **Revoke Compromised Data (60-120 min)**
   - Invalidate all active sessions
   - Force password reset for affected users
   - Invalidate API keys
   - Clear sensitive caches

4. **Apply Security Patch**
   ```bash
   # Deploy security patch
   $ git checkout security/fix-breach-1573
   $ npm run build && npm test
   $ docker build -t quorum-credit:secure-patch-1 .
   $ docker push quorum-credit:secure-patch-1
   
   # Update deployment
   $ kubectl set image deployment/api-server \
     api-server=quorum-credit:secure-patch-1 \
     -n production
   ```

5. **Restore Service (120-150 min)**
   ```bash
   $ export API_READ_ONLY=false
   $ kubectl set env deployment/api-server API_READ_ONLY=false -n production
   ```

---

### 4.4 DDoS/DoS Attack Response (Category 4)

**Severity Level:** High  
**Estimated Duration:** 15-60 minutes

#### Step-by-Step Procedure

1. **Enable DDoS Protection (0-5 min)**
   ```bash
   # Enable AWS Shield Advanced
   $ aws shield enable-drt-log-delivery \
     --role-arn arn:aws:iam::123456789:role/shield-logs
   
   # Activate rate limiting
   $ kubectl set env deployment/api-server \
     RATE_LIMIT_ENABLED=true \
     RATE_LIMIT_RPS=1000 \
     -n production
   ```

2. **Scale Resources (5-15 min)**
   ```bash
   # Auto-scale services
   $ kubectl autoscale deployment api-server \
     --min=5 --max=20 -n production
   
   # Monitor scaling
   $ watch 'kubectl get hpa -n production'
   ```

3. **Activate WAF Rules (15-25 min)**
   ```bash
   # Update AWS WAF rules
   $ aws wafv2 update-web-acl \
     --id <acl-id> \
     --region us-east-1 \
     --default-action Type=BLOCK \
     --rules file://waf-rules.json
   ```

4. **Monitor Attack (25-60 min)**
   ```bash
   # Check CloudWatch metrics
   $ aws cloudwatch get-metric-statistics \
     --namespace AWS/ApplicationELB \
     --metric-name TargetResponseTime \
     --start-time 2024-09-24T12:00:00Z \
     --end-time 2024-09-24T13:00:00Z \
     --period 60 \
     --statistics Average
   
   # When attack subsides, gradually normalize
   $ kubectl set env deployment/api-server \
     RATE_LIMIT_RPS=10000 \
     -n production
   ```

---

### 4.5 Backup Recovery Runbook (Category 6)

**Severity Level:** Critical  
**Estimated Duration:** 4-8 hours

#### Step-by-Step Procedure

1. **Locate Latest Backup (0-10 min)**
   ```bash
   # Check S3 backups
   $ aws s3 ls s3://quorum-credit-backups/ --recursive --human-readable --summarize
   
   # Verify backup integrity
   $ aws s3api head-object \
     --bucket quorum-credit-backups \
     --key daily/2024-09-24/database.sql.gz \
     --region us-east-1
   ```

2. **Set Up Restore Environment (10-30 min)**
   ```bash
   # Create temporary restore instance
   $ docker run -d \
     --name db-restore \
     -e POSTGRES_PASSWORD=temporary-password \
     -p 5432:5432 \
     postgres:15
   
   # Wait for startup
   $ sleep 30
   ```

3. **Download and Restore Backup (30-120 min)**
   ```bash
   # Download backup
   $ aws s3 cp \
     s3://quorum-credit-backups/daily/2024-09-24/database.sql.gz \
     ./database.sql.gz
   
   # Restore database
   $ gunzip -c database.sql.gz | psql postgresql://localhost/quorum
   
   # Verify restore
   $ psql postgresql://localhost/quorum -c \
     "SELECT COUNT(*) FROM loans; SELECT MAX(updated_at) FROM transactions;"
   ```

4. **Sync Blockchain State (120-240 min)**
   ```bash
   # Fetch latest data from Stellar
   $ node scripts/resync-blockchain-state.js --from-date 2024-09-24
   
   # Verify data consistency
   $ npm run validate-blockchain-sync
   ```

5. **Promote Restored Database (240-270 min)**
   - Create RDS instance from restore
   - Update connection strings
   - Restart services

---

## 5. Backup Strategy

### Backup Schedule

| Backup Type | Frequency | Retention | Location |
|-------------|-----------|-----------|----------|
| Automated Snapshots | Hourly | 7 days | AWS RDS |
| Daily Backups | Daily (2 AM UTC) | 30 days | S3 (encrypted) |
| Weekly Backups | Weekly (Sunday 2 AM) | 13 weeks | S3 + Glacier |
| Monthly Backups | Monthly (1st day) | 12 months | Glacier |
| Continuous Replication | Real-time | 5 minute lag | Secondary Region |

### Backup Locations

- **Primary:** AWS RDS automated backups (US East 1)
- **Secondary:** AWS S3 (US West 2) - encrypted with KMS
- **Tertiary:** AWS Glacier (cross-region)
- **Real-time Replica:** Secondary RDS instance (ready standby)

### Backup Verification

```bash
#!/bin/bash
# Daily backup verification script

# Verify latest S3 backup exists and is valid
LATEST_BACKUP=$(aws s3 ls s3://quorum-credit-backups/daily/ | tail -1 | awk '{print $NF}')
SIZE=$(aws s3api head-object --bucket quorum-credit-backups --key daily/$LATEST_BACKUP | jq '.ContentLength')

if [ "$SIZE" -lt 1000000 ]; then
  echo "ERROR: Backup too small: $SIZE bytes"
  exit 1
fi

# Test restore to temporary instance
psql postgresql://backup-test-db -c "RESTORE DATABASE FROM BACKUP '$LATEST_BACKUP';"

# Validate data
psql postgresql://backup-test-db -c "SELECT COUNT(*) FROM loans;" | grep -q "^[0-9]\+$"
if [ $? -ne 0 ]; then
  echo "ERROR: Backup validation failed"
  exit 1
fi

echo "OK: Backup verified successfully"
```

---

## 6. Testing and Validation

### Monthly Recovery Test

Every month, we perform a full recovery test:

1. **Database Recovery Test**
   - Restore database from latest backup
   - Verify data integrity
   - Run validation queries
   - Clean up test instance

2. **Failover Test**
   - Test DNS failover to secondary region
   - Verify services come up
   - Run smoke tests
   - Failback to primary

3. **Documentation Update**
   - Update this plan if procedures changed
   - Note any issues encountered
   - Update time estimates if needed

### Test Schedule

| Test | Frequency | Owner | Duration |
|------|-----------|-------|----------|
| Database Restore | Monthly (2nd Monday) | Database Team | 2 hours |
| Regional Failover | Quarterly (mid-month) | DevOps Team | 1 hour |
| Full DR Drill | Annually (Q1) | All Teams | 4 hours |

---

## 7. Communication Plan

### Notification Flow

```
Monitoring Alert
    ↓
On-Call Engineer Notified (PagerDuty)
    ↓
[If critical severity]
    ├→ VP Engineering (SMS)
    ├→ Product Manager (Email)
    └→ Customer Success (Slack)
    ↓
[Every 15 minutes]
Status Update to Slack #incidents
    ↓
When Resolved:
    ├→ Update Status Page
    ├→ Send Customer Notification
    └→ Create Incident Post-mortem
```

### Status Page Updates

All outages > 5 minutes are posted to status page:
- **URL:** https://status.quorumcredit.io
- **Update frequency:** Every 15 minutes during outage
- **Post-incident:** Root cause analysis posted within 24 hours

---

## 8. Key Contacts

| Role | Name | Email | Phone |
|------|------|-------|-------|
| Head of DevOps | TBD | devops@quorumcredit.io | +1-555-0100 |
| On-Call Engineer | Rotated | oncall@quorumcredit.io | Check PagerDuty |
| Database Administrator | TBD | dba@quorumcredit.io | +1-555-0101 |
| Security Officer | TBD | security@quorumcredit.io | +1-555-0102 |
| VP Engineering | TBD | vp-eng@quorumcredit.io | +1-555-0103 |

---

## 9. Appendix: Command Reference

### Quick Health Check

```bash
#!/bin/bash
# Quick system health check

echo "=== API Health ==="
curl -s https://api.quorumcredit.xyz/health | jq .

echo "=== Database ==="
psql $DB_URL -c "SELECT version();"

echo "=== Kubernetes ==="
kubectl get nodes,pods,svc -n production

echo "=== Recent Errors ==="
kubectl logs deployment/api-server -n production --tail=50 | grep ERROR
```

### Emergency Contacts

For emergencies, use:
- **PagerDuty:** incident@pagerduty.com
- **Slack:** #incidents channel
- **War Room:** https://zoom.us/j/disasters
- **Backup Phone Tree:** Check SLACK_EMERGENCY_PIN

---

## 10. Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2024-09-24 | Initial document creation |

---

## Acknowledgments

This DR plan is based on industry best practices and has been reviewed by:
- DevOps Team
- Database Team
- Security Team
- Product Management
