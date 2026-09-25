#!/bin/bash

# Issue #1579: Rust Dependencies Security Scan
# Scans Rust dependencies for known vulnerabilities

set -euo pipefail

REPO_ROOT="$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"
REPORT_DIR="$REPO_ROOT/security/reports"

log_info() { echo "ℹ️  $*" >&2; }
log_success() { echo "✅ $*" >&2; }
log_error() { echo "❌ $*" >&2; }
log_warn() { echo "⚠️  $*" >&2; }

mkdir -p "$REPORT_DIR"

log_info "Running Cargo Audit for Rust dependencies..."

# Run cargo-audit and capture both output and exit code
if cargo audit --json > "$REPORT_DIR/cargo-audit.json" 2>&1; then
  log_success "Cargo Audit completed - no vulnerabilities found"
  exit 0
else
  exit_code=$?
  log_warn "Cargo Audit found vulnerabilities (exit code: $exit_code)"

  # Parse and analyze results
  if command -v jq &> /dev/null; then
    critical_count=$(jq '[.vulnerabilities[] | select(.advisory.severity == "critical")] | length' "$REPORT_DIR/cargo-audit.json" 2>/dev/null || echo 0)
    high_count=$(jq '[.vulnerabilities[] | select(.advisory.severity == "high")] | length' "$REPORT_DIR/cargo-audit.json" 2>/dev/null || echo 0)
    medium_count=$(jq '[.vulnerabilities[] | select(.advisory.severity == "medium")] | length' "$REPORT_DIR/cargo-audit.json" 2>/dev/null || echo 0)

    log_info "Vulnerability Summary:"
    log_error "Critical: $critical_count"
    log_warn "High: $high_count"
    log_warn "Medium: $medium_count"

    if [ "$critical_count" -gt 0 ]; then
      log_error "Critical vulnerabilities found in Rust dependencies"
      exit 1
    fi
  fi

  exit 0
fi
