#!/bin/bash

# Issue #1579: Generate Cargo SBOM
# Creates Software Bill of Materials for Rust dependencies

set -euo pipefail

REPO_ROOT="$(dirname "$(dirname "$(realpath "${BASH_SOURCE[0]}")")")"
SBOM_DIR="$REPO_ROOT/security/sbom"

log_info() { echo "ℹ️  $*" >&2; }
log_success() { echo "✅ $*" >&2; }

mkdir -p "$SBOM_DIR"

log_info "Generating Cargo SBOM..."

# Create SBOM header
cat > "$SBOM_DIR/sbom-cargo.json" <<EOF
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.4",
  "serializationFormat": "json",
  "version": 1,
  "metadata": {
    "timestamp": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "component": {
      "type": "application",
      "name": "QuorumCredit",
      "version": "$(git describe --tags 2>/dev/null || echo 'unknown')"
    }
  },
  "components": [
EOF

# Extract dependencies from Cargo.lock
if command -v cargo &> /dev/null; then
  cargo tree --depth 1 --prefix indent 2>/dev/null | grep -E '^[├└]' | while read -r line; do
    # Parse dependency info
    name=$(echo "$line" | sed -E 's/.*[├└]── ([^ ]+).*/\1/')
    version=$(echo "$line" | sed -E 's/.*[├└]── [^ ]+ v([^ ]*).*/\1/')

    if [ -n "$name" ] && [ -n "$version" ]; then
      cat >> "$SBOM_DIR/sbom-cargo.json" <<EOF
    {
      "type": "library",
      "name": "$name",
      "version": "$version",
      "purl": "pkg:cargo/$name@$version"
    },
EOF
    fi
  done
fi

# Close JSON
echo "  ]" >> "$SBOM_DIR/sbom-cargo.json"
echo "}" >> "$SBOM_DIR/sbom-cargo.json"

log_success "Cargo SBOM generated at $SBOM_DIR/sbom-cargo.json"
