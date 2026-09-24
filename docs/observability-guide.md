# Observability Stack Integration Guide

## Overview

This guide describes the comprehensive observability stack for QuorumCredit, providing centralized monitoring, logging, tracing, and alerting capabilities.

## Architecture

### Components

```
┌─────────────────────────────────────────────────────────────────┐
│                     Applications                                │
│  (Backend API, Indexer, Dashboard)                             │
└─────────┬───────────────────────────────────────────────────────┘
          │
    ┌─────┴──────────────────┐
    │                        │
    ▼                        ▼
┌────────────┐         ┌──────────────┐
│ Prometheus │         │   Jaeger     │
│ (Metrics)  │         │  (Tracing)   │
└─────┬──────┘         └──────┬───────┘
      │                       │
      │   ┌───────────────────┴────────┐
      │   │                            │
      ▼   ▼                            ▼
    ┌──────────────────────┐     ┌─────────────┐
    │  Logs & Events       │     │ AlertManager│
    │  (ELK Stack)         │     │ (Alerting)  │
    │  Elasticsearch       │     └──────┬──────┘
    │  Logstash            │            │
    │  Kibana              │            │
    └──────────────────────┘            │
                                       │
                              ┌────────┴──────────┐
                              │                   │
                              ▼                   ▼
                          ┌────────┐         ┌─────────┐
                          │ Slack  │         │Pagerduty│
                          └────────┘         └─────────┘
```

### Stack Components

1. **Prometheus**: Metrics collection and time-series database
2. **Grafana**: Metrics visualization and dashboards
3. **Jaeger**: Distributed tracing
4. **Elasticsearch**: Centralized log storage
5. **Logstash**: Log processing and pipeline
6. **Kibana**: Log visualization
7. **AlertManager**: Alert routing and notifications
8. **Push Gateway**: Batch job metrics

## Setup

### Docker Compose

Start the full observability stack:

```bash
docker-compose -f docker-compose.observability.yml up -d
```

Verify all services are healthy:

```bash
docker-compose -f docker-compose.observability.yml ps
```

### Accessing Services

| Service | URL | Default Credentials |
|---------|-----|-------------------|
| Prometheus | http://localhost:9090 | None |
| Grafana | http://localhost:3000 | admin/admin |
| Jaeger | http://localhost:16686 | None |
| Kibana | http://localhost:5601 | None |
| AlertManager | http://localhost:9093 | None |

## Metrics Collection

### Application Instrumentation

Instrument your applications to expose metrics:

```go
// Example: Go with Prometheus client library
package main

import (
    "github.com/prometheus/client_golang/prometheus"
    "github.com/prometheus/client_golang/prometheus/promauto"
    "github.com/prometheus/client_golang/prometheus/promhttp"
    "net/http"
)

var (
    requests = promauto.NewCounterVec(
        prometheus.CounterOpts{
            Name: "http_requests_total",
            Help: "Total HTTP requests",
        },
        []string{"method", "endpoint", "status"},
    )
    
    latency = promauto.NewHistogramVec(
        prometheus.HistogramOpts{
            Name: "http_request_duration_seconds",
            Help: "HTTP request latency",
        },
        []string{"method", "endpoint"},
    )
)

func main() {
    http.Handle("/metrics", promhttp.Handler())
    http.ListenAndServe(":8000", nil)
}
```

### Key Metrics

- **Request Rate**: `rate(http_requests_total[5m])`
- **Error Rate**: `rate(http_requests_total{status=~"5.."}[5m])`
- **Latency P95**: `histogram_quantile(0.95, rate(http_request_duration_seconds_bucket[5m]))`
- **Memory Usage**: `process_resident_memory_bytes`
- **CPU Usage**: `rate(process_cpu_seconds_total[5m])`

## Distributed Tracing

### Instrumentation

Add tracing to your applications using OpenTelemetry:

```go
// Example: OpenTelemetry instrumentation
import "go.opentelemetry.io/otel"
import "go.opentelemetry.io/otel/exporters/jaeger/jaegerhttp"

exporter, _ := jaegerhttp.New(
    jaegerhttp.WithEndpoint("http://jaeger:14268/api/traces"),
)

tp := tracesdk.NewTracerProvider(
    tracesdk.WithBatcher(exporter),
)

otel.SetTracerProvider(tp)
```

### Viewing Traces

Access Jaeger UI: http://localhost:16686

- Search by service name
- Filter by duration and tags
- View distributed trace waterfall
- Identify latency bottlenecks

## Centralized Logging

### Log Format

Send structured JSON logs to Logstash:

```json
{
  "timestamp": "2024-09-24T12:34:56.789Z",
  "level": "INFO",
  "service": "backend",
  "message": "User login successful",
  "user_id": "user123",
  "trace_id": "abc123def456",
  "span_id": "xyz789",
  "duration_ms": 45
}
```

### Log Configuration

Configure your application to send logs to Logstash:

```yaml
# Example: Fluent Bit configuration
[OUTPUT]
    Name es
    Match *
    Host elasticsearch
    Port 9200
    Type _doc
    Index logs-${HOSTNAME}-${TIMESTAMP}
```

### Viewing Logs

Access Kibana: http://localhost:5601

- Create index patterns
- Search and filter logs
- Build log visualizations
- Create dashboards

## Alerting

### Alert Rules

Prometheus alert rules are defined in `prometheus/alert-rules.yml`:

```yaml
alert: HighErrorRate
expr: rate(http_requests_total{status=~"5.."}[5m]) > 0.05
for: 5m
labels:
  severity: critical
annotations:
  summary: "High error rate on {{ $labels.instance }}"
```

### Notification Channels

Configure AlertManager to send alerts to:

- **Slack**: Real-time notifications in Slack channels
- **PagerDuty**: On-call escalation
- **Email**: Email notifications
- **Custom Webhooks**: Custom integrations

### Silencing Alerts

Temporarily silence alerts in AlertManager UI:

```bash
curl -XPOST http://localhost:9093/api/v1/silences \
  -H "Content-Type: application/json" \
  -d '{
    "matchers": [
      {"name": "alertname", "value": "HighErrorRate"}
    ],
    "duration": "1h",
    "comment": "Maintenance window"
  }'
```

## Dashboards

### Pre-built Dashboard

A default observability dashboard is provisioned automatically:

- **QuorumCredit Observability Dashboard**
  - Request rate and error rate
  - Latency percentiles (P50, P95, P99)
  - Memory and CPU usage
  - Service health overview

### Creating Custom Dashboards

1. Open Grafana: http://localhost:3000
2. Click "Create" → "Dashboard"
3. Add panels with Prometheus queries
4. Save dashboard

### Example Query

```promql
# Combined metric showing request rate by status
sum(rate(http_requests_total[5m])) by (status)
```

## Best Practices

### Metrics

1. **Use Descriptive Names**
   - `http_requests_total` (good) vs `requests` (vague)

2. **Include Relevant Labels**
   - Track by method, endpoint, status code

3. **Set Appropriate Retention**
   - 30 days default in Prometheus
   - Adjust based on storage

### Logging

1. **Structured Logging**
   - Use JSON format for easy parsing

2. **Log Levels**
   - ERROR: System errors
   - WARN: Degraded conditions
   - INFO: Important events
   - DEBUG: Detailed diagnostics

3. **Sensitive Data**
   - Don't log passwords or tokens
   - Redact PII in logs

### Tracing

1. **Sample Strategically**
   - Sample 10% of normal traffic
   - Sample 100% of errors

2. **Meaningful Spans**
   - Create spans for expensive operations
   - Add relevant attributes

### Alerting

1. **Meaningful Alerts**
   - Action: Alert should trigger action
   - Signal: Alert indicates real problem
   - Timing: Alert quickly enough to matter

2. **Avoid Alert Fatigue**
   - Fine-tune thresholds
   - Use appropriate repeat intervals
   - Group related alerts

## Troubleshooting

### Prometheus Not Scraping Metrics

```bash
# Check Prometheus targets
curl http://localhost:9090/api/v1/targets

# Check target configuration
cat observability/prometheus/prometheus.yml
```

### Logs Not Appearing in Kibana

```bash
# Check Logstash processing
curl http://localhost:9600/

# Verify Elasticsearch index
curl http://localhost:9200/_cat/indices/
```

### Alerts Not Sending

```bash
# Check AlertManager
curl http://localhost:9093/api/v1/alerts

# Verify webhook URL
curl -v http://localhost:9093/api/v1/receivers
```

## Integration with CI/CD

### GitHub Actions Workflow

```yaml
- name: Verify Observability Stack
  run: |
    chmod +x ./scripts/verify-observability-health.sh
    ./scripts/verify-observability-health.sh \
      --component prometheus \
      --environment staging
```

## Performance Considerations

- **Prometheus Retention**: 30 days of 15-second metrics
- **Elasticsearch Shards**: 1 shard per day for optimal performance
- **Logstash Batch Size**: 125 events per batch
- **Grafana Query Cache**: 5-minute TTL

## Security

- Enable authentication for Kibana and Grafana
- Use TLS for data in transit
- Restrict network access to monitoring infrastructure
- Audit access to sensitive metrics and logs
- Rotate API keys and credentials regularly
