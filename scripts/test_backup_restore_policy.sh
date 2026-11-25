#!/usr/bin/env bash
# Verifies that restore stays dry-run by default and requires an explicit opt-in.
set -euo pipefail

script="scripts/restore.sh"
if [[ ! -f "$script" ]]; then
  echo "::error::Missing restore script: $script" >&2
  exit 1
fi
bash -n "$script"

if ! grep -q -- '--execute' "$script"; then
  echo "::error::$script has no explicit --execute opt-in" >&2
  exit 1
fi
if ! grep -q 'EXECUTE=false' "$script"; then
  echo "::error::$script must default to dry-run" >&2
  exit 1
fi

echo "OK: restore safety policy verified (dry-run by default)"
