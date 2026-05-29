# Threat Model: Yield Reserve Depletion

## Executive Summary

The yield reserve is critical to protocol solvency. This document identifies attack vectors targeting reserve depletion and mitigation strategies.

## Threat: Reserve Draining Attack

### Attack Vector 1: Yield Over-Promising

**Description:** Attacker manipulates yield rate to exceed reserve capacity.

**Preconditions:**
- Attacker controls admin multisig (compromised key)
- Yield rate set to unsustainable level (e.g., 50% instead of 2%)

**Attack Flow:**
1. Attacker calls `update_config()` with `yield_bps = 50000` (500%)
2. Borrowers request loans
3. On repayment, yield payout exceeds reserve balance
4. Contract panics with `InsufficientFunds`
5. Protocol halts

**Impact:**
- Denial of service (contract paused)
- Vouchers cannot receive yield
- Borrowers cannot repay
- Reputation damage

**Likelihood:** Low (requires admin compromise)

### Attack Vector 2: Loan Disbursement Without Reserve Check

**Description:** Contract disburses loans without verifying yield reserve sufficiency.

**Preconditions:**
- Reserve balance < (loan_amount * (1 + yield_bps/10000))
- No pre-disbursement reserve check

**Attack Flow:**
1. Attacker requests large loan
2. Contract disburses without checking reserve
3. Reserve depleted
4. Future repayments fail
5. Protocol becomes insolvent

**Impact:**
- Protocol insolvency
- Vouchers lose yield
- Borrowers cannot repay

**Likelihood:** Medium (if reserve checks not implemented)

### Attack Vector 3: Coordinated Default + Slash Drain

**Description:** Attacker coordinates defaults to drain slash treasury, then exploits reserve.

**Preconditions:**
- Attacker controls multiple borrower accounts
- Attacker controls voucher accounts
- Slash treasury used to replenish yield reserve

**Attack Flow:**
1. Attacker vouches for own borrower accounts
2. Requests large loans
3. Defaults intentionally
4. Slash treasury accumulates slashed funds
5. Attacker withdraws slash treasury
6. Yield reserve depleted for future loans

**Impact:**
- Yield reserve depletion
- Protocol insolvency
- Loss of funds for legitimate vouchers

**Likelihood:** Low (requires multiple account control)

## Threat: Yield Calculation Precision Loss

### Attack Vector 4: Rounding Down Yield to Zero

**Description:** Attacker creates many small vouches to accumulate yield through rounding errors.

**Preconditions:**
- Yield calculation uses integer division
- Minimum stake < 50 stroops (current minimum)

**Attack Flow:**
1. Attacker creates 1000 vouches of 1 stroop each
2. Loan repaid with 2% yield
3. Each vouch: `1 * 200 / 10000 = 0` (rounds down)
4. Attacker receives 0 yield but protocol owes 20 stroops
5. Repeated across many loans drains reserve

**Impact:**
- Yield reserve depletion through accumulated rounding errors
- Legitimate vouchers receive no yield

**Likelihood:** Low (minimum stake enforced at 50 stroops)

## Mitigations

### Mitigation 1: Pre-Disbursement Reserve Check

**Implementation:**
```rust
fn request_loan(...) {
    // Calculate required reserve
    let required_reserve = amount + (amount * yield_bps / 10_000);
    
    // Check reserve before disbursement
    let current_reserve = get_yield_reserve();
    assert!(current_reserve >= required_reserve, "InsufficientFunds");
    
    // Disburse loan
    transfer_to_borrower(amount);
}
```

**Effectiveness:** Prevents loans when reserve insufficient

**Operational Impact:** May reject valid loans if reserve low

### Mitigation 2: Yield Rate Bounds

**Implementation:**
```rust
const MAX_YIELD_BPS: i128 = 1000; // 10% max

fn update_config(yield_bps: i128) {
    assert!(yield_bps <= MAX_YIELD_BPS, "InvalidYield");
}
```

**Effectiveness:** Prevents unsustainable yield rates

**Operational Impact:** Limits protocol flexibility

### Mitigation 3: Reserve Monitoring and Alerts

**Implementation:**
- Prometheus metric: `qc_yield_reserve_balance`
- Alert when reserve < 110% of max loan amount
- Alert when reserve < 10% of total loan volume

**Effectiveness:** Early warning of reserve depletion

**Operational Impact:** Requires active monitoring

### Mitigation 4: Minimum Stake Enforcement

**Implementation:**
```rust
const MIN_STAKE_FOR_YIELD: i128 = 50; // stroops

fn vouch(stake: i128) {
    assert!(stake >= MIN_STAKE_FOR_YIELD, "MinStakeNotMet");
}
```

**Effectiveness:** Prevents rounding errors from small stakes

**Operational Impact:** Minimum stake requirement

### Mitigation 5: Multisig Admin Control

**Implementation:**
- All config changes require `admin_threshold` signatures
- Prevents single key compromise from changing yield rate
- Requires 2-of-3 or 3-of-5 multisig

**Effectiveness:** Prevents unilateral yield manipulation

**Operational Impact:** Slower config changes

### Mitigation 6: Reserve Replenishment Procedure

**Implementation:**
- Admin-only function to transfer XLM to contract
- Requires multisig approval
- Logged and auditable

```rust
fn replenish_reserve(admin_signers: Vec<Address>, amount: i128) {
    require_admin_approval(admin_signers);
    transfer_from_admin(amount);
}
```

**Effectiveness:** Allows reserve recovery

**Operational Impact:** Requires admin action

## Operator Recommendations

### Daily Checks

```bash
# Check reserve balance
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn get_fee_treasury \
  --network mainnet

# Calculate reserve health
# reserve_health = reserve / (max_loan_amount * 1.02)
# Alert if < 1.1 (110%)
```

### Weekly Review

- Review loan volume trends
- Check yield distribution
- Verify no unusual defaults
- Audit admin actions

### Monthly Actions

- Replenish reserve if needed
- Review yield rate sustainability
- Update monitoring thresholds
- Audit slash treasury

### Reserve Sizing Formula

```
Required Reserve = (Max Concurrent Loans) × (Max Loan Amount) × (1 + Yield Rate)

Example:
- Max concurrent loans: 100
- Max loan amount: 1000 XLM
- Yield rate: 2%
- Required reserve: 100 × 1000 × 1.02 = 102,000 XLM
- Recommended buffer: 110% = 112,200 XLM
```

## Detection Strategies

### Metric-Based Detection

Monitor these metrics for anomalies:

| Metric | Normal Range | Alert Threshold |
|--------|--------------|-----------------|
| Reserve balance | > 110% required | < 110% required |
| Yield payout rate | 2% of repayments | > 5% of repayments |
| Default rate | < 5% | > 10% |
| Loan volume | Steady growth | > 50% spike |

### Transaction-Based Detection

```python
def detect_reserve_drain():
    """Detect unusual reserve depletion patterns"""
    
    # Get reserve history
    reserve_history = get_reserve_history(days=7)
    
    # Calculate daily change
    daily_changes = [
        reserve_history[i] - reserve_history[i-1]
        for i in range(1, len(reserve_history))
    ]
    
    # Alert if > 20% daily decrease
    for change in daily_changes:
        if change < -0.2 * reserve_history[0]:
            alert("Unusual reserve depletion detected")
```

## Incident Response

### If Reserve Depleted

1. **Immediate (< 5 min):**
   - Pause contract
   - Alert ops team
   - Notify stakeholders

2. **Short-term (< 1 hour):**
   - Investigate cause
   - Review recent transactions
   - Check admin logs

3. **Medium-term (< 24 hours):**
   - Replenish reserve
   - Audit all loans
   - Verify yield calculations

4. **Long-term (< 1 week):**
   - Root cause analysis
   - Update monitoring
   - Implement additional safeguards

### Communication Template

```
INCIDENT: Yield Reserve Depletion

SEVERITY: Critical
TIME: [timestamp]
DURATION: [duration]

IMPACT:
- Repayment transactions failing
- Vouchers cannot receive yield
- Protocol paused

CAUSE: [root cause]

RESOLUTION:
- Reserve replenished with [amount] XLM
- Contract unpaused at [time]

PREVENTION:
- [mitigation implemented]
```

## Testing

### Stress Test: Reserve Depletion

```rust
#[test]
fn test_reserve_depletion_protection() {
    // Setup: Create contract with 100 XLM reserve
    let reserve = 100_000_000_000i128; // 100 XLM
    
    // Attempt to request loan > reserve
    let loan_amount = 150_000_000_000i128; // 150 XLM
    
    // Should fail with InsufficientFunds
    assert_eq!(
        request_loan(borrower, loan_amount, threshold, token),
        Err(ContractError::InsufficientFunds)
    );
}
```

### Fuzz Test: Yield Calculation

```rust
#[test]
fn fuzz_yield_calculation() {
    for stake in 1..1_000_000_000 {
        let yield_amount = (stake * 200) / 10_000;
        
        // Verify yield never exceeds 2%
        assert!(yield_amount <= (stake * 2) / 100);
        
        // Verify no negative yields
        assert!(yield_amount >= 0);
    }
}
```

## References

- [Yield Accounting & Solvency](../README.md#-yield-accounting--solvency)
- [Error Reference](../README.md#error-reference)
- [Deployment Guide](./deployment-guide.md)
- [Monitoring Guide](./monitoring-guide.md)
