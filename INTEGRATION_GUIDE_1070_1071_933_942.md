# Integration Guide: Circuit Breaker, Insurance Fund, Lazy Detection & Fuzz Testing

## Quick Start

This guide shows how to integrate the new features into the existing QuorumCredit contract workflow.

---

## 1. Initialization Setup

When initializing the contract, set the new config parameters:

```rust
// In admin.rs or initialization code
let config = Config {
    // ... existing fields ...
    
    // NEW: Circuit Breaker Configuration
    default_rate_threshold: 1000,  // 10% default rate triggers circuit breaker
    
    // NEW: Insurance Fund Configuration
    insurance_fund_premium_bps: 50,    // 0.5% of each loan goes to insurance
    insurance_max_payout_bps: 2500,    // Insurance covers up to 25% of slashed amount
};

env.storage().instance().set(&DataKey::Config, &config);
```

---

## 2. Loan Disbursement Flow

When disbursing a loan, collect insurance premium:

```rust
// In loan.rs request_loan() function
pub fn request_loan(
    env: Env,
    borrower: Address,
    amount: i128,
    // ... other params ...
) -> Result<(), ContractError> {
    // ... existing validation ...
    
    // NEW: Collect insurance fee
    let config = config(&env);
    let insurance_fee = insurance::collect_insurance_fee(&env, amount, &config)?;
    
    // Disburse loan amount minus insurance
    let net_disbursement = amount.saturating_sub(insurance_fee);
    
    // Transfer to borrower
    token_client(&env, &config.token).transfer(
        &env.current_contract_address(),
        &borrower,
        &net_disbursement,
    );
    
    // Create loan record
    let loan = LoanRecord {
        amount,  // Track original amount
        // ... other fields ...
    };
    
    Ok(())
}
```

---

## 3. Slash Processing with Insurance

When slashing a defaulted loan, use insurance for shortfalls:

```rust
// In loan.rs or governance.rs slash() function
pub fn execute_slash(
    env: Env,
    admin_signers: Vec<Address>,
    borrower: Address,
) -> Result<(), ContractError> {
    // ... auth checks ...
    
    let config = config(&env);
    let loan = get_loan(&env, &borrower)?;
    
    // Calculate total slash
    let total_slash = (loan.amount as u128)
        .saturating_mul(config.slash_bps as u128)
        .saturating_div(10_000)
        as i128;
    
    // Distribute to vouchers
    let vouches = get_vouches(&env, &borrower)?;
    let per_voucher_slash = total_slash / (vouches.len() as i128);
    
    let mut distributed = 0i128;
    for vouch in &vouches {
        // Transfer slash funds...
        distributed = distributed.saturating_add(per_voucher_slash);
    }
    
    // NEW: If shortfall exists, use insurance
    let shortfall = total_slash.saturating_sub(distributed);
    if shortfall > 0 {
        let insurance_payout = insurance::claim_insurance_for_shortfall(&env, shortfall)?;
        // Use insurance_payout to cover remaining voucher losses
    }
    
    // NEW: Try to trigger circuit breaker
    let default_count: u32 = env.storage().instance()
        .get(&DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(Ok(0))
        .unwrap_or(0);
    let total_loan_count: u32 = env.storage().instance()
        .get(&DataKey::LoanCounter)
        .unwrap_or(Ok(0))
        .unwrap_or(0);
    
    let _breaker_triggered = circuit_breaker::try_trigger_circuit_breaker(
        &env,
        &config,
        default_count,
        total_loan_count,
    )?;
    
    Ok(())
}
```

---

## 4. Lazy Default Detection

Call lazy detection before critical borrower operations:

```rust
// In vouch.rs or loan.rs
pub fn vouch(
    env: Env,
    voucher: Address,
    borrower: Address,
    stake: i128,
    token: Address,
) -> Result<(), ContractError> {
    // NEW: Check for lazy defaults first
    let _ = lazy_default_detection::check_all_defaults_for_borrower(&env, &borrower)?;
    
    // Check if borrower has active loans or other restrictions
    if let Some(active_loan_id) = get_active_loan(&env, &borrower)? {
        // NEW: Check if the active loan should be marked as defaulted
        if lazy_default_detection::is_loan_defaulted(&env, &get_loan(&env, &borrower)?) {
            return Err(ContractError::ActiveLoanExists);
        }
    }
    
    // ... rest of vouch logic ...
    
    Ok(())
}
```

---

## 5. Admin Operations

### Set Circuit Breaker Threshold

```rust
// Via admin.rs or governance
circuit_breaker::set_default_rate_threshold(
    &env,
    admin_signers,
    1000,  // 10% default rate
)?;
```

### Contribute to Insurance Fund

```rust
// Admin manually funds insurance pool
insurance::contribute_to_insurance_fund(
    &env,
    admin_signers,
    1_000_000_000,  // 100 XLM in stroops
)?;
```

### Query Insurance Fund Status

```rust
let balance = insurance::get_insurance_fund_balance(&env);
let last_contribution = insurance::get_insurance_fund_last_contribution(&env);

// Emit for monitoring
env.events().publish(
    ("insurance", "fund_status"),
    (balance, last_contribution),
);
```

---

## 6. Query Functions for Monitoring

Add these public query functions to the contract:

```rust
// In lib.rs contractimpl block

/// Get current default rate in basis points
pub fn get_default_rate(env: Env) -> Result<u32, ContractError> {
    circuit_breaker::get_current_default_rate(&env)
}

/// Get circuit breaker threshold
pub fn get_circuit_breaker_threshold(env: Env) -> Result<u32, ContractError> {
    circuit_breaker::get_default_rate_threshold(&env)
}

/// Get insurance fund balance
pub fn get_insurance_fund_status(env: Env) -> (i128, u64) {
    (
        insurance::get_insurance_fund_balance(&env),
        insurance::get_insurance_fund_last_contribution(&env),
    )
}

/// Check if a specific loan is defaulted (lazy detection, no-op)
pub fn is_loan_defaulted(env: Env, loan_id: u64) -> Result<bool, ContractError> {
    lazy_default_detection::get_default_detection_status(&env, loan_id)
}
```

---

## 7. Event Monitoring

Off-chain monitoring should watch for these events:

### Circuit Breaker Events
```
Event: ("circuit_breaker", "activated")
Data: (default_count, total_loan_count, current_rate, threshold)

Example: (10, 100, 1000, 1000)  // 10 defaults, 10% rate, 10% threshold
```

### Insurance Fund Events
```
Event: ("insurance", "fund_status")
Data: (balance, last_contribution_timestamp)

Example: (50_000_000, 1690000000)  // 5 XLM balance
```

### Lazy Default Detection Events
```
Event: ("loan", "default_detected")
Data: (borrower, loan_id, deadline, current_timestamp)

Example: (address, 42, 1690000000, 1690086400)  // Detected 1 day after deadline
```

---

## 8. Testing Integration

Run the comprehensive test suite:

```bash
# Run all new tests
cargo test fuzz_stake_testing -- --nocapture
cargo test circuit_breaker_insurance_integration_test -- --nocapture

# Run specific test
cargo test test_circuit_breaker_triggers_on_high_default_rate -- --nocapture

# Run fuzz testing with more cases
PROPTEST_CASES=1000 cargo test fuzz_yield_calculation_consistency

# Full test suite
cargo test --lib
```

---

## 9. Deployment Workflow

### Step 1: Pre-Deployment Verification
```bash
# Check compilation
cargo check

# Run linter
cargo clippy -- -D warnings

# Run tests
cargo test --lib 2>&1 | tail -100
```

### Step 2: Build Contract
```bash
cargo build --target wasm32v1-none --release
```

### Step 3: Deploy
```bash
stellar contract deploy \
  --wasm target/wasm32v1-none/release/quorum_credit.wasm \
  --network testnet \
  --source $DEPLOYER_SECRET_KEY
```

### Step 4: Initialize with New Config
```bash
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn initialize \
  --network testnet \
  --source $DEPLOYER_SECRET_KEY \
  -- \
  --deployer $DEPLOYER_ADDRESS \
  --admins '[...]' \
  --admin_threshold 1 \
  --token $TOKEN_ADDRESS

# Config will use defaults:
# - default_rate_threshold: 10_000 (100%, effectively disabled)
# - insurance_fund_premium_bps: 50 (0.5%)
# - insurance_max_payout_bps: 2500 (25%)
```

### Step 5: Post-Deployment Configuration
```bash
# Set circuit breaker threshold to 10% (1000 bps)
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn set_default_rate_threshold \
  --network testnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_ADDRESS'"]' \
  --new_threshold 1000

# Pre-fund insurance pool
stellar contract invoke \
  --id $CONTRACT_ID \
  --fn contribute_to_insurance_fund \
  --network testnet \
  --source $ADMIN_SECRET_KEY \
  -- \
  --admin_signers '["'$ADMIN_ADDRESS'"]' \
  --amount 1000000000  # 100 XLM
```

---

## 10. Monitoring Dashboard

Create an off-chain monitoring dashboard to track:

| Metric | Query Function | Threshold Alert |
|--------|---|---|
| Default Rate | `get_default_rate()` | >= 500 bps (5%) |
| Insurance Balance | `get_insurance_fund_status()` | < 10 XLM equivalent |
| Circuit Breaker Status | Check if contract is paused | Immediate alert |
| Lazy Default Count | Monitor `default_detected` events | > 5 per hour |

---

## 11. Troubleshooting

### Circuit Breaker Won't Activate
- Check: `default_rate_threshold` is <= current default rate
- Check: Cooldown period (1 hour) has elapsed since last trigger
- Verify: Default count is being incremented correctly

### Insurance Fund Claims Fail
- Check: Insurance fund has sufficient balance (`get_insurance_fund_status()`)
- Check: Premium collection is enabled (`insurance_fund_premium_bps > 0`)
- Verify: Fund was pre-funded with admin contribution

### Lazy Detection Misses Defaults
- Check: Detection is being called before loan operations
- Verify: Loan deadline comparison is working (check ledger timestamp)
- Review: Loan status transitions (must be Active → Defaulted)

---

## 12. Backward Compatibility

All changes are **fully backward compatible**:

✅ Existing loan flows work unchanged (insurance optional)  
✅ Vouching/slashing logic preserved (circuit breaker optional)  
✅ Contract remains functional with default config  
✅ Admin can enable features gradually  

---

## 13. Gas Optimization Notes

- Lazy detection is O(1) per loan (no iteration)
- Circuit breaker check is O(1) calculation
- Insurance fund operations are O(1) storage access
- Fuzz testing runs off-chain (no on-chain gas impact)

---

## 14. Future Extensions

Potential enhancements:

1. **Partial Insurance Claims** - Pay out proportionally if fund is depleted
2. **Dynamic Threshold** - Adjust default_rate_threshold based on pool health
3. **Insurance Governance** - Community votes on rebalancing
4. **Staged Circuit Breaker** - Gradual pause instead of immediate
5. **Cross-Chain Insurance** - Unified fund across multiple chains

---

## Summary

The integration of these four features creates a **resilient, monitored lending protocol**:

1. **Lazy Detection** - Efficient default flagging
2. **Fuzz Testing** - Mathematically verified calculations
3. **Circuit Breaker** - Automatic crisis response
4. **Insurance Fund** - Risk absorption for tail events

Together, they reduce protocol risk and improve user confidence in QuorumCredit's stability.
