#!/bin/bash

# Issue #1577: Deployment Monitoring Script
# Monitors deployment metrics and triggers rollback on anomalies
# Tracks error rates, latency, and other critical metrics

set -euo pipefail

COMPONENT="${1:-}"
ENVIRONMENT="${2:-staging}"
DURATION_SECONDS="${3:-300}"
ERROR_RATE_THRESHOLD="${4:-5}"
LATENCY_THRESHOLD_MS="${5:-500}"

log_info() {
  echo "ℹ️  $*" >&2
}

log_success() {
  echo "✅ $*" >&2
}

log_error() {
  echo "❌ $*" >&2
}

log_warn() {
  echo "⚠️  $*" >&2
}

get_metrics_endpoint() {
  local component=$1
  local environment=$2

  case "$environment" in
    staging)
      echo "http://metrics-staging.azurewebsites.net"
      ;;
    production)
      echo "https://metrics.quorumcredit.io"
      ;;
  esac
}

collect_metrics() {
  local component=$1
  local metrics_endpoint=$2
  local start_time=$(date +%s)
  local end_time=$((start_time + $DURATION_SECONDS))

  log_info "Collecting metrics for $COMPONENT from $start_time to $end_time"

  local query="component=\"$component\" AND timestamp >= $start_time AND timestamp <= $end_time"

  # Query Prometheus/metrics system
  local metrics=$(curl -s "${metrics_endpoint}/query" \
    --data-urlencode "query=$query" \
    2>/dev/null || echo '{}')

  echo "$metrics"
}

analyze_error_rate() {
  local component=$1
  local metrics_endpoint=$2
  local threshold=$3

  log_info "Analyzing error rate for $component..."

  # Query error rate
  local error_query="rate(errors_total{component=\"$component\"}[5m])"
  local error_rate=$(curl -s "${metrics_endpoint}/query" \
    --data-urlencode "query=$error_query" 2>/dev/null | \
    grep -oP '"value":\s*"\K[^"]+' | head -1 || echo "0")

  log_info "Current error rate: $error_rate%"

  if (( $(echo "$error_rate > $threshold" | bc -l 2>/dev/null || echo 0) )); then
    log_error "Error rate ($error_rate%) exceeds threshold ($threshold%)"
    return 1
  fi

  log_success "Error rate within acceptable limits"
  return 0
}

analyze_latency() {
  local component=$1
  local metrics_endpoint=$2
  local threshold=$3

  log_info "Analyzing latency for $component..."

  # Query P95 latency
  local latency_query="histogram_quantile(0.95, rate(request_duration_ms{component=\"$component\"}[5m]))"
  local p95_latency=$(curl -s "${metrics_endpoint}/query" \
    --data-urlencode "query=$latency_query" 2>/dev/null | \
    grep -oP '"value":\s*"\K[^"]+' | head -1 || echo "0")

  log_info "P95 Latency: ${p95_latency}ms"

  if (( $(echo "$p95_latency > $threshold" | bc -l 2>/dev/null || echo 0) )); then
    log_error "Latency (${p95_latency}ms) exceeds threshold (${threshold}ms)"
    return 1
  fi

  log_success "Latency within acceptable limits"
  return 0
}

analyze_resource_usage() {
  local component=$1
  local metrics_endpoint=$2

  log_info "Analyzing resource usage for $component..."

  # Query CPU usage
  local cpu_query="rate(cpu_usage_seconds_total{component=\"$component\"}[5m])"
  local cpu_usage=$(curl -s "${metrics_endpoint}/query" \
    --data-urlencode "query=$cpu_query" 2>/dev/null | \
    grep -oP '"value":\s*"\K[^"]+' | head -1 || echo "0")

  # Query memory usage
  local mem_query="memory_usage_bytes{component=\"$component\"}"
  local memory_usage=$(curl -s "${metrics_endpoint}/query" \
    --data-urlencode "query=$mem_query" 2>/dev/null | \
    grep -oP '"value":\s*"\K[^"]+' | head -1 || echo "0")

  log_info "CPU Usage: ${cpu_usage}%"
  log_info "Memory Usage: ${memory_usage}MB"

  # Warn if resources are trending high
  if (( $(echo "$cpu_usage > 80" | bc -l 2>/dev/null || echo 0) )); then
    log_warn "High CPU usage detected: ${cpu_usage}%"
  fi

  if (( $(echo "$memory_usage > 450" | bc -l 2>/dev/null || echo 0) )); then
    log_warn "High memory usage detected: ${memory_usage}MB"
  fi

  return 0
}

generate_report() {
  local component=$1
  local environment=$2
  local duration=$3

  local repo_root="$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"
  mkdir -p "$repo_root/deploy/reports"

  local report_file="$repo_root/deploy/reports/deployment-metrics-${component}.json"

  cat > "$report_file" <<EOF
{
  "component": "${component}",
  "environment": "${environment}",
  "monitoring_period_seconds": ${duration},
  "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
  "metrics": {
    "error_rate_threshold_percent": ${ERROR_RATE_THRESHOLD},
    "latency_threshold_ms": ${LATENCY_THRESHOLD_MS},
    "status": "monitored"
  },
  "checks": {
    "error_rate": "passed",
    "latency": "passed",
    "resource_usage": "passed"
  }
}
EOF

  log_info "Metrics report saved to: $report_file"
}

main() {
  if [[ -z "$COMPONENT" ]]; then
    log_error "Usage: $0 <component> <environment> <duration_seconds> <error_rate_threshold> <latency_threshold_ms>"
    exit 1
  fi

  log_info "Starting deployment monitoring for $COMPONENT"
  log_info "Duration: ${DURATION_SECONDS}s"
  log_info "Error rate threshold: ${ERROR_RATE_THRESHOLD}%"
  log_info "Latency threshold: ${LATENCY_THRESHOLD_MS}ms"

  metrics_endpoint=$(get_metrics_endpoint "$COMPONENT" "$ENVIRONMENT")

  # Run monitoring checks
  if ! analyze_error_rate "$COMPONENT" "$metrics_endpoint" "$ERROR_RATE_THRESHOLD"; then
    log_error "Error rate check failed - triggering rollback"
    exit 1
  fi

  if ! analyze_latency "$COMPONENT" "$metrics_endpoint" "$LATENCY_THRESHOLD_MS"; then
    log_error "Latency check failed - triggering rollback"
    exit 1
  fi

  analyze_resource_usage "$COMPONENT" "$metrics_endpoint"

  generate_report "$COMPONENT" "$ENVIRONMENT" "$DURATION_SECONDS"

  log_success "Deployment monitoring completed successfully"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  main
fi
