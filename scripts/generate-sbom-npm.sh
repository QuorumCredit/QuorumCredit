#!/bin/bash

# Issue #1579: Generate npm SBOM
# Creates Software Bill of Materials for npm dependencies

set -euo pipefail

REPO_ROOT="$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"
SBOM_DIR="$REPO_ROOT/security/sbom"

log_info() { echo "ℹ️  $*" >&2; }
log_success() { echo "✅ $*" >&2; }

mkdir -p "$SBOM_DIR"

log_info "Generating npm SBOM..."

# Use cyclonedx-npm if available, otherwise use npm list
if command -v cyclonedx &> /dev/null; then
  cyclonedx --help > /dev/null 2>&1 && cyclonedx --output-file "$SBOM_DIR/sbom-npm.json" . || generate_npm_sbom
else
  generate_npm_sbom
fi

log_success "npm SBOM generated at $SBOM_DIR/sbom-npm.json"

generate_npm_sbom() {
  # Create SBOM from npm list
  npm list --depth=0 --json 2>/dev/null | jq '{
    bomFormat: "CycloneDX",
    specVersion: "1.4",
    serializationFormat: "json",
    version: 1,
    metadata: {
      timestamp: now | todate,
      component: {
        type: "application",
        name: "QuorumCredit",
        version: "unknown"
      }
    },
    components: [
      (.dependencies | to_entries[] | {
        type: "library",
        name: .key,
        version: .value.version,
        purl: "pkg:npm/\(.key)@\(.value.version)"
      })
    ]
  }' > "$SBOM_DIR/sbom-npm.json"
}
