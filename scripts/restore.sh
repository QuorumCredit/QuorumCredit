#!/bin/bash
# restore.sh — Disaster recovery restore script for QuorumCredit.
#
# Reads a backup archive produced by backup.sh and provides guided recovery
# procedures. For on-chain state, it prints the exact stellar CLI commands
# needed to restore each piece of state — it does NOT execute them automatically
# to prevent accidental overwrites. Pass --execute to apply changes.
#
# Usage:
#   ./scripts/restore.sh --backup <path-to-backup.tar.gz> [--network <network>] [--execute]
#
# Required environment variables (or .env entries):
#   CONTRACT_ID     — Deployed contract ID (C...)
#   ADMIN_KEY       — Admin secret key (S...)
#   NETWORK         — Stellar network: testnet | mainnet (default: testnet)
#
# Options:
#   --backup <path>   Path to backup archive (.tar.gz) or extracted directory
#   --network <net>   Override NETWORK env var
#   --execute         Actually execute the restore commands (default: dry-run)
#   --scenario <n>    Jump directly to a specific recovery scenario (1-5)
#
# Scenarios:
#   1 — Unpause contract (contract paused unexpectedly)
#   2 — Restore config from backup
#   3 — Verify yield reserve solvency
#   4 — Admin key rotation
#   5 — Full contract upgrade
#
# Example (dry-run):
#   CONTRACT_ID="C..." ADMIN_KEY="S..." ./scripts/restore.sh --backup backups/backup_20260530_120000Z.tar.gz
#
# Example (execute):
#   CONTRACT_ID="C..." ADMIN_KEY="S..." ./scripts/restore.sh --backup backups/backup_20260530_120000Z.tar.gz --execute

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

BACKUP_PATH=""
EXECUTE=false
SCENARIO=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --backup)   BACKUP_PATH="${2:?'--backup requires a path'}"; shift 2 ;;
        --network)  NETWORK="${2:?'--network requires a value'}"; shift 2 ;;
        --execute)  EXECUTE=true; shift ;;
        --scenario) SCENARIO="${2:?'--scenario requires a number'}"; shift 2 ;;
        *) echo "Error: Unknown argument: $1" >&2; exit 1 ;;
    esac
done

# ── Defaults ───────────────────────────────────────────────────────────────────

NETWORK="${NETWORK:-testnet}"

# ── Validate ───────────────────────────────────────────────────────────────────

for var in CONTRACT_ID ADMIN_KEY; do
    if [ -z "${!var:-}" ]; then
        echo "Error: $var is not set." >&2
        exit 1
    fi
done

if ! command -v stellar &>/dev/null; then
    echo "Error: 'stellar' CLI not found." >&2
    exit 1
fi

if ! command -v jq &>/dev/null; then
    echo "Error: 'jq' not found." >&2
    exit 1
fi

# ── Helper: run or print a command ────────────────────────────────────────────

run_or_print() {
    if [ "$EXECUTE" = true ]; then
        echo "  Executing: $*"
        "$@"
    else
        echo "  [DRY-RUN] Would run: $*"
    fi
}

# ── Helper: invoke a contract function ────────────────────────────────────────

invoke_fn() {
    local fn="$1"; shift
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" \
        --source "$ADMIN_KEY" \
        --network "$NETWORK" \
        -- "$fn" "$@"
}

# ── Extract backup if archive ──────────────────────────────────────────────────

BACKUP_DIR=""
if [ -n "$BACKUP_PATH" ]; then
    if [ ! -e "$BACKUP_PATH" ]; then
        echo "Error: Backup path not found: $BACKUP_PATH" >&2
        exit 1
    fi

    if [[ "$BACKUP_PATH" == *.tar.gz ]]; then
        BACKUP_DIR=$(mktemp -d)
        trap 'rm -rf "$BACKUP_DIR"' EXIT
        echo "Extracting $BACKUP_PATH ..."
        tar -xzf "$BACKUP_PATH" -C "$BACKUP_DIR"
        # Find the timestamped subdirectory
        BACKUP_DIR=$(find "$BACKUP_DIR" -mindepth 1 -maxdepth 1 -type d | head -1)
    else
        BACKUP_DIR="$BACKUP_PATH"
    fi

    if [ ! -f "$BACKUP_DIR/manifest.json" ]; then
        echo "Error: No manifest.json found in backup. Is this a valid QuorumCredit backup?" >&2
        exit 1
    fi

    BACKUP_TIMESTAMP=$(jq -r '.timestamp' "$BACKUP_DIR/manifest.json")
    BACKUP_NETWORK=$(jq -r '.network' "$BACKUP_DIR/manifest.json")
    BACKUP_CONTRACT=$(jq -r '.contract_id' "$BACKUP_DIR/manifest.json")

    echo "Backup info:"
    echo "  Timestamp   : $BACKUP_TIMESTAMP"
    echo "  Network     : $BACKUP_NETWORK"
    echo "  Contract    : $BACKUP_CONTRACT"
    echo ""

    if [ "$BACKUP_NETWORK" != "$NETWORK" ]; then
        echo "WARNING: Backup was taken from '$BACKUP_NETWORK' but restoring to '$NETWORK'." >&2
        echo "         Pass --network $BACKUP_NETWORK to match, or confirm this is intentional." >&2
        echo ""
    fi
fi

# ── Mode header ────────────────────────────────────────────────────────────────

if [ "$EXECUTE" = true ]; then
    echo "MODE: EXECUTE — commands will be applied to $NETWORK"
    if [ "$NETWORK" = "mainnet" ]; then
        echo ""
        echo "WARNING: You are restoring to MAINNET." >&2
        read -r -p "Type 'yes' to confirm: " CONFIRM
        [ "$CONFIRM" = "yes" ] || { echo "Aborted."; exit 1; }
    fi
else
    echo "MODE: DRY-RUN — no changes will be made (pass --execute to apply)"
fi
echo ""

# ── Scenario dispatcher ────────────────────────────────────────────────────────

run_scenario_1() {
    echo "=== Scenario 1: Unpause contract ==="
    echo "Use when: Contract is paused unexpectedly or after an emergency pause."
    echo ""

    echo "Step 1 — Check current pause state:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- get_paused

    echo ""
    echo "Step 2 — Check admin audit log for unauthorized pause:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- get_admin_audit_log

    echo ""
    echo "Step 3 — Unpause the contract:"
    invoke_fn unpause --admin_signers "[\"$ADMIN_KEY\"]"

    echo ""
    echo "Step 4 — Verify contract is unpaused:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- get_paused
}

run_scenario_2() {
    echo "=== Scenario 2: Restore config from backup ==="
    echo "Use when: Protocol config was incorrectly updated."
    echo ""

    if [ -z "$BACKUP_DIR" ]; then
        echo "Error: --backup is required for scenario 2." >&2
        exit 1
    fi

    CONFIG_FILE="$BACKUP_DIR/config.json"
    if [ ! -f "$CONFIG_FILE" ]; then
        echo "Error: config.json not found in backup." >&2
        exit 1
    fi

    echo "Backup config:"
    jq . "$CONFIG_FILE"
    echo ""

    echo "Current on-chain config:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- get_config

    echo ""
    echo "Step — Apply backup config:"
    BACKUP_CONFIG=$(cat "$CONFIG_FILE")
    invoke_fn set_config \
        --admin_signers "[\"$ADMIN_KEY\"]" \
        --config "$BACKUP_CONFIG"
}

run_scenario_3() {
    echo "=== Scenario 3: Verify yield reserve solvency ==="
    echo "Use when: Repayments fail with InsufficientFunds."
    echo ""

    echo "Step 1 — Check contract balance:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- get_contract_balance

    echo ""
    echo "Step 2 — Check protocol health:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- get_protocol_health

    echo ""
    echo "Step 3 — Check slash treasury balance:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- get_slash_treasury_balance

    echo ""
    echo "NOTE: To replenish the yield reserve, transfer XLM directly to the"
    echo "      contract address using the Stellar token interface."
    echo "      Contract ID: $CONTRACT_ID"
}

run_scenario_4() {
    echo "=== Scenario 4: Admin key rotation ==="
    echo "Use when: An admin key is compromised."
    echo ""

    NEW_ADMIN="${NEW_ADMIN_ADDRESS:-<NEW_ADMIN_ADDRESS>}"

    echo "Step 1 — Pause contract immediately:"
    invoke_fn pause --admin_signers "[\"$ADMIN_KEY\"]"

    echo ""
    echo "Step 2 — Review audit log for unauthorized actions:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- get_admin_audit_log

    echo ""
    echo "Step 3 — Add new admin (set NEW_ADMIN_ADDRESS env var):"
    invoke_fn add_admin \
        --admin_signers "[\"$ADMIN_KEY\"]" \
        --new_admin "$NEW_ADMIN"

    echo ""
    echo "Step 4 — Remove compromised admin:"
    echo "  [MANUAL] Call remove_admin with the compromised address after confirming new admin is active."

    echo ""
    echo "Step 5 — Unpause contract:"
    invoke_fn unpause --admin_signers "[\"$ADMIN_KEY\"]"
}

run_scenario_5() {
    echo "=== Scenario 5: Full contract upgrade ==="
    echo "Use when: A critical bug requires a WASM upgrade."
    echo ""

    NEW_WASM_HASH="${NEW_WASM_HASH:-<NEW_WASM_HASH>}"

    echo "Step 1 — Build new WASM:"
    run_or_print cargo build --target wasm32-unknown-unknown --release \
        --manifest-path "$PROJECT_ROOT/QuorumCredit/Cargo.toml"

    echo ""
    echo "Step 2 — Pause contract:"
    invoke_fn pause --admin_signers "[\"$ADMIN_KEY\"]"

    echo ""
    echo "Step 3 — Validate upgrade (set NEW_WASM_HASH env var):"
    invoke_fn validate_upgrade --new_wasm_hash "$NEW_WASM_HASH"

    echo ""
    echo "Step 4 — Upload new WASM and capture hash:"
    run_or_print stellar contract install \
        --wasm "$PROJECT_ROOT/target/wasm32-unknown-unknown/release/quorum_credit.wasm" \
        --source "$ADMIN_KEY" \
        --network "$NETWORK"

    echo ""
    echo "Step 5 — Execute upgrade:"
    invoke_fn upgrade \
        --admin_signers "[\"$ADMIN_KEY\"]" \
        --new_wasm_hash "$NEW_WASM_HASH"

    echo ""
    echo "Step 6 — Verify health after upgrade:"
    run_or_print stellar contract invoke \
        --id "$CONTRACT_ID" --source "$ADMIN_KEY" --network "$NETWORK" \
        -- health_check

    echo ""
    echo "Step 7 — Unpause contract:"
    invoke_fn unpause --admin_signers "[\"$ADMIN_KEY\"]"
}

# ── Run selected scenario(s) ───────────────────────────────────────────────────

if [ -n "$SCENARIO" ]; then
    case "$SCENARIO" in
        1) run_scenario_1 ;;
        2) run_scenario_2 ;;
        3) run_scenario_3 ;;
        4) run_scenario_4 ;;
        5) run_scenario_5 ;;
        *) echo "Error: Unknown scenario '$SCENARIO'. Valid: 1-5." >&2; exit 1 ;;
    esac
else
    # Interactive menu
    echo "Available recovery scenarios:"
    echo "  1 — Unpause contract"
    echo "  2 — Restore config from backup"
    echo "  3 — Verify yield reserve solvency"
    echo "  4 — Admin key rotation"
    echo "  5 — Full contract upgrade"
    echo ""
    read -r -p "Select scenario (1-5): " CHOICE
    case "$CHOICE" in
        1) run_scenario_1 ;;
        2) run_scenario_2 ;;
        3) run_scenario_3 ;;
        4) run_scenario_4 ;;
        5) run_scenario_5 ;;
        *) echo "Error: Invalid choice '$CHOICE'." >&2; exit 1 ;;
    esac
fi

echo ""
echo "Recovery procedure complete."
[ "$EXECUTE" = false ] && echo "Re-run with --execute to apply changes."
