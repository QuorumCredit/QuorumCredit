# Incident Response Procedures

This guide documents how to respond to incidents affecting QuorumCredit production systems. Incidents range from minor bugs to critical security issues.

## Table of Contents

1. [Incident Classification](#incident-classification)
2. [Incident Declaration](#incident-declaration)
3. [Response Procedures](#response-procedures)
4. [Communication Protocol](#communication-protocol)
5. [Post-Incident Review](#post-incident-review)
6. [Recovery Time Objectives](#recovery-time-objectives)

---

## Incident Classification

Incidents are classified by severity, impact, and required response time:

| Level | Severity | Impact | Response Time | Example |
|-------|----------|--------|---------------|---------|
| **P1** | Critical | Complete outage or security breach | Immediate (< 5 min) | Contract compromised, funds locked, mass slashing |
| **P2** | High | Major functionality broken or data corruption | 1 hour | High error rate (>1%), users unable to request loans, vouch system down |
| **P3** | Medium | Degraded performance or partial outage | 4 hours | Slow response times, some transactions fail, yield calculation incorrect |
| **P4** | Low | Minor issues, workarounds available | 24 hours | UI bugs, documentation errors, non-critical features affected |

### Severity Indicators

**P1 Indicators:**
- Contract is paused or unresponsive
- Users report fund loss or inability to withdraw
- Security vulnerability detected in production
- Multiple critical services failing (RPC, database, monitoring)
- Data corruption or state inconsistency detected

**P2 Indicators:**
- Error rate > 1% for >10 minutes
- Core features (loans, vouches, repayment) broken for >30% of users
- Database replication lag > 1 minute
- One critical service degraded but not fully down

**P3 Indicators:**
- Response time > 2s for >30% of requests
- Non-critical features unavailable
- Monitoring alerts firing frequently
- Performance degradation but operations continue

**P4 Indicators:**
- Single user affected by non-critical issue
- Documentation gaps
- Low-priority bug reports
- Cosmetic UI issues

---

## Incident Declaration

### Who Can Declare an Incident?

- On-call engineer
- Monitoring system (automated alerts)
- User reports escalated by support
- Product manager or team lead

### How to Declare

1. **Create incident ticket**
   ```bash
   # Use incident template
   gh issue create \
     --title "[INCIDENT] <Brief Description>" \
     --label "incident,p1" \
     --assignee @on-call-engineer
   ```

2. **Notify team immediately**
   - Post in #incidents Slack channel
   - Mention @on-call-team
   - Include severity level and brief description

3. **Start incident clock**
   - Record start time (UTC)
   - Assign incident ID (auto-generated)
   - Document initial observations

### Incident Declaration Template

```
🚨 INCIDENT DECLARED

ID: INC-2026-0001
Severity: P1 (Critical)
Time: 2026-06-29T14:32:00Z
Component: Contract Core
Status: Investigating

Description:
Contract responding with timeout errors after upgrade at 2026-06-29T14:30:00Z

Impact:
- Users unable to request loans
- Estimated affected: 100+ active users
- Business impact: High (lending is blocked)

Initial Observations:
- Contract RPC timeout after 15 seconds
- Logs show "CallFailed" errors
- Database query latency normal
- Network connectivity normal
```

---

## Response Procedures

### P1 (Critical) Response

**Timeline: 0-5 minutes**

1. **Declare emergency** - Follow declaration process above
2. **Assemble team** - Page on-call engineer and tech lead immediately
3. **Pause contract** - Execute emergency pause if affecting fund safety
4. **Preserve evidence** - Capture logs, metrics, database state

```bash
#!/bin/bash
# P1 Emergency Response Checklist

CONTRACT_ID=$1

# 1. Pause contract immediately
./scripts/pause_contract.sh $CONTRACT_ID

# 2. Capture logs (last 1 hour)
docker logs --since 1h api-service > logs/incident_api_$(date +%s).txt
docker logs --since 1h contract-monitor > logs/incident_monitor_$(date +%s).txt

# 3. Export current metrics
curl -s http://prometheus:9090/api/v1/query_range \
  --data-urlencode 'query=qc_contract_errors_total' \
  --data-urlencode 'start='$(date -d '1 hour ago' +%s) \
  --data-urlencode 'end='$(date +%s) \
  --data-urlencode 'step=60' > metrics/error_rate_$(date +%s).json

# 4. Document system state
stellar account info $CONTRACT_ID --network mainnet > state/contract_account_$(date +%s).txt
stellar contract info --id $CONTRACT_ID --network mainnet > state/contract_info_$(date +%s).txt
```

**Timeline: 5-30 minutes**

1. **Investigate root cause** - Check logs, metrics, recent changes
2. **Assess damage** - Determine impact scope and affected users
3. **Prepare fix** - Code fix, configuration change, or rollback
4. **Verify fix on testnet** - Test before production deployment
5. **Communicate** - Update status to team and customers

```bash
#!/bin/bash
# P1 Investigation Checklist

CONTRACT_ID=$1

# Check recent deployments
echo "=== Recent Deployments ==="
git log --oneline -n 20

# Check error logs
echo "=== Recent Errors ==="
docker logs api-service --tail 100 | grep -i error

# Check contract state
echo "=== Contract State ==="
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network testnet

# Analyze metrics
echo "=== Error Rate (last 5 min) ==="
curl -s 'http://prometheus:9090/api/v1/query' \
  --data-urlencode 'query=rate(qc_contract_errors_total[5m])'

# Check dependencies
echo "=== Database Status ==="
curl -s http://db-monitor:8080/health

echo "=== RPC Status ==="
curl -s -X POST https://rpc.mainnet.stellar.org:443 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc": "2.0", "id": 1, "method": "getHealth"}'
```

**Timeline: 30+ minutes**

1. **Deploy fix** - To testnet, verify, then production
2. **Unpause and verify** - Resume operations and run smoke tests
3. **Monitor closely** - Watch metrics for 30 minutes
4. **Communicate resolution** - Notify team and customers

### P2 (High) Response

**Timeline: Within 1 hour**

1. **Declare incident** - Standard declaration process
2. **Investigate** - Identify root cause within 30 minutes
3. **Prepare fix** - Code change or configuration update
4. **Test on testnet** - Verify fix before deployment
5. **Deploy to production** - During business hours if possible
6. **Verify and monitor** - Confirm fix effectiveness

### P3 (Medium) Response

**Timeline: Within 4 hours**

1. **Create ticket** - Document issue and impact
2. **Schedule investigation** - Next business day if off-hours
3. **Plan fix** - Can be batched with other P3 items
4. **Deploy during maintenance window** - Coordinated with users

### P4 (Low) Response

**Timeline: Within 24 hours**

1. **Create bug report** - Standard issue creation
2. **Plan fix** - Next sprint or maintenance window
3. **Deploy** - Per normal release schedule

---

## Communication Protocol

### Incident Notifications

**Channels** (by priority):

1. **#incidents** - Real-time incident updates
2. **Email** - Status notifications to stakeholders
3. **Status page** - Customer-facing updates
4. **Slack** - Direct DMs to affected users if needed

### Message Templates

**Initial Declaration**
```
🚨 INCIDENT DECLARED - P1

Service: QuorumCredit Contract
Issue: [Brief description]
Status: Investigating
Started: [UTC time]

We are actively investigating. More updates in 15 minutes.
```

**Investigating Update**
```
🔍 INVESTIGATING

Root cause: [Brief explanation]
Impact: [Users affected, operations affected]
ETA: [Estimated time to resolution]

Next update in 10 minutes.
```

**Resolution**
```
✅ RESOLVED

Root cause: [What caused the incident]
Fix: [What was done to fix it]
Impact: [Actual downtime and affected users]
Next steps: [Post-incident review scheduled for X]

Thank you for your patience.
```

### Update Frequency

- **P1**: Every 5-10 minutes
- **P2**: Every 15-30 minutes
- **P3**: Every 1 hour
- **P4**: Daily

---

## Recovery Procedures

### Service Restoration

#### Option 1: Configuration Revert

For configuration-related incidents (fee rates, thresholds):

```bash
#!/bin/bash
# Revert configuration to last known good state

CONTRACT_ID=$1

# Get last known good config
LAST_GOOD_CONFIG=$(cat deployment/last_good_config.json)

# Pause contract
./scripts/pause_contract.sh $CONTRACT_ID

# Update configuration
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn update_config \
  --network mainnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --yield_bps $(echo $LAST_GOOD_CONFIG | jq .yield_bps) \
  --slash_bps $(echo $LAST_GOOD_CONFIG | jq .slash_bps)

# Unpause and verify
./scripts/unpause_contract.sh $CONTRACT_ID
sleep 5
./scripts/smoke_tests.sh $CONTRACT_ID
```

#### Option 2: Contract Rollback

For code-related incidents requiring rollback:

```bash
#!/bin/bash
# Rollback contract to previous version

CONTRACT_ID=$1
PREVIOUS_WASM_HASH=$2

# 1. Pause
./scripts/pause_contract.sh $CONTRACT_ID

# 2. Rollback WASM
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn upgrade \
  --network mainnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_1'","'$ADMIN_2'"]' \
  --new_wasm_hash $PREVIOUS_WASM_HASH

# 3. Verify rollback
sleep 10
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_config \
  --network mainnet

# 4. Run smoke tests
./scripts/smoke_tests.sh $CONTRACT_ID

# 5. If successful, unpause
./scripts/unpause_contract.sh $CONTRACT_ID
```

#### Option 3: Data Recovery

For data corruption incidents:

```bash
#!/bin/bash
# Restore contract state from backup

CONTRACT_ID=$1
BACKUP_DATE=$2  # Format: YYYY-MM-DD

# 1. Pause contract
./scripts/pause_contract.sh $CONTRACT_ID

# 2. Download backup
aws s3 cp s3://quorum-credit-backups/$BACKUP_DATE.tar.gz.enc \
  backups/restore_$BACKUP_DATE.tar.gz.enc

# 3. Decrypt
openssl enc -aes-256-cbc -d \
  -in backups/restore_$BACKUP_DATE.tar.gz.enc \
  -pass env:BACKUP_ENCRYPTION_KEY | \
  tar xz -C backups/

# 4. Restore state
# NOTE: This requires contract-specific restore logic
# and must be carefully tested on testnet first
./scripts/restore_state.sh $CONTRACT_ID backups/$BACKUP_DATE/

# 5. Verify restored state
./scripts/verify_state.sh $CONTRACT_ID

# 6. Unpause and monitor
./scripts/unpause_contract.sh $CONTRACT_ID
```

---

## Post-Incident Review

Within 24-48 hours of resolving a P1 or P2 incident, conduct a post-incident review.

### Review Meeting

**Attendees:**
- On-call engineer
- Engineer who deployed the change
- Tech lead
- Product manager
- QA lead (if relevant)

**Duration:** 30-60 minutes

### Review Document Template

```markdown
# Post-Incident Review: INC-2026-0001

## Incident Summary

**Title:** Contract timeout after upgrade
**Severity:** P1 (Critical)
**Start Time:** 2026-06-29T14:30:00Z
**End Time:** 2026-06-29T15:15:00Z
**Duration:** 45 minutes
**Impact:** Lending blocked for 100+ users

## Root Cause Analysis

### Timeline

- 14:25:00 - Upgrade initiated
- 14:30:00 - Users report loan request failures
- 14:32:00 - P1 declared
- 14:35:00 - Contract paused
- 14:50:00 - Root cause identified: New WASM had infinite loop in validation
- 15:00:00 - Previous WASM deployed and verified on testnet
- 15:15:00 - Rollback to production, services restored

### Root Cause

The new WASM build included an untested code path that caused an infinite loop in loan validation. This code path was triggered during normal loan request processing.

**Why did this happen?**
- Code path was added in last commit but not tested
- Integration tests didn't cover this scenario
- Pre-upgrade testing on testnet used mock data that didn't trigger the path

## Lessons Learned

### What Went Well

1. ✅ Monitoring detected issue within 2 minutes
2. ✅ Team responded quickly (declared in 2 minutes)
3. ✅ Rollback tested and executed within 15 minutes
4. ✅ Good communication with users

### What Could Be Better

1. ❌ Code review missed untested path (add peer review checklist)
2. ❌ Pre-upgrade testing insufficient (improve test coverage)
3. ❌ No integration test for new validation logic
4. ❌ No feature flag for gradual rollout

## Action Items

| Action | Owner | Deadline | Priority |
|--------|-------|----------|----------|
| Add integration test for loan validation edge cases | Alice | 2026-07-06 | High |
| Implement code review checklist for new code paths | Bob | 2026-07-02 | High |
| Add feature flags for gradual rollout | Carol | 2026-07-13 | Medium |
| Review test coverage on all recent commits | Alice | 2026-07-03 | High |
| Document pre-upgrade testing requirements | Bob | 2026-07-05 | Medium |

## Prevention Measures

1. **Code Review Checklist**
   - [ ] All new code paths tested?
   - [ ] Edge cases covered?
   - [ ] Feature flags included?
   - [ ] Integration tests added?

2. **Pre-Upgrade Testing**
   - Run against real testnet data
   - Stress test with high transaction volume
   - Test all recent code changes
   - Run for 24+ hours without errors

3. **Deployment Strategy**
   - Use feature flags for new features
   - Gradual rollout (1% → 10% → 100%)
   - Monitor closely during rollout
   - Quick rollback if issues detected
```

### Follow-up Actions

1. **Track action items** - Assign owners and track completion
2. **Prevent recurrence** - Implement preventive measures
3. **Document learnings** - Add to team wiki
4. **Share findings** - Present to broader team

---

## Recovery Time Objectives (RTO) & Recovery Point Objectives (RPO)

| Metric | Target | Owner |
|--------|--------|-------|
| **P1 RTO** | 30 minutes | On-call engineer |
| **P2 RTO** | 4 hours | Tech lead |
| **P3 RTO** | 24 hours | Product team |
| **RPO** | 1 hour | DevOps (backup schedule) |
| **Mean Time to Detect (MTTD)** | < 2 minutes | Monitoring |
| **Mean Time to Respond (MTTR)** | < 5 minutes | On-call |
| **Mean Time to Resolve (MTTS)** | < 30 min (P1) | Team |

---

## On-Call Escalation Chain

### Primary On-Call
- 24/7 responsibility
- First responder
- Escalates to on-call manager if unable to resolve in 15 minutes

### On-Call Manager
- Manages incident response
- Coordinates team resources
- Escalates to CTO if needed

### CTO (Last Resort)
- Strategic decisions
- Critical incidents
- Public communication decisions

---

## Tools and Resources

### Incident Management

- **Ticket System:** GitHub Issues with `incident` label
- **Communication:** Slack (#incidents channel)
- **Metrics:** Prometheus/Grafana
- **Logs:** Docker logs, CloudWatch
- **Status Page:** status.quorumcredit.io

### Critical Scripts

- `./scripts/pause_contract.sh` - Emergency pause
- `./scripts/unpause_contract.sh` - Resume operations
- `./scripts/smoke_tests.sh` - Verify functionality
- `./scripts/backup.sh` - Manual backup
- `./scripts/restore.sh` - State recovery

### Documentation Links

- [Production Deployment Guide](production-deployment-guide.md)
- [Upgrade Procedure](upgrade-guide.md)
- [Backup & Recovery Guide](backup-recovery-guide.md)
- [Monitoring Guide](monitoring-guide.md)
- [Troubleshooting Guide](troubleshooting-guide.md)

---

## Revision History

| Date | Version | Changes | Author |
|------|---------|---------|--------|
| 2026-06-29 | 1.0 | Initial document | Engineering |

---

## Questions?

For questions about incident response procedures:
- Slack: #incidents
- Email: oncall@quorumcredit.io
- Wiki: https://wiki.quorumcredit.io/incident-response
