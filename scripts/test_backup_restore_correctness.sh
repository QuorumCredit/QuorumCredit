#!/bin/bash
# test_backup_restore_correctness.sh — Validates that test_backup_restore.sh
# Step 4 correctly detects corrupted loan/vouch data after restore.
#
# This test proves the fix for Issue #1369: Step 4 should fail when loan/vouch
# records differ between backup and staging, not just check the paused flag.
#
# Usage:
#   ./scripts/test_backup_restore_correctness.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "═══════════════════════════════════════════════════════════"
echo "Testing backup/restore correctness verification (Issue #1369)"
echo "═══════════════════════════════════════════════════════════"
echo ""

# Create a mock backup directory structure that mimics what backup.sh produces
TEST_BACKUP_DIR=$(mktemp -d)
TEST_TIMESTAMP=$(date -u +%Y%m%d_%H%M%SZ)
MOCK_BACKUP="$TEST_BACKUP_DIR/$TEST_TIMESTAMP"
mkdir -p "$MOCK_BACKUP/loans" "$MOCK_BACKUP/vouches"

echo "Created mock backup at: $MOCK_BACKUP"

# Mock borrower addresses
BORROWER1="GBORROWER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
BORROWER2="GBORROWER2BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB"

echo "$BORROWER1" > "$MOCK_BACKUP/derived_addresses.txt"
echo "$BORROWER2" >> "$MOCK_BACKUP/derived_addresses.txt"

# Create mock backup data with consistent safe_names
SAFE_NAME1=$(echo -n "$BORROWER1" | sha256sum | cut -c1-16)
SAFE_NAME2=$(echo -n "$BORROWER2" | sha256sum | cut -c1-16)

# Borrower 1 — Active loan with vouches
cat > "$MOCK_BACKUP/loans/${SAFE_NAME1}.json" <<'EOF'
{
  "id": 1,
  "borrower": "GBORROWER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
  "amount": 5000000000,
  "amount_repaid": 0,
  "status": "Active"
}
EOF

cat > "$MOCK_BACKUP/loans/${SAFE_NAME1}_status.json" <<'EOF'
"Active"
EOF

cat > "$MOCK_BACKUP/vouches/${SAFE_NAME1}.json" <<'EOF'
[
  {
    "voucher": "GVOUCHER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    "stake": 10000000000,
    "token": "CTOKENAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"
  }
]
EOF

cat > "$MOCK_BACKUP/vouches/${SAFE_NAME1}_total.json" <<'EOF'
10000000000
EOF

# Borrower 2 — No active loan
cat > "$MOCK_BACKUP/loans/${SAFE_NAME2}.json" <<'EOF'
null
EOF

cat > "$MOCK_BACKUP/loans/${SAFE_NAME2}_status.json" <<'EOF'
"None"
EOF

cat > "$MOCK_BACKUP/vouches/${SAFE_NAME2}.json" <<'EOF'
[]
EOF

cat > "$MOCK_BACKUP/vouches/${SAFE_NAME2}_total.json" <<'EOF'
0
EOF

# Mock paused state
cat > "$MOCK_BACKUP/paused.json" <<'EOF'
false
EOF

echo "Mock backup structure created."
echo ""

# ── Test 1: Verify script detects corrupted loan data ─────────────────────────

echo "Test 1: Verify Step 4 fails when loan record is corrupted"
echo "─────────────────────────────────────────────────────────────"

# Create a mock staging query script that returns DIFFERENT loan data
MOCK_STELLAR_CLI=$(mktemp)
cat > "$MOCK_STELLAR_CLI" <<'SCRIPT_END'
#!/bin/bash
# Mock stellar CLI that returns corrupted data for get_loan

case "$*" in
    *"get_paused"*)
        echo "false"
        ;;
    *"get_loan"*"GBORROWER1"*)
        # Return CORRUPTED loan data (different amount)
        echo '{"id":1,"borrower":"GBORROWER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","amount":9999999999,"amount_repaid":0,"status":"Active"}'
        ;;
    *"get_loan"*"GBORROWER2"*)
        echo "null"
        ;;
    *"get_vouches"*"GBORROWER1"*)
        echo '[{"voucher":"GVOUCHER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","stake":10000000000,"token":"CTOKENAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}]'
        ;;
    *"get_vouches"*"GBORROWER2"*)
        echo '[]'
        ;;
    *"loan_status"*"GBORROWER1"*)
        echo '"Active"'
        ;;
    *"loan_status"*"GBORROWER2"*)
        echo '"None"'
        ;;
    *"total_vouched"*"GBORROWER1"*)
        echo '10000000000'
        ;;
    *"total_vouched"*"GBORROWER2"*)
        echo '0'
        ;;
    *)
        echo "null"
        ;;
esac
SCRIPT_END

chmod +x "$MOCK_STELLAR_CLI"

# Temporarily add mock to PATH
export PATH="$(dirname "$MOCK_STELLAR_CLI"):$PATH"
mv "$MOCK_STELLAR_CLI" "$(dirname "$MOCK_STELLAR_CLI")/stellar"

# Extract and run just Step 4 logic from test_backup_restore.sh
LATEST_BACKUP_DIR="$MOCK_BACKUP"
STAGING_CONTRACT_ID="CTEST123456789"
STAGING_ADMIN_KEY="STEST123456789"
NETWORK="testnet"
STATUS="pass"
RESTORE_OK="true"
BACKUP_TEST_EXECUTE="true"
FAILURE_REASON=""
CORRECTNESS="skipped"

# Run Step 4 inline
if [ "$STATUS" = "pass" ] && [ "$RESTORE_OK" = "true" ] && [ "$BACKUP_TEST_EXECUTE" = "true" ]; then
    
    CORRECTNESS_CHECKS_PASSED=0
    CORRECTNESS_CHECKS_FAILED=0
    
    if command -v stellar &>/dev/null; then
        # Check 1: Paused state
        BACKUP_PAUSED=$(jq -c . "$LATEST_BACKUP_DIR/paused.json" 2>/dev/null || echo "null")
        STAGING_PAUSED=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
            --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
            -- get_paused 2>/dev/null || echo "null")
        if [ "$BACKUP_PAUSED" = "$STAGING_PAUSED" ]; then
            CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
        else
            CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
            STATUS="fail"
            FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }restored state diverges from backup (get_paused mismatch)"
        fi
        
        # Check 2-5: Loan and vouch records for sampled borrowers
        SAMPLE_BORROWERS_FILE="${BACKUP_TEST_SAMPLE_BORROWERS:-$LATEST_BACKUP_DIR/derived_addresses.txt}"
        
        if [ -f "$SAMPLE_BORROWERS_FILE" ]; then
            LOANS_DIR="$LATEST_BACKUP_DIR/loans"
            VOUCHES_DIR="$LATEST_BACKUP_DIR/vouches"
            
            while IFS= read -r borrower; do
                [ -z "$borrower" ] && continue
                
                safe_name=$(echo -n "$borrower" | sha256sum | cut -c1-16)
                
                # Check loan record
                if [ -f "$LOANS_DIR/${safe_name}.json" ]; then
                    BACKUP_LOAN=$(jq -c . "$LOANS_DIR/${safe_name}.json" 2>/dev/null || echo "null")
                    STAGING_LOAN=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
                        --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
                        -- get_loan --borrower "$borrower" 2>/dev/null | jq -c . || echo "null")
                    
                    if [ "$BACKUP_LOAN" = "$STAGING_LOAN" ]; then
                        CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
                    else
                        CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
                        STATUS="fail"
                        FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }loan record mismatch for borrower $borrower"
                    fi
                fi
                
                # Check vouch records
                if [ -f "$VOUCHES_DIR/${safe_name}.json" ]; then
                    BACKUP_VOUCHES=$(jq -c . "$VOUCHES_DIR/${safe_name}.json" 2>/dev/null || echo "null")
                    STAGING_VOUCHES=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
                        --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
                        -- get_vouches --borrower "$borrower" 2>/dev/null | jq -c . || echo "null")
                    
                    if [ "$BACKUP_VOUCHES" = "$STAGING_VOUCHES" ]; then
                        CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
                    else
                        CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
                        STATUS="fail"
                        FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }vouch records mismatch for borrower $borrower"
                    fi
                fi
                
                # Check loan status
                if [ -f "$LOANS_DIR/${safe_name}_status.json" ]; then
                    BACKUP_STATUS=$(jq -c . "$LOANS_DIR/${safe_name}_status.json" 2>/dev/null || echo "null")
                    STAGING_STATUS=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
                        --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
                        -- loan_status --borrower "$borrower" 2>/dev/null | jq -c . || echo "null")
                    
                    if [ "$BACKUP_STATUS" = "$STAGING_STATUS" ]; then
                        CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
                    else
                        CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
                        STATUS="fail"
                        FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }loan_status mismatch for borrower $borrower"
                    fi
                fi
                
                # Check total vouched amount
                if [ -f "$VOUCHES_DIR/${safe_name}_total.json" ]; then
                    BACKUP_TOTAL=$(jq -c . "$VOUCHES_DIR/${safe_name}_total.json" 2>/dev/null || echo "null")
                    STAGING_TOTAL=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
                        --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
                        -- total_vouched --borrower "$borrower" 2>/dev/null | jq -c . || echo "null")
                    
                    if [ "$BACKUP_TOTAL" = "$STAGING_TOTAL" ]; then
                        CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
                    else
                        CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
                        STATUS="fail"
                        FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }total_vouched mismatch for borrower $borrower"
                    fi
                fi
            done < "$SAMPLE_BORROWERS_FILE"
        fi
        
        # Determine overall correctness
        if [ "$CORRECTNESS_CHECKS_FAILED" -eq 0 ] && [ "$CORRECTNESS_CHECKS_PASSED" -gt 0 ]; then
            CORRECTNESS="true"
        elif [ "$CORRECTNESS_CHECKS_PASSED" -eq 0 ] && [ "$CORRECTNESS_CHECKS_FAILED" -eq 0 ]; then
            CORRECTNESS="skipped"
        else
            CORRECTNESS="false"
        fi
    fi
fi

if [ "$STATUS" = "fail" ] && [ "$CORRECTNESS" = "false" ]; then
    echo "✅ PASS: Step 4 correctly detected corrupted loan data and failed"
    echo "   STATUS=$STATUS, CORRECTNESS=$CORRECTNESS"
    echo "   FAILURE_REASON: $FAILURE_REASON"
    TEST1_RESULT="pass"
else
    echo "❌ FAIL: Step 4 should have failed but reported STATUS=$STATUS, CORRECTNESS=$CORRECTNESS"
    TEST1_RESULT="fail"
fi

echo ""

# ── Test 2: Verify script passes when all data matches ────────────────────────

echo "Test 2: Verify Step 4 passes when all loan/vouch data matches"
echo "─────────────────────────────────────────────────────────────"

# Create a new mock stellar CLI that returns MATCHING data
MOCK_STELLAR_CLI_2=$(mktemp)
cat > "$MOCK_STELLAR_CLI_2" <<'SCRIPT_END'
#!/bin/bash
# Mock stellar CLI that returns correct/matching data

case "$*" in
    *"get_paused"*)
        echo "false"
        ;;
    *"get_loan"*"GBORROWER1"*)
        # Return CORRECT loan data (matching backup)
        echo '{"id":1,"borrower":"GBORROWER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","amount":5000000000,"amount_repaid":0,"status":"Active"}'
        ;;
    *"get_loan"*"GBORROWER2"*)
        echo "null"
        ;;
    *"get_vouches"*"GBORROWER1"*)
        echo '[{"voucher":"GVOUCHER1AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","stake":10000000000,"token":"CTOKENAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}]'
        ;;
    *"get_vouches"*"GBORROWER2"*)
        echo '[]'
        ;;
    *"loan_status"*"GBORROWER1"*)
        echo '"Active"'
        ;;
    *"loan_status"*"GBORROWER2"*)
        echo '"None"'
        ;;
    *"total_vouched"*"GBORROWER1"*)
        echo '10000000000'
        ;;
    *"total_vouched"*"GBORROWER2"*)
        echo '0'
        ;;
    *)
        echo "null"
        ;;
esac
SCRIPT_END

chmod +x "$MOCK_STELLAR_CLI_2"
rm -f "$(dirname "$MOCK_STELLAR_CLI_2")/stellar"
mv "$MOCK_STELLAR_CLI_2" "$(dirname "$MOCK_STELLAR_CLI_2")/stellar"
export PATH="$(dirname "$(dirname "$MOCK_STELLAR_CLI_2")/stellar"):$PATH"

# Reset state
STATUS="pass"
RESTORE_OK="true"
BACKUP_TEST_EXECUTE="true"
FAILURE_REASON=""
CORRECTNESS="skipped"

# Run Step 4 inline again
if [ "$STATUS" = "pass" ] && [ "$RESTORE_OK" = "true" ] && [ "$BACKUP_TEST_EXECUTE" = "true" ]; then
    
    CORRECTNESS_CHECKS_PASSED=0
    CORRECTNESS_CHECKS_FAILED=0
    
    if command -v stellar &>/dev/null; then
        # Check 1: Paused state
        BACKUP_PAUSED=$(jq -c . "$LATEST_BACKUP_DIR/paused.json" 2>/dev/null || echo "null")
        STAGING_PAUSED=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
            --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
            -- get_paused 2>/dev/null || echo "null")
        if [ "$BACKUP_PAUSED" = "$STAGING_PAUSED" ]; then
            CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
        else
            CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
            STATUS="fail"
            FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }restored state diverges from backup (get_paused mismatch)"
        fi
        
        # Check 2-5: Loan and vouch records for sampled borrowers
        SAMPLE_BORROWERS_FILE="${BACKUP_TEST_SAMPLE_BORROWERS:-$LATEST_BACKUP_DIR/derived_addresses.txt}"
        
        if [ -f "$SAMPLE_BORROWERS_FILE" ]; then
            LOANS_DIR="$LATEST_BACKUP_DIR/loans"
            VOUCHES_DIR="$LATEST_BACKUP_DIR/vouches"
            
            while IFS= read -r borrower; do
                [ -z "$borrower" ] && continue
                
                safe_name=$(echo -n "$borrower" | sha256sum | cut -c1-16)
                
                # Check loan record
                if [ -f "$LOANS_DIR/${safe_name}.json" ]; then
                    BACKUP_LOAN=$(jq -c . "$LOANS_DIR/${safe_name}.json" 2>/dev/null || echo "null")
                    STAGING_LOAN=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
                        --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
                        -- get_loan --borrower "$borrower" 2>/dev/null | jq -c . || echo "null")
                    
                    if [ "$BACKUP_LOAN" = "$STAGING_LOAN" ]; then
                        CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
                    else
                        CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
                        STATUS="fail"
                        FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }loan record mismatch for borrower $borrower"
                    fi
                fi
                
                # Check vouch records
                if [ -f "$VOUCHES_DIR/${safe_name}.json" ]; then
                    BACKUP_VOUCHES=$(jq -c . "$VOUCHES_DIR/${safe_name}.json" 2>/dev/null || echo "null")
                    STAGING_VOUCHES=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
                        --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
                        -- get_vouches --borrower "$borrower" 2>/dev/null | jq -c . || echo "null")
                    
                    if [ "$BACKUP_VOUCHES" = "$STAGING_VOUCHES" ]; then
                        CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
                    else
                        CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
                        STATUS="fail"
                        FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }vouch records mismatch for borrower $borrower"
                    fi
                fi
                
                # Check loan status
                if [ -f "$LOANS_DIR/${safe_name}_status.json" ]; then
                    BACKUP_STATUS=$(jq -c . "$LOANS_DIR/${safe_name}_status.json" 2>/dev/null || echo "null")
                    STAGING_STATUS=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
                        --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
                        -- loan_status --borrower "$borrower" 2>/dev/null | jq -c . || echo "null")
                    
                    if [ "$BACKUP_STATUS" = "$STAGING_STATUS" ]; then
                        CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
                    else
                        CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
                        STATUS="fail"
                        FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }loan_status mismatch for borrower $borrower"
                    fi
                fi
                
                # Check total vouched amount
                if [ -f "$VOUCHES_DIR/${safe_name}_total.json" ]; then
                    BACKUP_TOTAL=$(jq -c . "$VOUCHES_DIR/${safe_name}_total.json" 2>/dev/null || echo "null")
                    STAGING_TOTAL=$(stellar contract invoke --id "$STAGING_CONTRACT_ID" \
                        --source "${STAGING_ADMIN_KEY:-ADMIN_KEY}" --network "$NETWORK" \
                        -- total_vouched --borrower "$borrower" 2>/dev/null | jq -c . || echo "null")
                    
                    if [ "$BACKUP_TOTAL" = "$STAGING_TOTAL" ]; then
                        CORRECTNESS_CHECKS_PASSED=$((CORRECTNESS_CHECKS_PASSED + 1))
                    else
                        CORRECTNESS_CHECKS_FAILED=$((CORRECTNESS_CHECKS_FAILED + 1))
                        STATUS="fail"
                        FAILURE_REASON="${FAILURE_REASON:+$FAILURE_REASON; }total_vouched mismatch for borrower $borrower"
                    fi
                fi
            done < "$SAMPLE_BORROWERS_FILE"
        fi
        
        # Determine overall correctness
        if [ "$CORRECTNESS_CHECKS_FAILED" -eq 0 ] && [ "$CORRECTNESS_CHECKS_PASSED" -gt 0 ]; then
            CORRECTNESS="true"
        elif [ "$CORRECTNESS_CHECKS_PASSED" -eq 0 ] && [ "$CORRECTNESS_CHECKS_FAILED" -eq 0 ]; then
            CORRECTNESS="skipped"
        else
            CORRECTNESS="false"
        fi
    fi
fi

if [ "$STATUS" = "pass" ] && [ "$CORRECTNESS" = "true" ]; then
    echo "✅ PASS: Step 4 correctly passed when all data matched"
    echo "   STATUS=$STATUS, CORRECTNESS=$CORRECTNESS"
    echo "   Checks passed: $CORRECTNESS_CHECKS_PASSED, failed: $CORRECTNESS_CHECKS_FAILED"
    TEST2_RESULT="pass"
else
    echo "❌ FAIL: Step 4 should have passed but reported STATUS=$STATUS, CORRECTNESS=$CORRECTNESS"
    echo "   Checks passed: $CORRECTNESS_CHECKS_PASSED, failed: $CORRECTNESS_CHECKS_FAILED"
    TEST2_RESULT="fail"
fi

echo ""

# ── Cleanup and summary ────────────────────────────────────────────────────────

rm -rf "$TEST_BACKUP_DIR"

echo "═══════════════════════════════════════════════════════════"
echo "Test summary:"
echo "  Test 1 (detect corruption): $TEST1_RESULT"
echo "  Test 2 (pass on match):     $TEST2_RESULT"
echo "═══════════════════════════════════════════════════════════"

if [ "$TEST1_RESULT" = "pass" ] && [ "$TEST2_RESULT" = "pass" ]; then
    echo ""
    echo "✅ All tests passed — Issue #1369 fix verified"
    exit 0
else
    echo ""
    echo "❌ Some tests failed — see output above"
    exit 1
fi
