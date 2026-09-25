# Runbook: Data Center Failover

**Time Estimate:** 15-30 minutes  
**Severity:** Critical  
**Last Updated:** 2024-09-24

## Quick Reference

1. Verify primary is down
2. Check secondary region health
3. Execute DNS failover
4. Validate services
5. Notify stakeholders

---

## Detailed Steps

### Step 1: Detect Outage (0-5 min)

```bash
# Verify primary region is unreachable
curl -m 5 https://api.quorumcredit.xyz/health || echo "PRIMARY DOWN"

# Check health of secondary
curl -m 5 https://api-secondary.quorumcredit.xyz/health
# Should return: { "status": "ok" }

# Verify Kubernetes cluster status
kubectl --context production get nodes
# Should show: NotReady or Connection refused

kubectl --context secondary get nodes
# Should show: Ready
```

### Step 2: Assess Replication Lag (5-10 min)

```bash
# SSH to secondary database
ssh ubuntu@db-secondary.quorumcredit.io

# Check replication status
psql -h localhost -U admin -d quorum -c "
SELECT 
  slot_name,
  restart_lsn,
  confirmed_flush_lsn,
  pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn) AS bytes_behind
FROM pg_replication_slots;
"
# bytes_behind should be < 100000 (100KB)
```

### Step 3: Execute Failover (10-15 min)

```bash
# Login to AWS CLI
aws configure

# Update Route53 DNS
aws route53 change-resource-record-sets \
  --hosted-zone-id Z123EXAMPLE \
  --change-batch '{
    "Changes": [
      {
        "Action": "UPSERT",
        "ResourceRecordSet": {
          "Name": "api.quorumcredit.xyz",
          "Type": "A",
          "SetIdentifier": "Secondary",
          "FailoverRoutingPolicy": {
            "Type": "SECONDARY"
          },
          "AliasTarget": {
            "HostedZoneId": "Z35SXDOTRQ7X7K",
            "DNSName": "api-secondary-elb.us-west-2.elb.amazonaws.com",
            "EvaluateTargetHealth": true
          }
        }
      }
    ]
  }'

# Wait for DNS to propagate (2-3 minutes)
nslookup api.quorumcredit.xyz
# Should resolve to secondary region IP
```

### Step 4: Verify Services (15-20 min)

```bash
# Test API endpoint
curl -s https://api.quorumcredit.xyz/health | jq .
# Response: { "status": "ok" }

# Test authentication
curl -s -X POST https://api.quorumcredit.xyz/api/auth/token \
  -H "Content-Type: application/json" \
  -d '{"apiKey": "test_key"}' | jq .

# Check database connectivity
psql postgresql://admin@db-secondary.quorumcredit.io/quorum -c "SELECT COUNT(*) FROM loans;"

# Verify webhook processing
kubectl --context secondary get pods -n production | grep webhook

# Check recent logs
kubectl --context secondary logs deployment/api-server -n production --tail=50
```

### Step 5: Notify Stakeholders (20-25 min)

```bash
# Post to Slack #incidents
slack send-message "#incidents" "FAILOVER COMPLETE: Traffic now routing to secondary region. Primary region under investigation."

# Update status page
# Go to https://manage.quorumcredit.io/incidents/new
# Title: "Failover to Secondary Region"
# Status: Investigating

# Notify VPs via email
mail vp-eng@quorumcredit.io -s "FAILOVER EXECUTED"
```

### Step 6: Begin Failback Planning (25-30 min)

```bash
# Monitor primary region recovery
watch -n 30 'aws ec2 describe-instance-status --instance-ids i-xyz123 --query "InstanceStatuses[0].InstanceStatus.Status"'

# Once primary recovers:
# 1. Verify replication is caught up
# 2. Update Route53 to prefer primary
# 3. Monitor for 30 minutes
# 4. If stable, update DNS to exclusive primary routing
```

---

## Rollback (When Primary Recovers)

```bash
# Verify primary region is healthy
kubectl --context production get nodes
# All nodes should be Ready

# Check database replication
ssh ubuntu@db-primary.quorumcredit.io
psql -c "SELECT pg_last_xact_replay_timestamp();"

# Route 50% traffic to primary (canary)
aws route53 change-resource-record-sets \
  --hosted-zone-id Z123EXAMPLE \
  --change-batch file://dns-50-primary.json

# Monitor for 15 minutes
watch -n 30 'curl -s https://api.quorumcredit.xyz/health | jq .'

# If stable, route 100% to primary
aws route53 change-resource-record-sets \
  --hosted-zone-id Z123EXAMPLE \
  --change-batch file://dns-100-primary.json
```

---

## Troubleshooting

### DNS Not Propagating

```bash
# Force refresh
dig @8.8.8.8 api.quorumcredit.xyz +short
dig @1.1.1.1 api.quorumcredit.xyz +short

# If different results, wait and retry
# If still wrong, check Route53:
aws route53 list-resource-record-sets --hosted-zone-id Z123EXAMPLE | jq '.ResourceRecordSets[] | select(.Name == "api.quorumcredit.xyz.")'
```

### Secondary Services Not Responding

```bash
# Check if pods are running
kubectl --context secondary get pods -n production

# Check resource usage
kubectl --context secondary top nodes
kubectl --context secondary top pods -n production

# Check recent errors
kubectl --context secondary describe pod <pod-name> -n production
kubectl --context secondary logs <pod-name> -n production --tail=100
```

### Database Replication Behind

```bash
# Check lag
psql -c "SELECT NOW() - pg_last_xact_replay_timestamp();"

# If > 5 minutes, wait before failover
# Check for long-running transactions on primary:
ssh ubuntu@db-primary.quorumcredit.io
psql -c "SELECT pid, usename, query_start, query FROM pg_stat_activity WHERE state != 'idle' ORDER BY query_start;"
```

---

## Success Criteria

Failover is complete when:

- [ ] API responds with `{ "status": "ok" }`
- [ ] Database queries execute successfully
- [ ] Webhook events are being processed
- [ ] Error rate < 1%
- [ ] Response times < 500ms (p99)
- [ ] All Kubernetes pods are running

---

## Post-Incident

After failover:
1. Create incident post-mortem
2. Review root cause
3. Update monitoring alerts
4. Schedule platform improvements
5. Update this runbook if needed
