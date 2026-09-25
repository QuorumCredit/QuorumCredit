#!/bin/bash

# Issue #1577: Health Check Script
# Performs comprehensive health checks on deployed applications
# Validates readiness before traffic switching

set -euo pipefail

COMPONENT="${1:-}"
ENVIRONMENT="${2:-staging}"
SLOT="${3:-green}"
MAX_RETRIES="${4:-30}"
RETRY_DELAY="${5:-10}"

log_info() {
  echo "ℹ️  $*" >&2
}

log_success() {
  echo "✅ $*" >&2
}

log_error() {
  echo "❌ $*" >&2
}

# Get service endpoint for the component and slot
get_service_endpoint() {
  local component=$1
  local environment=$2
  local slot=$3

  case "$environment" in
    staging)
      case "$component" in
        backend) echo "http://quorum-backend-${slot}-staging.azurewebsites.net" ;;
        indexer) echo "http://quorum-indexer-${slot}-staging.azurewebsites.net" ;;
        dashboard) echo "http://quorum-dashboard-${slot}-staging.azurewebsites.net" ;;
      esac
      ;;
    production)
      case "$component" in
        backend) echo "https://api.quorumcredit.io" ;;
        indexer) echo "https://indexer.quorumcredit.io" ;;
        dashboard) echo "https://app.quorumcredit.io" ;;
      esac
      ;;
  esac
}

check_endpoint_health() {
  local url=$1
  local max_retries=$2
  local retry_delay=$3
  local attempt=0

  while [[ $attempt -lt $max_retries ]]; do
    attempt=$((attempt + 1))

    log_info "Health check attempt $attempt/$max_retries for $url"

    if response=$(curl -s -w "\n%{http_code}" "$url/health" 2>/dev/null); then
      http_code=$(echo "$response" | tail -n1)
      body=$(echo "$response" | head -n-1)

      if [[ "$http_code" == "200" ]]; then
        log_success "Health check passed (HTTP $http_code)"
        echo "$body" > /tmp/health-check-response.json
        return 0
      else
        log_info "Received HTTP $http_code, retrying in ${retry_delay}s..."
      fi
    else
      log_info "Connection failed, retrying in ${retry_delay}s..."
    fi

    sleep "$retry_delay"
  done

  log_error "Health check failed after $max_retries attempts"
  return 1
}

check_dependencies() {
  local component=$1
  local endpoint=$2

  log_info "Checking dependencies for $component..."

  case "$component" in
    backend)
      # Check database connectivity
      if ! curl -s "$endpoint/health/db" | grep -q '"status":"ok"'; then
        log_error "Database health check failed"
        return 1
      fi

      # Check cache connectivity
      if ! curl -s "$endpoint/health/cache" | grep -q '"status":"ok"'; then
        log_error "Cache health check failed"
        return 1
      fi

      log_success "All backend dependencies are healthy"
      ;;

    indexer)
      # Check blockchain connectivity
      if ! curl -s "$endpoint/health/blockchain" | grep -q '"status":"ok"'; then
        log_error "Blockchain connectivity check failed"
        return 1
      fi

      log_success "Indexer dependencies are healthy"
      ;;

    dashboard)
      # Check backend API connectivity
      if ! curl -s "$endpoint/health/api" | grep -q '"status":"ok"'; then
        log_error "Backend API connectivity check failed"
        return 1
      fi

      log_success "Dashboard dependencies are healthy"
      ;;
  esac

  return 0
}

check_performance_baseline() {
  local endpoint=$1
  local component=$2

  log_info "Checking performance baseline for $component..."

  # Measure response time
  response_time=$(curl -s -w "%{time_total}" -o /dev/null "$endpoint/health")
  log_info "Health endpoint response time: ${response_time}s"

  # Check if response time is within acceptable range (< 1 second)
  if (( $(echo "$response_time > 1.0" | bc -l) )); then
    log_error "Response time exceeds acceptable threshold: ${response_time}s > 1.0s"
    return 1
  fi

  log_success "Performance baseline acceptable"
  return 0
}

main() {
  if [[ -z "$COMPONENT" ]]; then
    log_error "Usage: $0 <component> <environment> <slot> [max_retries] [retry_delay]"
    exit 1
  fi

  log_info "Running health checks for $COMPONENT in $ENVIRONMENT ($SLOT)"

  endpoint=$(get_service_endpoint "$COMPONENT" "$ENVIRONMENT" "$SLOT")
  log_info "Target endpoint: $endpoint"

  # Run health checks
  if ! check_endpoint_health "$endpoint" "$MAX_RETRIES" "$RETRY_DELAY"; then
    log_error "Endpoint health check failed"
    exit 1
  fi

  if ! check_dependencies "$COMPONENT" "$endpoint"; then
    log_error "Dependency health check failed"
    exit 1
  fi

  if ! check_performance_baseline "$endpoint" "$COMPONENT"; then
    log_error "Performance baseline check failed"
    exit 1
  fi

  log_success "All health checks passed for $COMPONENT ($SLOT)"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  main
fi
