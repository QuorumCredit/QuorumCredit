#!/usr/bin/env bash
# Offline correctness/safety checks for the backup and restore scripts.
# The live drill needs staging secrets and remains scheduled/manual only.
set -euo pipefail

for script in scripts/backup.sh scripts/restore.sh scripts/test_backup_restore.sh; do
  if [[ ! -f "$script" ]]; then
    echo "::error::Missing backup/restore script: $script" >&2
    exit 1
  fi
  bash -n "$script"
done

if ! grep -q 'state' scripts/test_backup_restore.sh; then
  echo "::error::Backup drill does not perform a state comparison" >&2
  exit 1
fi

if [[ "${BACKUP_TEST_EXECUTE:-false}" != "true" ]]; then
  echo "OK: offline backup/restore correctness checks passed (live drill skipped)"
  exit 0
fi

echo "::notice::Running live backup/restore drill"
./scripts/test_backup_restore.sh --network "${NETWORK:-testnet}"
