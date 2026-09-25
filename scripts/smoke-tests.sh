#!/bin/bash

# Issue #1577: Smoke Tests Script
# Runs basic functionality tests on newly deployed services
# Validates core features before traffic switch

set -euo pipefail

COMPONENT="${1:-}"
ENVIRONMENT="${2:-staging}"
SLOT="${3:-green}"

log_info() {
  echo "ℹ️  $*" >&2
}

log_success() {
  echo "✅ $*" >&2
}

log_error() {
  echo "❌ $*" >&2
}

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

run_backend_tests() {
  local endpoint=$1

  log_info "Running backend smoke tests..."

  # Test 1: API version endpoint
  if curl -s "$endpoint/api/v1/version" | grep -q '"version"'; then
    log_success "✓ Version endpoint responding"
  else
    log_error "✗ Version endpoint failed"
    return 1
  fi

  # Test 2: Health metrics
  if curl -s "$endpoint/health/metrics" | grep -q '"uptime"'; then
    log_success "✓ Metrics endpoint responding"
  else
    log_error "✗ Metrics endpoint failed"
    return 1
  fi

  # Test 3: Authentication flow
  if curl -s -X POST "$endpoint/api/v1/auth/check" \
    -H "Content-Type: application/json" \
    -d '{}' | grep -q '"authenticated"'; then
    log_success "✓ Authentication endpoint responding"
  else
    log_error "✗ Authentication endpoint failed"
    return 1
  fi

  # Test 4: Database connection
  if curl -s "$endpoint/health/db" | grep -q '"status":"ok"'; then
    log_success "✓ Database connectivity verified"
  else
    log_error "✗ Database connectivity check failed"
    return 1
  fi

  log_success "Backend smoke tests passed"
  return 0
}

run_indexer_tests() {
  local endpoint=$1

  log_info "Running indexer smoke tests..."

  # Test 1: Indexer status
  if curl -s "$endpoint/status" | grep -q '"status"'; then
    log_success "✓ Indexer status endpoint responding"
  else
    log_error "✗ Indexer status endpoint failed"
    return 1
  fi

  # Test 2: Blockchain sync status
  if curl -s "$endpoint/sync-status" | grep -q '"synced"'; then
    log_success "✓ Sync status endpoint responding"
  else
    log_error "✗ Sync status endpoint failed"
    return 1
  fi

  # Test 3: Last indexed ledger
  if curl -s "$endpoint/ledger/latest" | grep -q '"ledger_number"'; then
    log_success "✓ Latest ledger endpoint responding"
  else
    log_error "✗ Latest ledger endpoint failed"
    return 1
  fi

  log_success "Indexer smoke tests passed"
  return 0
}

run_dashboard_tests() {
  local endpoint=$1

  log_info "Running dashboard smoke tests..."

  # Test 1: Dashboard loads
  if curl -s "$endpoint" | grep -q '<!DOCTYPE html'; then
    log_success "✓ Dashboard HTML loads successfully"
  else
    log_error "✗ Dashboard failed to load"
    return 1
  fi

  # Test 2: API configuration
  if curl -s "$endpoint/config" | grep -q '"apiEndpoint"'; then
    log_success "✓ API configuration endpoint responding"
  else
    log_error "✗ API configuration endpoint failed"
    return 1
  fi

  # Test 3: Static assets
  if curl -s -I "$endpoint/static/app.js" | grep -q "200 OK"; then
    log_success "✓ Static assets serving correctly"
  else
    log_error "✗ Static assets not found"
    return 1
  fi

  log_success "Dashboard smoke tests passed"
  return 0
}

main() {
  if [[ -z "$COMPONENT" ]]; then
    log_error "Usage: $0 <component> <environment> <slot>"
    exit 1
  fi

  log_info "Running smoke tests for $COMPONENT in $ENVIRONMENT ($SLOT)"

  endpoint=$(get_service_endpoint "$COMPONENT" "$ENVIRONMENT" "$SLOT")
  log_info "Target endpoint: $endpoint"

  # Wait for service to be fully ready
  sleep 5

  case "$COMPONENT" in
    backend)
      run_backend_tests "$endpoint" || exit 1
      ;;
    indexer)
      run_indexer_tests "$endpoint" || exit 1
      ;;
    dashboard)
      run_dashboard_tests "$endpoint" || exit 1
      ;;
    *)
      log_error "Unknown component: $COMPONENT"
      exit 1
      ;;
  esac

  log_success "All smoke tests passed for $COMPONENT"
}

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
  main
fi
