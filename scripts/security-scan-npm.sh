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

# Issue #1991: Flag end-of-life Node base images in Dockerfiles
log_info "Checking container base images for end-of-life Node versions..."

EOL_NODE_MAJORS="14 16 18"
DOCKERFILES=("$REPO_ROOT/server/Dockerfile" "$REPO_ROOT/services/indexer/Dockerfile")
EOL_REPORT="$REPORT_DIR/base-image-eol.json"
eol_found=0
eol_entries=""

for dockerfile in "${DOCKERFILES[@]}"; do
  [ -f "$dockerfile" ] || continue
  rel_path="${dockerfile#"$REPO_ROOT"/}"

  while IFS= read -r image; do
    [ -n "$image" ] || continue
    # Extract the Node major version from tags like node:18-alpine or node:18.20.4
    if [[ "$image" =~ ^node:([0-9]+) ]]; then
      major="${BASH_REMATCH[1]}"
      for eol in $EOL_NODE_MAJORS; do
        if [ "$major" = "$eol" ]; then
          log_error "End-of-life Node base image in $rel_path: $image"
          eol_found=1
          entry=$(printf '{"file":"%s","image":"%s","nodeMajor":%s}' "$rel_path" "$image" "$major")
          if [ -z "$eol_entries" ]; then
            eol_entries="$entry"
          else
            eol_entries="$eol_entries,$entry"
          fi
          break
        fi
      done
    fi
  done < <(grep -iE '^[[:space:]]*FROM[[:space:]]+' "$dockerfile" | awk '{print $2}')
done

if [ "$eol_found" -eq 1 ]; then
  printf '{"status":"fail","eolImages":[%s]}\n' "$eol_entries" > "$EOL_REPORT"
  log_error "End-of-life Node base images detected (see $EOL_REPORT)"
  exit 1
else
  printf '{"status":"pass","eolImages":[]}\n' > "$EOL_REPORT"
  log_success "No end-of-life Node base images detected"
fi

exit 0
