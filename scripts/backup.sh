#!/bin/bash
# backup.sh — Automated contract state snapshot for QuorumCredit.
#
# Queries all critical on-chain state and writes JSON snapshots to a timestamped
# directory under ./backups/. Optionally compresses and uploads to S3.
#
# Issue #1146: the per-borrower address set is derived from the indexer's
# recorded event history (every address that has ever appeared in a `loan` or
# `vouch` event) instead of relying solely on an operator-maintained
# BORROWER_ADDRESSES list, so an address nobody remembered to add is no longer
# silently absent from the backup. BORROWER_ADDRESSES, if set, is still merged
# in (belt-and-suspenders for addresses the indexer hasn't ingested yet, e.g.
# during backfill). The manifest also records a completeness proof — the
# derived address count vs. the on-chain `get_borrower_count()` ground truth —
# and a checksum of every backed-up file, so an operator can verify nothing
# was silently skipped or corrupted.
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
#   INDEXER_DB_PATH     — Path to the quorum-credit-indexer SQLite DB (default: indexer.db).
#                         Used to derive the backed-up borrower address set.
#   INDEXER_BIN         — Path to the quorum-credit-indexer binary (default: looks for a
#                         release build, falling back to `cargo run -p quorum-credit-indexer`).
#   BORROWER_ADDRESSES  — Space-separated list of additional borrower addresses to
#                         snapshot even if absent from the indexer DB (supplementary, not required).
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

# ── 2. Derive the borrower address set (Issue #1146) ──────────────────────────
#
# Ground truth is the indexer's recorded event history, not a manually
# maintained list: any address that has ever appeared in a loan or vouch
# event is included, whether or not an operator remembered to add it anywhere.

INDEXER_DB_PATH="${INDEXER_DB_PATH:-$PROJECT_ROOT/indexer.db}"
DERIVED_ADDRESSES_FILE="$OUTPUT_DIR/derived_addresses.txt"
touch "$DERIVED_ADDRESSES_FILE"
INDEXER_ADDRESS_COUNT=0

if [ -f "$INDEXER_DB_PATH" ]; then
    INDEXER_BIN="${INDEXER_BIN:-}"
    if [ -z "$INDEXER_BIN" ]; then
        if [ -x "$PROJECT_ROOT/target/release/quorum-credit-indexer" ]; then
            INDEXER_BIN="$PROJECT_ROOT/target/release/quorum-credit-indexer"
        fi
    fi

    echo "Deriving borrower address set from indexer DB: $INDEXER_DB_PATH"
    if [ -n "$INDEXER_BIN" ]; then
        "$INDEXER_BIN" --contract-id "$CONTRACT_ID" --db-path "$INDEXER_DB_PATH" \
            --export-addresses >> "$DERIVED_ADDRESSES_FILE" 2>/dev/null || true
    elif command -v cargo &>/dev/null; then
        (cd "$PROJECT_ROOT" && cargo run --quiet -p quorum-credit-indexer --release -- \
            --contract-id "$CONTRACT_ID" --db-path "$INDEXER_DB_PATH" \
            --export-addresses) >> "$DERIVED_ADDRESSES_FILE" 2>/dev/null || true
    elif command -v sqlite3 &>/dev/null; then
        # Fallback: query the indexer's SQLite views directly (schema documented
        # in docs/event-indexing-guide.md) if neither the built binary nor cargo
        # is available on this host.
        sqlite3 "$INDEXER_DB_PATH" \
            "SELECT DISTINCT borrower FROM (
               SELECT json_extract(value_json,'\$.borrower') AS borrower FROM events WHERE category='loan'
               UNION
               SELECT json_extract(value_json,'\$.borrower') AS borrower FROM events WHERE category='vouch'
             ) WHERE borrower IS NOT NULL ORDER BY borrower;" \
            >> "$DERIVED_ADDRESSES_FILE" 2>/dev/null || true
    else
        echo "Warning: indexer DB found but no indexer binary, cargo, or sqlite3 available to query it." >&2
    fi
    INDEXER_ADDRESS_COUNT=$(wc -l < "$DERIVED_ADDRESSES_FILE" | tr -d ' ')
else
    echo "Warning: indexer DB not found at $INDEXER_DB_PATH — falling back to BORROWER_ADDRESSES only." >&2
    echo "         Any borrower not listed there will be silently absent from this backup." >&2
fi

# Merge in any manually supplied addresses (supplementary, never required).
for borrower in ${BORROWER_ADDRESSES:-}; do
    echo "$borrower" >> "$DERIVED_ADDRESSES_FILE"
done

sort -u -o "$DERIVED_ADDRESSES_FILE" "$DERIVED_ADDRESSES_FILE"
sed -i '/^$/d' "$DERIVED_ADDRESSES_FILE" 2>/dev/null || true
DERIVED_ADDRESS_COUNT=$(wc -l < "$DERIVED_ADDRESSES_FILE" | tr -d ' ')

echo "  Derived from indexer : $INDEXER_ADDRESS_COUNT address(es)"
echo "  Total after merge    : $DERIVED_ADDRESS_COUNT address(es)"

# ── 3. Completeness proof (Issue #1146) ────────────────────────────────────────
#
# Cross-check the derived set against on-chain ground truth: get_borrower_count()
# reflects every borrower the contract has ever registered. If the derived set
# is short, the manifest records exactly how many addresses were potentially
# missed instead of silently shipping an incomplete backup.

ONCHAIN_BORROWER_COUNT="null"
COMPLETENESS_FILE="$OUTPUT_DIR/completeness.json"
if RAW_COUNT=$(stellar contract invoke --id "$CONTRACT_ID" --source "$ADMIN_KEY" \
    --network "$NETWORK" -- get_borrower_count 2>/dev/null); then
    ONCHAIN_BORROWER_COUNT=$(echo "$RAW_COUNT" | tr -d '[:space:]"')
    [[ "$ONCHAIN_BORROWER_COUNT" =~ ^[0-9]+$ ]] || ONCHAIN_BORROWER_COUNT="null"
fi

if [ "$ONCHAIN_BORROWER_COUNT" != "null" ]; then
    if [ "$DERIVED_ADDRESS_COUNT" -ge "$ONCHAIN_BORROWER_COUNT" ]; then
        COMPLETE="true"
        MISSING_ESTIMATE=0
    else
        COMPLETE="false"
        MISSING_ESTIMATE=$((ONCHAIN_BORROWER_COUNT - DERIVED_ADDRESS_COUNT))
        echo "Warning: derived address set ($DERIVED_ADDRESS_COUNT) is short of the" >&2
        echo "         on-chain borrower count ($ONCHAIN_BORROWER_COUNT) by $MISSING_ESTIMATE." >&2
        echo "         The indexer may be behind — check its sync status before trusting this backup." >&2
    fi
else
    COMPLETE="unknown"
    MISSING_ESTIMATE="null"
    echo "Warning: could not read on-chain get_borrower_count() — completeness cannot be verified." >&2
fi

cat > "$COMPLETENESS_FILE" <<EOF
{
  "onchain_borrower_count": $ONCHAIN_BORROWER_COUNT,
  "derived_address_count": $DERIVED_ADDRESS_COUNT,
  "indexer_derived_count": $INDEXER_ADDRESS_COUNT,
  "complete": "$COMPLETE",
  "missing_estimate": $MISSING_ESTIMATE
}
EOF

if [ "$COMPLETE" = "false" ]; then
    ERRORS=$((ERRORS + 1))
fi

# ── 4. Per-borrower state ──────────────────────────────────────────────────────

if [ "$DERIVED_ADDRESS_COUNT" -gt 0 ]; then
    echo ""
    echo "Snapshotting per-borrower state for $DERIVED_ADDRESS_COUNT address(es)..."
    LOANS_DIR="$OUTPUT_DIR/loans"
    VOUCHES_DIR="$OUTPUT_DIR/vouches"
    mkdir -p "$LOANS_DIR" "$VOUCHES_DIR"

    while IFS= read -r borrower; do
        [ -z "$borrower" ] && continue
        safe_name=$(echo -n "$borrower" | sha256sum | cut -c1-16)
        echo "$borrower" > "$LOANS_DIR/${safe_name}.address"
        invoke_query get_loan    "$LOANS_DIR/${safe_name}.json"   --borrower "$borrower"
        invoke_query get_vouches "$VOUCHES_DIR/${safe_name}.json" --borrower "$borrower"
        invoke_query loan_status "$LOANS_DIR/${safe_name}_status.json" --borrower "$borrower"
        invoke_query total_vouched "$VOUCHES_DIR/${safe_name}_total.json" --borrower "$borrower"
    done < "$DERIVED_ADDRESSES_FILE"
fi

# ── 3. Write manifest ──────────────────────────────────────────────────────────

# Issue #1146: a checksum over every backed-up file's contents, so an operator
# can verify the archive wasn't silently truncated or corrupted after the fact
# (e.g. after copying to cold storage or S3) independent of the completeness
# proof above (which only covers address coverage, not byte-level integrity).
BACKUP_CHECKSUM=$(find "$OUTPUT_DIR" -type f ! -name "manifest.json" | sort | xargs sha256sum | sha256sum | cut -d' ' -f1)

cat > "$MANIFEST" <<EOF
{
  "timestamp": "$TIMESTAMP",
  "network": "$NETWORK",
  "contract_id": "$CONTRACT_ID",
  "errors": $ERRORS,
  "checksum": "sha256:$BACKUP_CHECKSUM",
  "completeness": $(cat "$COMPLETENESS_FILE"),
  "files": $(find "$OUTPUT_DIR" -type f ! -name "manifest.json" | sort | jq -R . | jq -s .)
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
