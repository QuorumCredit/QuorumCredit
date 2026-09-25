#!/bin/bash

# Issue #1579: npm Dependencies Security Scan
# Scans npm dependencies for known vulnerabilities

set -euo pipefail

REPO_ROOT="$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"
REPORT_DIR="$REPO_ROOT/security/reports"

log_info() { echo "ℹ️  $*" >&2; }
log_success() { echo "✅ $*" >&2; }
log_error() { echo "❌ $*" >&2; }
log_warn() { echo "⚠️  $*" >&2; }

mkdir -p "$REPORT_DIR"

log_info "Running npm audit for Node.js dependencies..."

# Run npm audit with JSON output
npm audit --json > "$REPORT_DIR/npm-audit.json" 2>&1 || true

# Parse results
if command -v jq &> /dev/null; then
  total_vulnerabilities=$(jq '.metadata.vulnerabilities.total // 0' "$REPORT_DIR/npm-audit.json")
  critical=$(jq '.metadata.vulnerabilities.critical // 0' "$REPORT_DIR/npm-audit.json")
  high=$(jq '.metadata.vulnerabilities.high // 0' "$REPORT_DIR/npm-audit.json")

  log_info "npm Audit Results:"
  log_info "Total vulnerabilities: $total_vulnerabilities"
  [ "$critical" -gt 0 ] && log_error "Critical: $critical" || log_success "Critical: $critical"
  [ "$high" -gt 0 ] && log_warn "High: $high" || log_success "High: $high"

  if [ "$critical" -gt 0 ]; then
    log_error "Critical vulnerabilities found in npm dependencies"

    # Generate detailed report
    jq '.vulnerabilities | to_entries[] | select(.value.severity == "critical")' "$REPORT_DIR/npm-audit.json" | while read -r vuln; do
      package=$(echo "$vuln" | jq -r '.value.name // "unknown"')
      severity=$(echo "$vuln" | jq -r '.value.severity')
      description=$(echo "$vuln" | jq -r '.value.title // "No description"')
      log_error "  - $package: $description"
    done

    exit 1
  fi
fi

log_success "npm audit completed"
exit 0
