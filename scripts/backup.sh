#!/bin/bash
# backup.sh — Automated contract state snapshot for QuorumCredit.
#
# Queries all critical on-chain state and writes JSON snapshots to a timestamped
# directory under ./backups/. Optionally compresses and uploads to S3.
#
# Usage:
#   ./scripts/backup.sh [--network <network>] [--output-dir <dir>] [--s3-bucket <bucket>]
#
# Required environment variables (or .env entries):
#   CONTRACT_ID         — Deployed contract ID (C...)
#   ADMIN_KEY           — Secret key for read-only queries (S...)
#   NETWORK             — Stellar network: testnet | mainnet (default: testnet)
#
# Optional environment variables:
#   BORROWER_ADDRESSES  — Space-separated list of borrower addresses to snapshot
#   S3_BUCKET           — S3 bucket name for remote backup (e.g. my-backup-bucket)
#   BACKUP_RETENTION_DAYS — Days to keep local backups (default: 30)
#
# Example:
#   CONTRACT_ID="C..." ADMIN_KEY="S..." NETWORK=testnet ./scripts/backup.sh
#   CONTRACT_ID="C..." ADMIN_KEY="S..." S3_BUCKET="my-bucket" ./scripts/backup.sh --network mainnet

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# ── Load .env if present ───────────────────────────────────────────────────────

ENV_FILE="$PROJECT_ROOT/.env"
if [ -f "$ENV_FILE" ]; then
    set -o allexport
    # shellcheck source=/dev/null
    source "$ENV_FILE"
    set +o allexport
fi

# ── Parse CLI arguments ────────────────────────────────────────────────────────

OUTPUT_DIR=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --network)    NETWORK="${2:?'--network requires a value'}"; shift 2 ;;
        --output-dir) OUTPUT_DIR="${2:?'--output-dir requires a value'}"; shift 2 ;;
        --s3-bucket)  S3_BUCKET="${2:?'--s3-bucket requires a value'}"; shift 2 ;;
        *) echo "Error: Unknown argument: $1" >&2; exit 1 ;;
    esac
done

# ── Defaults ───────────────────────────────────────────────────────────────────

NETWORK="${NETWORK:-testnet}"
BACKUP_RETENTION_DAYS="${BACKUP_RETENTION_DAYS:-30}"
TIMESTAMP=$(date -u +%Y%m%d_%H%M%SZ)
OUTPUT_DIR="${OUTPUT_DIR:-$PROJECT_ROOT/backups/$TIMESTAMP}"

# ── Validate required variables ───────────────────────────────────────────────

for var in CONTRACT_ID ADMIN_KEY; do
    if [ -z "${!var:-}" ]; then
        echo "Error: $var is not set." >&2
        exit 1
    fi
done

if ! command -v stellar &>/dev/null; then
    echo "Error: 'stellar' CLI not found. Install with: cargo install --locked stellar-cli" >&2
    exit 1
fi

if ! command -v jq &>/dev/null; then
    echo "Error: 'jq' not found. Install with: apt-get install jq" >&2
    exit 1
fi

# ── Setup output directory ─────────────────────────────────────────────────────

mkdir -p "$OUTPUT_DIR"
MANIFEST="$OUTPUT_DIR/manifest.json"
ERRORS=0

echo "QuorumCredit backup — $TIMESTAMP"
echo "  Network     : $NETWORK"
echo "  Contract    : $CONTRACT_ID"
echo "  Output dir  : $OUTPUT_DIR"
echo ""

# ── Helper: invoke a read-only contract function ───────────────────────────────

invoke_query() {
    local fn="$1"
    local output_file="$2"
    shift 2
    local extra_args=("$@")

    if stellar contract invoke \
        --id "$CONTRACT_ID" \
        --source "$ADMIN_KEY" \
        --network "$NETWORK" \
        -- "$fn" "${extra_args[@]}" \
        > "$output_file" 2>/dev/null; then
        echo "  [OK]  $fn"
    else
        echo "  [ERR] $fn — skipped (function may not be available on this network)"
        echo "null" > "$output_file"
        ERRORS=$((ERRORS + 1))
    fi
}

# ── 1. Protocol-level state ────────────────────────────────────────────────────

echo "Snapshotting protocol state..."
invoke_query get_config          "$OUTPUT_DIR/config.json"
invoke_query get_admins          "$OUTPUT_DIR/admins.json"
invoke_query get_paused          "$OUTPUT_DIR/paused.json"
invoke_query get_pause_status    "$OUTPUT_DIR/pause_status.json"
invoke_query get_contract_balance "$OUTPUT_DIR/contract_balance.json"
invoke_query get_slash_treasury_balance "$OUTPUT_DIR/slash_treasury.json"
invoke_query get_fee_treasury    "$OUTPUT_DIR/fee_treasury.json"
invoke_query get_admin_audit_log "$OUTPUT_DIR/admin_audit_log.json"
invoke_query health_check        "$OUTPUT_DIR/health.json"
invoke_query get_protocol_health "$OUTPUT_DIR/protocol_health.json"
invoke_query is_initialized      "$OUTPUT_DIR/is_initialized.json"

# ── 2. Per-borrower state ──────────────────────────────────────────────────────

if [ -n "${BORROWER_ADDRESSES:-}" ]; then
    echo ""
    echo "Snapshotting per-borrower state..."
    LOANS_DIR="$OUTPUT_DIR/loans"
    VOUCHES_DIR="$OUTPUT_DIR/vouches"
    mkdir -p "$LOANS_DIR" "$VOUCHES_DIR"

    for borrower in $BORROWER_ADDRESSES; do
        safe_name="${borrower:0:8}"
        invoke_query get_loan    "$LOANS_DIR/${safe_name}.json"   --borrower "$borrower"
        invoke_query get_vouches "$VOUCHES_DIR/${safe_name}.json" --borrower "$borrower"
        invoke_query loan_status "$LOANS_DIR/${safe_name}_status.json" --borrower "$borrower"
        invoke_query total_vouched "$VOUCHES_DIR/${safe_name}_total.json" --borrower "$borrower"
    done
fi

# ── 3. Write manifest ──────────────────────────────────────────────────────────

cat > "$MANIFEST" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "network": "$NETWORK",
  "contract_id": "$CONTRACT_ID",
  "errors": $ERRORS,
  "files": $(find "$OUTPUT_DIR" -name "*.json" ! -name "manifest.json" | sort | jq -R . | jq -s .)
}
EOF

echo ""
echo "Manifest written: $MANIFEST"

# ── 4. Compress archive ────────────────────────────────────────────────────────

ARCHIVE="$PROJECT_ROOT/backups/backup_${TIMESTAMP}.tar.gz"
tar -czf "$ARCHIVE" -C "$PROJECT_ROOT/backups" "$TIMESTAMP"
echo "Archive created: $ARCHIVE"

# ── 5. Upload to S3 (optional) ────────────────────────────────────────────────

if [ -n "${S3_BUCKET:-}" ]; then
    if ! command -v aws &>/dev/null; then
        echo "Warning: 'aws' CLI not found — skipping S3 upload." >&2
    else
        S3_KEY="quorumcredit-backups/$NETWORK/backup_${TIMESTAMP}.tar.gz"
        echo "Uploading to s3://$S3_BUCKET/$S3_KEY ..."
        aws s3 cp "$ARCHIVE" "s3://$S3_BUCKET/$S3_KEY" --quiet
        echo "Upload complete."
    fi
fi

# ── 6. Prune old local backups ─────────────────────────────────────────────────

find "$PROJECT_ROOT/backups" -maxdepth 1 -name "backup_*.tar.gz" \
    -mtime "+$BACKUP_RETENTION_DAYS" -delete 2>/dev/null || true
find "$PROJECT_ROOT/backups" -maxdepth 1 -mindepth 1 -type d \
    -mtime "+$BACKUP_RETENTION_DAYS" -exec rm -rf {} + 2>/dev/null || true

# ── Summary ────────────────────────────────────────────────────────────────────

echo ""
if [ "$ERRORS" -eq 0 ]; then
    echo "Backup completed successfully."
else
    echo "Backup completed with $ERRORS query error(s). Check output above."
    exit 1
fi
