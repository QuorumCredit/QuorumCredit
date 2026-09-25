#!/bin/bash

# Issue #1579: Aggregate Security Scan Results
# Consolidates all security scan reports into comprehensive summary

set -euo pipefail

REPO_ROOT="$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"
REPORTS_DIR="$REPO_ROOT/security/reports/summary"

log_info() { echo "ℹ️  $*" >&2; }
log_success() { echo "✅ $*" >&2; }
log_error() { echo "❌ $*" >&2; }

mkdir -p "$REPORTS_DIR"

log_info "Aggregating security scan results..."

# Create comprehensive report
cat > "$REPORTS_DIR/README.md" <<'EOF'
# Security Scan Report

## Vulnerability Summary

### Rust Dependencies (Cargo Audit)

EOF

# Process cargo audit results
if [ -f "$REPO_ROOT/security/reports/cargo-audit.json" ]; then
  if command -v jq &> /dev/null; then
    critical=$(jq '[.vulnerabilities[] | select(.advisory.severity == "critical")] | length' "$REPO_ROOT/security/reports/cargo-audit.json" 2>/dev/null || echo 0)
    high=$(jq '[.vulnerabilities[] | select(.advisory.severity == "high")] | length' "$REPO_ROOT/security/reports/cargo-audit.json" 2>/dev/null || echo 0)

    cat >> "$REPORTS_DIR/README.md" <<EOF
- **Critical**: $critical
- **High**: $high

EOF

    if [ "$critical" -gt 0 ]; then
      echo "CRITICAL" > "$REPORTS_DIR/CRITICAL_FOUND"
      jq '.vulnerabilities[] | select(.advisory.severity == "critical")' "$REPO_ROOT/security/reports/cargo-audit.json" >> "$REPORTS_DIR/README.md"
    fi
  fi
fi

# Process npm audit results
cat >> "$REPORTS_DIR/README.md" <<'EOF'

### Node.js Dependencies (npm Audit)

EOF

if [ -f "$REPO_ROOT/security/reports/npm-audit.json" ]; then
  if command -v jq &> /dev/null; then
    total=$(jq '.metadata.vulnerabilities.total // 0' "$REPO_ROOT/security/reports/npm-audit.json")
    critical=$(jq '.metadata.vulnerabilities.critical // 0' "$REPO_ROOT/security/reports/npm-audit.json")
    high=$(jq '.metadata.vulnerabilities.high // 0' "$REPO_ROOT/security/reports/npm-audit.json")

    cat >> "$REPORTS_DIR/README.md" <<EOF
- **Total**: $total
- **Critical**: $critical
- **High**: $high

EOF

    if [ "$critical" -gt 0 ]; then
      echo "CRITICAL" > "$REPORTS_DIR/CRITICAL_FOUND"
    fi
  fi
fi

# Add remediation guidance
cat >> "$REPORTS_DIR/README.md" <<'EOF'

## Remediation Steps

1. **Review Vulnerabilities**
   - Examine each vulnerability in detail
   - Understand the affected components
   - Assess impact on the application

2. **Update Dependencies**
   - Update to patched versions
   - Run tests after updates
   - Verify compatibility

3. **Monitor Continuously**
   - Set up automated scanning
   - Review alerts regularly
   - Track vulnerability trends

## Next Steps

- [ ] Review critical vulnerabilities
- [ ] Create remediation tickets
- [ ] Schedule security updates
- [ ] Verify fixes in testing environment
- [ ] Deploy to production

---

Report generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF

log_success "Security report generated at $REPORTS_DIR/README.md"
