#!/bin/bash

# Issue #1579: Rust Dependencies Security Scan
# Scans Rust dependencies for known vulnerabilities.
# Issue #2091: fails the build on high, critical or unknown-severity advisories
# that are not accepted in security/audit-allowlist.json.

set -euo pipefail

REPO_ROOT="$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"
REPORT_DIR="$REPO_ROOT/security/reports"
REPORT="$REPORT_DIR/cargo-audit.json"

# shellcheck source=lib/audit-gate.sh
source "$REPO_ROOT/scripts/lib/audit-gate.sh"

log_info() { echo "ℹ️  $*" >&2; }
log_success() { echo "✅ $*" >&2; }
log_error() { echo "❌ $*" >&2; }
log_warn() { echo "⚠️  $*" >&2; }

mkdir -p "$REPORT_DIR"

log_info "Running Cargo Audit for Rust dependencies..."

# cargo audit exits non-zero when it finds vulnerabilities, so the exit code
# is recorded and the JSON report decides the result.
set +e
cargo audit --json > "$REPORT" 2>"$REPORT_DIR/cargo-audit.stderr"
audit_rc=$?
set -e

if ! jq -e '.vulnerabilities' "$REPORT" >/dev/null 2>&1; then
  log_error "cargo audit did not produce a readable JSON report (exit $audit_rc)"
  exit 1
fi

found=$(jq '.vulnerabilities.count // 0' "$REPORT")
if [ "$found" -eq 0 ]; then
  log_success "Cargo Audit completed - no vulnerabilities found"
  exit 0
fi

log_warn "Cargo Audit reported $found vulnerabilities"
jq -r '.vulnerabilities.list[]? | "  - \(.advisory.id) (\(.advisory.severity // "unknown")) \(.package.name // "?")@\(.package.version // "?")"' "$REPORT" >&2

blocking=$(jq -r '.vulnerabilities.list[]? | [.advisory.id, (.advisory.severity // "unknown")] | @tsv' "$REPORT" \
  | audit_gate_blocking)

if [ -n "$blocking" ]; then
  log_error "Blocking Rust advisories (high/critical/unknown and not allowlisted):"
  printf '%s\n' "$blocking" | sed 's/^/  - /' >&2
  exit 1
fi

log_success "Cargo Audit: no blocking advisories (medium/low or allowlisted)"
exit 0
