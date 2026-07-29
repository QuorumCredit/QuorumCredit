# Technical Architecture & Specifications

## System Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        QuorumCredit Smart Contract                          │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │ Loan Request Flow                                                  │   │
│  ├────────────────────────────────────────────────────────────────────┤   │
│  │                                                                    │   │
│  │  request_loan(borrower, amount, ...)                             │   │
│  │      ↓                                                            │   │
│  │  [Lazy Detection] check_all_defaults_for_borrower()             │   │
│  │      ↓ (marks overdue loans as defaulted)                        │   │
│  │  [Validate] Threshold met? Active loan exists?                  │   │
│  │      ↓                                                            │   │
│  │  [Insurance] collect_insurance_fee()                            │   │
│  │      ↓ (route insurance_fund_premium_bps% to insurance fund)    │   │
│  │  [Disburse] Transfer net amount to borrower                     │   │
│  │      ↓                                                            │   │
│  │  [Circuit Breaker] try_trigger_circuit_breaker()                │   │
│  │      ↓ (check default_rate >= threshold)                        │   │
│  │  [Store] Create LoanRecord                                       │   │
│  │                                                                   │   │
│  └────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │ Repay Flow                                                         │   │
│  ├────────────────────────────────────────────────────────────────────┤   │
│  │                                                                    │   │
│  │  repay(borrower, payment)                                        │   │
│  │      ↓                                                            │   │
│  │  [Validate] Active loan? Amount valid?                          │   │
│  │      ↓                                                            │   │
│  │  [Calculate] principal_repaid, yield_earned                     │   │
│  │      ↓                                                            │   │
│  │  [Transfer] Send repayment to contract                          │   │
│  │      ↓                                                            │   │
│  │  [Distribute] Yield to vouchers                                  │   │
│  │      ↓                                                            │   │
│  │  [Verify] Check if fully repaid                                 │   │
│  │      ↓                                                            │   │
│  │  [Finalize] Mark loan as Repaid                                 │   │
│  │                                                                   │   │
│  └────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │ Slash Flow (NEW: with Insurance & Circuit Breaker)               │   │
│  ├────────────────────────────────────────────────────────────────────┤   │
│  │                                                                    │   │
│  │  execute_slash(borrower)                                         │   │
│  │      ↓                                                            │   │
│  │  [Lazy Detection] check_and_mark_default(loan_id)              │   │
│  │      ↓ (marks as Defaulted, increments default_count)          │   │
│  │  [Auth] Admin approval required                                 │   │
│  │      ↓                                                            │   │
│  │  [Calculate] total_slash = loan_amount * slash_bps / 10_000    │   │
│  │      ↓                                                            │   │
│  │  [Distribute] Pay vouchers their slash amount                  │   │
│  │      ↓                                                            │   │
│  │  [Insurance] IF shortfall exists:                              │   │
│  │    |  shortfall = total_slash - distributed                    │   │
│  │    |  payout = claim_insurance_for_shortfall(shortfall)        │   │
│  │    |  cover remaining with insurance_payout                    │   │
│  │      ↓                                                            │   │
│  │  [Circuit Breaker] try_trigger_circuit_breaker()                │   │
│  │    |  Calculate: rate = (default_count / total) * 10_000       │   │
│  │    |  IF rate >= threshold AND cooldown elapsed:               │   │
│  │    |    auto_pause_contract()                                   │   │
│  │    |    emit event: ("circuit_breaker", "activated")           │   │
│  │      ↓                                                            │   │
│  │  [Finalize] Mark loan as Defaulted                             │   │
│  │                                                                   │   │
│  └────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
│  ┌────────────────────────────────────────────────────────────────────┐   │
│  │ Storage State (Persistent)                                        │   │
│  ├────────────────────────────────────────────────────────────────────┤   │
│  │                                                                    │   │
│  │  Config → {                                                      │   │
│  │    default_rate_threshold: u32,      [NEW] Circuit breaker       │   │
│  │    insurance_fund_premium_bps: u32,  [NEW] Insurance collection │   │
│  │    insurance_max_payout_bps: u32,    [NEW] Max insurance payout │   │
│  │    ... (other config fields)                                     │   │
│  │  }                                                               │   │
│  │                                                                   │
│  │  LoanRecord(loan_id) → {                                        │   │
│  │    borrower, amount, amount_repaid, status, deadline, ...      │   │
│  │  }                                                               │   │
│  │                                                                   │   │
│  │  DefaultCount(borrower) → u32    [NEW] Track default history   │   │
│  │  LoanCounter → u64                [EXISTING] Total loans       │   │
│  │                                                                   │   │
│  │  InsuranceFund → i128             [NEW] Fund balance            │   │
│  │  InsuranceFundLastContribution → u64  [NEW] Last deposit time  │   │
│  │                                                                   │   │
│  │  CircuitBreakerLastTriggered → u64    [NEW] Last activation    │   │
│  │                                                                   │   │
│  └────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Module Responsibilities

### `circuit_breaker.rs`

**Purpose**: Auto-pause protocol on default cascade

**Key Functions**:
- `calculate_default_rate()` - (defaults / total) * 10_000 in bps
- `should_trigger_circuit_breaker()` - Rate >= threshold?
- `try_trigger_circuit_breaker()` - Activate with cooldown check
- `set_default_rate_threshold()` - Governance update

**State**:
- `CircuitBreakerLastTriggered: u64` - Cooldown tracking
- `DefaultRateThreshold: u32` - Configurable in Config

**Events**:
- `circuit_breaker:activated` (default_count, total, rate, threshold)

**Gas Cost**: O(1) read + write

---

### `lazy_default_detection.rs`

**Purpose**: On-demand default flagging

**Key Functions**:
- `is_loan_defaulted()` - Check if past deadline + unpaid
- `check_and_mark_default()` - Lazy mark + increment counter
- `get_default_detection_status()` - Query without mutation

**State**:
- Reuses existing `LoanRecord` and `DefaultCount`
- No new storage keys required

**Events**:
- `loan:default_detected` (borrower, loan_id, deadline, now)

**Gas Cost**: O(1) per loan

---

### `insurance.rs`

**Purpose**: Risk absorption via pre-funded insurance pool

**Key Functions**:
- `collect_insurance_fee()` - Route loan_amount * premium_bps% to fund
- `contribute_to_insurance_fund()` - Admin pre-funding
- `claim_insurance_for_shortfall()` - Cover slash gaps
- `get_insurance_fund_balance()` - Query balance

**State**:
- `InsuranceFund: i128` - Fund balance
- `InsuranceFundLastContribution: u64` - Last deposit time
- `insurance_fund_premium_bps: u32` - In Config
- `insurance_max_payout_bps: u32` - In Config

**Error Paths**:
- `InsurancePoolEmpty` (44) - When fund depleted

**Gas Cost**: O(1) per operation

---

### `fuzz_stake_testing.rs`

**Purpose**: Property-based testing of arithmetic

**Test Categories**:
1. Overflow prevention (saturating arithmetic)
2. Yield calculation consistency
3. Slash calculation consistency
4. Stake accumulation precision
5. Basis points validation
6. Fee distribution precision
7. Extreme value stress tests

**No Runtime Impact** - Tests only, compile-time excluded

---

## Arithmetic Specifications

### Default Rate Calculation

```
DEFAULT_RATE_BPS = (default_count / total_loan_count) * 10_000

Range: [0, 10_000] basis points (0% - 100%)

Precision: Integer division (truncates)
  Example: 1 default, 3 total → (1 / 3) * 10_000 = 3333 bps

Edge Cases:
  - 0 defaults, N total → 0 bps (no defaults yet)
  - N defaults, N total → 10_000 bps (all defaulted)
  - 0 defaults, 0 total → 0 bps (no loans yet)
```

### Insurance Premium Calculation

```
INSURANCE_FEE = (loan_amount * insurance_fund_premium_bps) / 10_000

Range: [0, loan_amount] stroops

Precision: Integer division (truncates)
  Example: 100 XLM loan, 50 bps → (10^9 * 50) / 10_000 = 5 * 10^6 stroops (0.5 XLM)

Edge Cases:
  - 0 bps → 0 collected (no insurance)
  - 1 stroop loan, 50 bps → 0 collected (truncates to 0)
  - 10_000 XLM loan, 100 bps → 10_000_000 stroops (1 XLM)
```

### Slash Calculation

```
SLASH_AMOUNT = (stake * slash_bps) / 10_000

Range: [0, stake] stroops

Precision: Integer division (truncates)
  Example: 100 XLM stake, 5000 bps (50% slash) → (10^9 * 5000) / 10_000 = 5 * 10^8 stroops (50 XLM)

Edge Cases:
  - 0 bps → 0 slashed (no penalty)
  - 10_000 bps → entire stake slashed
  - 1 stroop stake, 50 bps → 0 slashed (truncates)
```

### Yield Calculation

```
YIELD = (principal * yield_bps) / 10_000

Minimum stake for non-zero yield: 50 stroops (at 200 bps = 2%)
  50 * 200 / 10_000 = 1 stroop

Example: 10 XLM stake, 200 bps (2%) → (10^7 * 200) / 10_000 = 2 * 10^5 stroops (0.02 XLM)

Invariant: YIELD <= PRINCIPAL (always true due to bps ≤ 10_000)
```

---

## State Transition Diagrams

### Loan Status with Lazy Detection

```
              request_loan()
                   ↓
    ┌─────────────────────────────────────┐
    │         Loan Created (Active)       │
    └─────────────────────────────────────┘
           ↓ repay()           ↓ time > deadline + no repayment
           │                   │
      [Repaid]          [Lazy Detection]
           │                   │
           └───→ [Defaulted] ←─┘
                   ↓
              [Slash Applied]
```

### Default Count Lifecycle

```
Loan Created: default_count(borrower) = N

Loan Repaid: default_count(borrower) = N (unchanged)

Lazy Detection Triggers: default_count(borrower) = N + 1

Slash Executed: default_count(borrower) = N + 1 (unchanged, already marked)

Circuit Breaker Calculation: rate = (default_count / total) * 10_000

Auto-Pause: IF rate >= threshold THEN contract.paused = true
```

### Insurance Fund Lifecycle

```
Initial State: InsuranceFund = 0

Admin Contributes: InsuranceFund += amount

Loan Disbursed: InsuranceFund += (amount * premium_bps / 10_000)

Slash Without Shortfall: InsuranceFund unchanged

Slash With Shortfall: 
    shortfall = total_slash - available
    claimed = min(shortfall, InsuranceFund)
    InsuranceFund -= claimed
```

---

## Computational Complexity

| Operation | Time | Space | Notes |
|---|---|---|---|
| `calculate_default_rate()` | O(1) | O(1) | Simple division |
| `should_trigger_circuit_breaker()` | O(1) | O(1) | Comparison |
| `try_trigger_circuit_breaker()` | O(1) | O(1) | Storage + event |
| `is_loan_defaulted()` | O(1) | O(1) | Timestamp check |
| `check_and_mark_default()` | O(1) | O(1) | Storage write |
| `collect_insurance_fee()` | O(1) | O(1) | Arithmetic + storage |
| `claim_insurance_for_shortfall()` | O(1) | O(1) | Storage read/write |

**Total Protocol Overhead Per Loan**: ~4 additional O(1) operations (negligible gas impact)

---

## Error Handling Matrix

| Scenario | Error | Recovery |
|---|---|---|
| Insurance fund depleted during slash | `InsurancePoolEmpty` | Admin must contribute to fund |
| Circuit breaker threshold exceeded | Auto-pause activated | Admin must manually unpause |
| Lazy detection on zero deadline | No error (not defaulted) | N/A |
| Invalid basis points (>10000) | `InvalidBps` | Admin must use valid range [0-10000] |
| Negative amounts | `InvalidAmount` | User must use positive amounts |

---

## Security Considerations

### 1. Reentrancy
- All operations use storage-only state (no callbacks)
- No external contract calls in new code
- Safe against reentrancy

### 2. Arithmetic Safety
- Uses `saturating_*` for all calculations
- No panics on overflow/underflow
- Truncates fractional stroops (acceptable for token precision)

### 3. Authorization
- `set_default_rate_threshold()` requires admin signatures
- `contribute_to_insurance_fund()` requires admin signatures
- Lazy detection is publicly callable (read-only)

### 4. State Consistency
- All operations are idempotent (same result if re-executed)
- No partial state updates (atomic operations)
- Default count only increments once per loan

---

## Upgrade Path

### From Previous Version

1. **New DataKeys** - Safe to add (don't affect existing storage)
2. **New Config Fields** - Have sensible defaults
3. **New Modules** - Opt-in (called only when enabled)
4. **Backward Compatible** - Existing loans continue unaffected

### Migration Steps

```bash
# Step 1: Deploy new contract with defaults
# Circuit breaker disabled: default_rate_threshold = 10_000 (effectively 100%)
# Insurance fund inactive: insurance_fund_premium_bps = 0

# Step 2: Admin enables features
# - Set default_rate_threshold = 1000 (10%)
# - Set insurance_fund_premium_bps = 50 (0.5%)
# - Pre-fund insurance pool

# Step 3: Monitor and adjust
# - Watch circuit_breaker:activated events
# - Track insurance fund balance
# - Adjust thresholds based on protocol health
```

---

## Monitoring & Observability

### Key Metrics

1. **Default Rate** - `current_defaults / total_loans`
2. **Insurance Fund Health** - Balance vs. claims
3. **Circuit Breaker Status** - Activated/inactive, cooldown remaining
4. **Lazy Detection Frequency** - Defaults flagged per hour

### Recommended Dashboards

- Real-time default rate percentage
- Insurance fund balance (XLM equivalent)
- Circuit breaker activation timeline
- Slash event frequency vs. insurance payouts

### Alerting Rules

- Alert if default_rate > 5% (0.5x threshold)
- Alert if insurance_fund < 1 XLM
- Alert if circuit_breaker:activated event emitted
- Alert if slash_shortfall > insurance_fund

---

## Performance Baseline (Expected)

| Benchmark | Gas Cost | Notes |
|---|---|---|
| `request_loan()` with insurance | +1,000-2,000 | ~0.1% increase |
| `execute_slash()` with circuit breaker | +2,000-3,000 | ~0.2% increase |
| `collect_insurance_fee()` | 500-800 | Negligible |
| Default rate calculation | 300-500 | Negligible |

**Total Protocol Overhead**: <1% additional gas per operation

---

## Conclusion

This architecture provides:
- **Safety** - Saturating arithmetic, idempotent operations
- **Efficiency** - O(1) operations, negligible gas overhead
- **Transparency** - Comprehensive event emission
- **Flexibility** - Configurable thresholds, opt-in insurance
- **Resilience** - Multi-layer defaults defense (lazy detection + circuit breaker + insurance)
