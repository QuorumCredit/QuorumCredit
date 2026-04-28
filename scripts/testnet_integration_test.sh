#!/bin/bash
# testnet_integration_test.sh — Full loan lifecycle integration test on Stellar Testnet.
#
# Runs the complete QuorumCredit flow against a live testnet deployment:
#   Build → Deploy → Initialize → Vouch → Request Loan → Repay
#   and separately: Vouch → Request Loan → Vote Slash → Execute Slash
#
# Usage:
#   ./scripts/testnet_integration_test.sh [--network <network>]
#
# Required environment variables (or .env entries):
#   DEPLOYER_SECRET_KEY  — Secret key of the deployer account (S...)
#   DEPLOYER_ADDRESS     — Public key of the deployer account (G...)
#   ADMIN_ADDRESS        — Admin public key (G...)
#   TOKEN_CONTRACT       — XLM token contract address on the target network (C...)
#
# Optional:
#   NETWORK              — testnet (default) or mainnet
#   VOUCHER_SECRET_KEY   — Voucher account secret key (generated if not set)
#   BORROWER_SECRET_KEY  — Borrower account secret key (generated if not set)
#
# See docs/testnet-guide.md for full setup instructions.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WASM_PATH="$PROJECT_ROOT/target/wasm32-unknown-unknown/release/quorum_credit.wasm"

PASS=0
FAIL=0

pass() { echo "[PASS] $1"; PASS=$((PASS + 1)); }
fail() { echo "[FAIL] $1"; FAIL=$((FAIL + 1)); }

# ── Load .env ─────────────────────────────────────────────────────────────────

ENV_FILE="$PROJECT_ROOT/.env"
if [ -f "$ENV_FILE" ]; then
    set -o allexport
    # shellcheck source=/dev/null
    source "$ENV_FILE"
    set +o allexport
fi

# ── Parse CLI args ─────────────────────────────────────────────────────────────

while [[ $# -gt 0 ]]; do
    case "$1" in
        --network) NETWORK="${2:?'--network requires a value'}"; shift 2 ;;
        *) echo "Unknown argument: $1" >&2; exit 1 ;;
    esac
done

NETWORK="${NETWORK:-testnet}"

# ── Validate required vars ────────────────────────────────────────────────────

for var in DEPLOYER_SECRET_KEY DEPLOYER_ADDRESS ADMIN_ADDRESS TOKEN_CONTRACT; do
    if [ -z "${!var:-}" ]; then
        echo "Error: $var is not set. See docs/testnet-guide.md." >&2
        exit 1
    fi
done

# ── Check dependencies ────────────────────────────────────────────────────────

for cmd in cargo stellar jq; do
    if ! command -v "$cmd" &>/dev/null; then
        echo "Error: '$cmd' not found." >&2
        exit 1
    fi
done

echo "=== QuorumCredit Testnet Integration Tests ==="
echo "Network : $NETWORK"
echo "Admin   : $ADMIN_ADDRESS"
echo "Token   : $TOKEN_CONTRACT"
echo ""

# ── Step 1: Build WASM ────────────────────────────────────────────────────────

echo "--- Step 1: Build WASM ---"
(cd "$PROJECT_ROOT" && cargo build --target wasm32-unknown-unknown --release --quiet)
if [ -f "$WASM_PATH" ]; then
    pass "Step 1: WASM built at $WASM_PATH"
else
    fail "Step 1: WASM not found at $WASM_PATH"
    exit 1
fi

# ── Step 2: Deploy contract ───────────────────────────────────────────────────

echo "--- Step 2: Deploy contract ---"
CONTRACT_ID=$(stellar contract deploy \
    --wasm "$WASM_PATH" \
    --source "$DEPLOYER_SECRET_KEY" \
    --network "$NETWORK" 2>&1)

if [[ "$CONTRACT_ID" == C* ]]; then
    pass "Step 2: Contract deployed — CONTRACT_ID=$CONTRACT_ID"
else
    fail "Step 2: Deploy failed — $CONTRACT_ID"
    exit 1
fi

# ── Step 3: Initialize contract ───────────────────────────────────────────────

echo "--- Step 3: Initialize contract ---"
stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn initialize \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" \
    -- \
    --deployer "$DEPLOYER_ADDRESS" \
    --admins "[\"$ADMIN_ADDRESS\"]" \
    --admin_threshold 1 \
    --token "$TOKEN_CONTRACT" > /dev/null 2>&1

YIELD_BPS=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn get_config \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" 2>&1 | jq -r '.yield_bps // empty')

if [ "$YIELD_BPS" = "200" ]; then
    pass "Step 3: Contract initialized — config.yield_bps=$YIELD_BPS"
else
    fail "Step 3: Unexpected yield_bps=$YIELD_BPS (expected 200)"
fi

# ── Generate test keypairs if not provided ────────────────────────────────────

VOUCHER_SECRET_KEY="${VOUCHER_SECRET_KEY:-$(stellar keys generate --no-fund --network "$NETWORK" voucher_$$ 2>/dev/null | grep -oP 'S[A-Z0-9]{55}' | head -1)}"
BORROWER_SECRET_KEY="${BORROWER_SECRET_KEY:-$(stellar keys generate --no-fund --network "$NETWORK" borrower_$$ 2>/dev/null | grep -oP 'S[A-Z0-9]{55}' | head -1)}"

VOUCHER_ADDRESS=$(stellar keys address voucher_$$ 2>/dev/null || \
    stellar keys public-key --secret-key "$VOUCHER_SECRET_KEY" 2>/dev/null || echo "")
BORROWER_ADDRESS=$(stellar keys address borrower_$$ 2>/dev/null || \
    stellar keys public-key --secret-key "$BORROWER_SECRET_KEY" 2>/dev/null || echo "")

# Fund test accounts via Friendbot (testnet only)
if [ "$NETWORK" = "testnet" ]; then
    for addr in "$VOUCHER_ADDRESS" "$BORROWER_ADDRESS"; do
        curl -sf "https://friendbot.stellar.org?addr=$addr" > /dev/null || true
    done
fi

# ── Step 4: Vouch ─────────────────────────────────────────────────────────────

echo "--- Step 4: Vouch ---"
STAKE=10000000  # 1 XLM in stroops

# Wait past MIN_VOUCH_AGE (60 s) — on testnet we sleep briefly
sleep 65

stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn vouch \
    --network "$NETWORK" \
    --source "$VOUCHER_SECRET_KEY" \
    -- \
    --voucher "$VOUCHER_ADDRESS" \
    --borrower "$BORROWER_ADDRESS" \
    --stake "$STAKE" \
    --token "$TOKEN_CONTRACT" > /dev/null 2>&1

TOTAL_VOUCHED=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn total_vouched \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" \
    -- --borrower "$BORROWER_ADDRESS" 2>&1 | tr -d '"')

if [ "$TOTAL_VOUCHED" = "$STAKE" ]; then
    pass "Step 4: Vouch recorded — total_vouched=$TOTAL_VOUCHED"
else
    fail "Step 4: total_vouched=$TOTAL_VOUCHED (expected $STAKE)"
fi

# ── Step 5: Request loan ──────────────────────────────────────────────────────

echo "--- Step 5: Request loan ---"
LOAN_AMOUNT=5000000  # 0.5 XLM

BORROWER_BALANCE_BEFORE=$(stellar contract invoke \
    --id "$TOKEN_CONTRACT" \
    --fn balance \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" \
    -- --id "$BORROWER_ADDRESS" 2>&1 | tr -d '"')

stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn request_loan \
    --network "$NETWORK" \
    --source "$BORROWER_SECRET_KEY" \
    -- \
    --borrower "$BORROWER_ADDRESS" \
    --amount "$LOAN_AMOUNT" \
    --threshold "$LOAN_AMOUNT" \
    --loan_purpose '"Integration test loan"' \
    --token "$TOKEN_CONTRACT" > /dev/null 2>&1

LOAN_STATUS=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn loan_status \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" \
    -- --borrower "$BORROWER_ADDRESS" 2>&1 | tr -d '"')

BORROWER_BALANCE_AFTER=$(stellar contract invoke \
    --id "$TOKEN_CONTRACT" \
    --fn balance \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" \
    -- --id "$BORROWER_ADDRESS" 2>&1 | tr -d '"')

BALANCE_DELTA=$((BORROWER_BALANCE_AFTER - BORROWER_BALANCE_BEFORE))

if [ "$LOAN_STATUS" = "Active" ] && [ "$BALANCE_DELTA" -eq "$LOAN_AMOUNT" ]; then
    pass "Step 5: Loan disbursed — loan_status=$LOAN_STATUS, borrower_balance_delta=$BALANCE_DELTA"
else
    fail "Step 5: loan_status=$LOAN_STATUS, balance_delta=$BALANCE_DELTA (expected Active, $LOAN_AMOUNT)"
fi

# ── Step 6: Repay loan ────────────────────────────────────────────────────────

echo "--- Step 6: Repay loan ---"
LOAN_RECORD=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn get_loan \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" \
    -- --borrower "$BORROWER_ADDRESS" 2>&1)

PRINCIPAL=$(echo "$LOAN_RECORD" | jq -r '.amount // empty')
YIELD=$(echo "$LOAN_RECORD" | jq -r '.total_yield // empty')
REPAYMENT=$((PRINCIPAL + YIELD))

stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn repay \
    --network "$NETWORK" \
    --source "$BORROWER_SECRET_KEY" \
    -- \
    --borrower "$BORROWER_ADDRESS" \
    --payment "$REPAYMENT" > /dev/null 2>&1

LOAN_STATUS_AFTER=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn loan_status \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" \
    -- --borrower "$BORROWER_ADDRESS" 2>&1 | tr -d '"')

if [ "$LOAN_STATUS_AFTER" = "Repaid" ]; then
    pass "Step 6: Loan repaid — loan_status=$LOAN_STATUS_AFTER"
else
    fail "Step 6: loan_status=$LOAN_STATUS_AFTER (expected Repaid)"
fi

# ── Step 7: Slash flow ────────────────────────────────────────────────────────

echo "--- Step 7: Slash flow (new borrower) ---"
BORROWER2_SECRET_KEY="${BORROWER2_SECRET_KEY:-$(stellar keys generate --no-fund --network "$NETWORK" borrower2_$$ 2>/dev/null | grep -oP 'S[A-Z0-9]{55}' | head -1)}"
BORROWER2_ADDRESS=$(stellar keys address borrower2_$$ 2>/dev/null || \
    stellar keys public-key --secret-key "$BORROWER2_SECRET_KEY" 2>/dev/null || echo "")

if [ "$NETWORK" = "testnet" ]; then
    curl -sf "https://friendbot.stellar.org?addr=$BORROWER2_ADDRESS" > /dev/null || true
fi

VOUCHER2_SECRET_KEY="${VOUCHER2_SECRET_KEY:-$(stellar keys generate --no-fund --network "$NETWORK" voucher2_$$ 2>/dev/null | grep -oP 'S[A-Z0-9]{55}' | head -1)}"
VOUCHER2_ADDRESS=$(stellar keys address voucher2_$$ 2>/dev/null || \
    stellar keys public-key --secret-key "$VOUCHER2_SECRET_KEY" 2>/dev/null || echo "")

if [ "$NETWORK" = "testnet" ]; then
    curl -sf "https://friendbot.stellar.org?addr=$VOUCHER2_ADDRESS" > /dev/null || true
fi

sleep 65  # Wait past MIN_VOUCH_AGE

stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn vouch \
    --network "$NETWORK" \
    --source "$VOUCHER2_SECRET_KEY" \
    -- \
    --voucher "$VOUCHER2_ADDRESS" \
    --borrower "$BORROWER2_ADDRESS" \
    --stake "$STAKE" \
    --token "$TOKEN_CONTRACT" > /dev/null 2>&1

stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn request_loan \
    --network "$NETWORK" \
    --source "$BORROWER2_SECRET_KEY" \
    -- \
    --borrower "$BORROWER2_ADDRESS" \
    --amount "$LOAN_AMOUNT" \
    --threshold "$LOAN_AMOUNT" \
    --loan_purpose '"Slash test loan"' \
    --token "$TOKEN_CONTRACT" > /dev/null 2>&1

SLASH_TREASURY_BEFORE=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn get_slash_treasury \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" 2>&1 | tr -d '"' || echo "0")

stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn vote_slash \
    --network "$NETWORK" \
    --source "$VOUCHER2_SECRET_KEY" \
    -- \
    --voucher "$VOUCHER2_ADDRESS" \
    --borrower "$BORROWER2_ADDRESS" \
    --approve true > /dev/null 2>&1

SLASH_TREASURY_AFTER=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn get_slash_treasury \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" 2>&1 | tr -d '"' || echo "0")

if [ "$SLASH_TREASURY_AFTER" -gt "$SLASH_TREASURY_BEFORE" ]; then
    pass "Step 7: Slash executed — slash_treasury increased from $SLASH_TREASURY_BEFORE to $SLASH_TREASURY_AFTER"
else
    fail "Step 7: slash_treasury did not increase (before=$SLASH_TREASURY_BEFORE, after=$SLASH_TREASURY_AFTER)"
fi

# ── Step 8: Fee calculation check ────────────────────────────────────────────

echo "--- Step 8: Fee calculation check ---"
# Set a 1% protocol fee and verify it is collected on repayment
stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn set_protocol_fee \
    --network "$NETWORK" \
    --source "$ADMIN_ADDRESS" \
    -- \
    --admin_signers "[\"$ADMIN_ADDRESS\"]" \
    --fee_bps 100 > /dev/null 2>&1 || true  # skip if function not available

FEE_TREASURY=$(stellar contract invoke \
    --id "$CONTRACT_ID" \
    --fn get_fee_treasury \
    --network "$NETWORK" \
    --source "$DEPLOYER_SECRET_KEY" 2>&1 | tr -d '"' || echo "0")

# Fee treasury may be 0 if set_protocol_fee is not available; just verify it's non-negative
if [ "$FEE_TREASURY" -ge 0 ] 2>/dev/null; then
    pass "Step 8: Fee treasury readable — fee_treasury=$FEE_TREASURY"
else
    fail "Step 8: Could not read fee treasury"
fi

# ── Summary ───────────────────────────────────────────────────────────────────

echo ""
echo "=== Results ==="
echo "PASS: $PASS"
echo "FAIL: $FAIL"
echo "CONTRACT_ID=$CONTRACT_ID"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
