# Blue-Green Deployment Guide

## Overview

This guide describes the blue-green deployment strategy for QuorumCredit, enabling zero-downtime deployments with automatic rollback capabilities.

## Architecture

### Environment Structure

```
┌─────────────────────────────────────────┐
│        Load Balancer / Ingress          │
├──────────────────┬──────────────────────┤
│                  │                      │
│            ┌─────────────┐      ┌──────────────┐
│            │  Blue Env   │      │  Green Env   │
│            │  (Active)   │      │  (Standby)   │
│            │             │      │              │
│            │ - Backend   │      │ - Backend    │
│            │ - Indexer   │      │ - Indexer    │
│            │ - Dashboard │      │ - Dashboard  │
│            └─────────────┘      └──────────────┘
│                  ↑                       ↑
│          100% Traffic             0% Traffic
│
│        (Traffic switch after
│         health checks pass)
│
```

### Deployment Flow

1. **Prepare Green Environment**
   - Deploy new version to green slot
   - Keep blue slot serving all traffic

2. **Health Checks**
   - Endpoint health verification
   - Dependency checks (DB, cache, API)
   - Performance baseline validation
   - Smoke tests

3. **Traffic Switch**
   - Gradual traffic migration (5% → 25% → 50% → 75% → 100%)
   - Monitor metrics at each step
   - Automatic rollback on issues

4. **Cleanup**
   - Blue environment remains as fallback
   - Ready for next deployment cycle

## Deployment Steps

### Manual Deployment

1. **Trigger Blue-Green Deployment**

```bash
gh workflow run blue-green-deploy.yml \
  -f environment=staging \
  -f version=v1.2.3 \
  -f auto_promote=true
```

2. **Monitor Workflow**

```bash
gh run watch
```

### Components Deployed

- **backend**: API server
- **indexer**: Event indexer service
- **dashboard**: Web dashboard

## Health Checks

### Endpoint Health Verification

```bash
curl -X GET https://api.example.com/health
```

Expected response:
```json
{
  "status": "ok",
  "version": "1.2.3",
  "uptime_seconds": 3600
}
```

### Dependency Checks

- **Database**: `/health/db`
- **Cache**: `/health/cache`
- **Message Queue**: `/health/queue`
- **Blockchain**: `/health/blockchain`

### Performance Baseline

- Response time < 1 second
- Error rate < 5%
- P95 latency < 500ms

## Traffic Switching

### Progressive Traffic Migration

```
Phase 1: 100% Blue, 0% Green
         ↓ 60 seconds monitoring
Phase 2: 75% Blue, 25% Green
         ↓ 60 seconds monitoring
Phase 3: 50% Blue, 50% Green
         ↓ 60 seconds monitoring
Phase 4: 25% Blue, 75% Green
         ↓ 60 seconds monitoring
Phase 5: 0% Blue, 100% Green (Complete)
```

### Automatic Rollback

Rollback is triggered if:
- Error rate > 5%
- P95 latency > 500ms
- Health check fails
- Dependency check fails
- Manual rollback requested

## Monitoring

### Key Metrics

```promql
# Error rate
rate(errors_total{component="backend"}[5m])

# Latency P95
histogram_quantile(0.95, rate(request_duration_ms[5m]))

# CPU usage
rate(cpu_usage_seconds_total[5m])

# Memory usage
memory_usage_bytes
```

### Alerts

Configure alerts for:
- Error rate > 5%
- P95 latency > 500ms
- Deployment failure
- Rollback triggered

## Rollback Procedure

### Automatic Rollback

Triggered automatically during deployment if health checks fail:

```bash
# Traffic switches back to previous blue version
Traffic: Green → Blue (100% to previous version)
```

### Manual Rollback

```bash
# Switch traffic back to blue
./scripts/traffic-switch.sh backend staging green blue
```

## Configuration

### Environment Variables

```bash
# Container registry
REGISTRY=quorum.azurecr.io

# Deployment timeout
TIMEOUT_SECONDS=600

# Health check configuration
HEALTH_CHECK_ENDPOINT=/health
MAX_RETRIES=30
RETRY_DELAY=10
```

### Load Balancer Configuration

```yaml
# nginx example
upstream backend_blue {
  server backend-blue:8000;
}

upstream backend_green {
  server backend-green:8000;
}

# Initially route 100% to blue
split_clients "${request_id}" $backend_upstream {
  100% backend_blue;
  0% backend_green;
}
```

## Troubleshooting

### Deployment Stuck on Health Checks

```bash
# Check service logs
kubectl logs -l app=backend,slot=green -f

# Manually verify health endpoint
curl -v https://api.example.com/health
```

### Traffic Switch Not Completing

```bash
# Check current traffic weights
kubectl get svc backend-blue -o jsonpath='{.status.loadBalancer}'

# Manually trigger traffic switch
./scripts/traffic-switch.sh backend staging green blue
```

### Need to Revert to Previous Version

```bash
# Switch traffic back to blue (previous version)
./scripts/traffic-switch.sh backend staging green blue

# Deploy older version to green for next cycle
gh workflow run blue-green-deploy.yml \
  -f environment=production \
  -f version=v1.2.2 \
  -f auto_promote=false
```

## Best Practices

1. **Always Verify in Staging First**
   - Test new versions in staging environment
   - Run full smoke tests before production deployment

2. **Monitor Closely During Migration**
   - Watch error rates during traffic shift
   - Be ready to rollback if issues arise

3. **Gradual Traffic Migration**
   - Never switch 100% traffic immediately
   - Progressive migration (5% → 25% → 50% → 75% → 100%)

4. **Keep Fallback Ready**
   - Blue environment remains available
   - Quick rollback if needed

5. **Document Changes**
   - Update deployment notes in git commit
   - Track version history in deployment reports

## Automation Integration

### GitHub Actions Workflow

```yaml
on:
  workflow_dispatch:
    inputs:
      version:
        required: true

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Blue-Green Deploy
        run: |
          gh workflow run blue-green-deploy.yml \
            -f version=${{ github.event.inputs.version }}
```

## Performance Considerations

- **Parallel Component Deployment**: Components deploy one at a time to maintain stability
- **Health Check Timeout**: 10 minutes per component
- **Traffic Switch Duration**: ~4 minutes per component
- **Total Deployment Time**: ~30 minutes for all three components

## Security

- Use CI/CD only for trusted deployments
- Require approval before production deployments
- Audit all traffic switches
- Encrypt sensitive configuration data
- Use separate credentials for blue/green environments
