#!/bin/bash

# Issue #1579: npm Dependencies Security Scan
# Scans npm dependencies for known vulnerabilities.
# Issue #2091: fails the build on high or critical advisories that are not
# accepted in security/audit-allowlist.json. An unreadable audit result fails.

set -euo pipefail

REPO_ROOT="$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"
REPORT_DIR="$REPO_ROOT/security/reports"
REPORT="$REPORT_DIR/npm-audit.json"

# shellcheck source=lib/audit-gate.sh
source "$REPO_ROOT/scripts/lib/audit-gate.sh"

log_info() { echo "ℹ️  $*" >&2; }
log_success() { echo "✅ $*" >&2; }
log_error() { echo "❌ $*" >&2; }
log_warn() { echo "⚠️  $*" >&2; }

mkdir -p "$REPORT_DIR"

log_info "Running npm audit for Node.js dependencies..."

# npm audit exits non-zero when it finds vulnerabilities; the JSON report decides.
set +e
npm audit --json > "$REPORT" 2>"$REPORT_DIR/npm-audit.stderr"
set -e

# A missing lockfile or registry error comes back as an "error" object. Fail closed.
if ! jq -e '.metadata' "$REPORT" >/dev/null 2>&1 || jq -e 'has("error")' "$REPORT" >/dev/null 2>&1; then
  log_error "npm audit did not produce a usable report (see $REPORT)"
  exit 1
fi

total=$(jq '.metadata.vulnerabilities.total // 0' "$REPORT")
high=$(jq '.metadata.vulnerabilities.high // 0' "$REPORT")
critical=$(jq '.metadata.vulnerabilities.critical // 0' "$REPORT")
log_info "npm Audit Results: total=$total high=$high critical=$critical"

# Advisories are listed under each affected package's "via" entries; string
# entries point at other packages and are skipped (their advisory is listed there).
candidates=$(jq -r '.vulnerabilities // {} | to_entries[] | .value as $v
  | ($v.via[]? | objects | [(.source | tostring), ($v.severity // "unknown")] | @tsv)' "$REPORT")

blocking=$(printf '%s\n' "$candidates" | audit_gate_blocking)

if [ -n "$blocking" ]; then
  log_error "Blocking npm advisories (high/critical and not allowlisted):"
  printf '%s\n' "$blocking" | sed 's/^/  - /' >&2
  exit 1
fi

log_success "npm audit: no blocking advisories (medium/low or allowlisted)"

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
