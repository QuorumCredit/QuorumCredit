# On-Chain Credit Score with Tiered Rewards

## Overview

The On-Chain Credit Score system provides a comprehensive, transparent, and algorithmic credit scoring mechanism for borrowers in the QuorumCredit protocol. Credit scores are calculated based on on-chain activity and determine tiered rewards that provide better loan terms to high-credit borrowers.

## Credit Score Calculation

### Score Range

Credit scores range from **0 to 1000**, where:
- **0-349**: Poor
- **350-549**: Fair
- **550-699**: Good
- **700-849**: Very Good
- **850-1000**: Excellent

### Calculation Factors

The credit score is calculated using a weighted average of five components:

1. **Repayment History (40%)**
   - Ratio of successfully repaid loans to total loans
   - Each default reduces the score by 200 points
   - New users start with a neutral score of 500

2. **Loan Count (15%)**
   - Total number of loans taken
   - More loans with good history = higher score
   - Max benefit at 10 loans

3. **Account Age (10%)**
   - Time since account registration
   - Max benefit at 1 year of account age
   - Encourages long-term participation

4. **Vouching Activity (15%)**
   - Number of times the user has vouched for others
   - Encourages community participation
   - Max benefit at 20 vouches

5. **Repayment Timeliness (20%)**
   - Average time of repayment relative to deadline
   - Early repayment increases score
   - Late repayment decreases score
   - Max benefit for 7 days early, max penalty for 7 days late

### Default Weights

```rust
repayment_history_weight: 4000,  // 40%
loan_count_weight: 1500,         // 15%
account_age_weight: 1000,         // 10%
vouching_weight: 1500,            // 15%
timeliness_weight: 2000,          // 20%
```

## Tiered Rewards

Each credit tier provides specific benefits:

### Poor Tier (0-349)
- **Yield Bonus**: 0 bps
- **Max Loan Multiplier**: 100% (1x)
- **Min Stake Reduction**: 0%
- **Duration Extension**: 0 days
- **Fee Discount**: 0%

### Fair Tier (350-549)
- **Yield Bonus**: 50 bps (0.5%)
- **Max Loan Multiplier**: 110% (1.1x)
- **Min Stake Reduction**: 5%
- **Duration Extension**: 1 day
- **Fee Discount**: 1%

### Good Tier (550-699)
- **Yield Bonus**: 100 bps (1%)
- **Max Loan Multiplier**: 125% (1.25x)
- **Min Stake Reduction**: 10%
- **Duration Extension**: 2 days
- **Fee Discount**: 2.5%

### Very Good Tier (700-849)
- **Yield Bonus**: 150 bps (1.5%)
- **Max Loan Multiplier**: 150% (1.5x)
- **Min Stake Reduction**: 15%
- **Duration Extension**: 4 days
- **Fee Discount**: 5%

### Excellent Tier (850-1000)
- **Yield Bonus**: 200 bps (2%)
- **Max Loan Multiplier**: 200% (2x)
- **Min Stake Reduction**: 20%
- **Duration Extension**: 7 days
- **Fee Discount**: 10%

## Data Structures

### CreditScore

```rust
pub struct CreditScore {
    pub score: u32,                    // Overall score (0-1000)
    pub tier: CreditTier,              // Current credit tier
    pub last_updated: u64,             // Last update timestamp
    pub total_loans: u32,             // Total loans taken
    pub successful_repayments: u32,    // Successfully repaid loans
    pub defaults: u32,                 // Number of defaults
    pub total_borrowed: i128,         // Total amount borrowed
    pub total_repaid: i128,           // Total amount repaid
    pub account_age: u64,              // Account age in seconds
    pub voucher_count: u32,            // Times as a voucher
    pub avg_repayment_time: i64,      // Avg repayment time (negative if late)
}
```

### CreditTier

```rust
pub enum CreditTier {
    Poor,       // 0-349
    Fair,       // 350-549
    Good,       // 550-699
    VeryGood,   // 700-849
    Excellent,  // 850-1000
}
```

### TierRewards

```rust
pub struct TierRewards {
    pub yield_bonus_bps: i32,          // Yield bonus in basis points
    pub max_loan_multiplier: u32,     // Max loan multiplier (100 = 1x)
    pub min_stake_reduction_bps: u32, // Min stake reduction in basis points
    pub duration_extension: u64,       // Duration extension in seconds
    pub fee_discount_bps: u32,        // Fee discount in basis points
}
```

### CreditScoreConfig

```rust
pub struct CreditScoreConfig {
    pub enabled: bool,                // Whether credit scoring is enabled
    pub factors: CreditFactors,       // Calculation factor weights
    pub poor_rewards: TierRewards,    // Rewards for Poor tier
    pub fair_rewards: TierRewards,    // Rewards for Fair tier
    pub good_rewards: TierRewards,    // Rewards for Good tier
    pub very_good_rewards: TierRewards, // Rewards for Very Good tier
    pub excellent_rewards: TierRewards, // Rewards for Excellent tier
}
```

## Contract Functions

### Credit Score Management

#### `update_credit_score`

Calculate and update the credit score for a borrower.

```rust
pub fn update_credit_score(
    env: Env,
    borrower: Address,
) -> Result<(), ContractError>
```

**Behavior**: Calculates the credit score based on the borrower's on-chain history and stores it.

#### `get_credit_score`

Get the credit score for a borrower.

```rust
pub fn get_credit_score(
    env: Env,
    borrower: Address,
) -> Option<CreditScore>
```

**Returns**: The credit score record, or None if not yet calculated.

### Configuration

#### `set_credit_score_config`

Set the credit score configuration parameters.

```rust
pub fn set_credit_score_config(
    env: Env,
    admin_signers: Vec<Address>,
    config: CreditScoreConfig,
) -> Result<(), ContractError>
```

**Requirements**: Admin approval (multi-sig)

**Validation**: Factor weights must sum to 10000 (100%).

#### `get_credit_score_config_view`

Get the current credit score configuration.

```rust
pub fn get_credit_score_config_view(env: Env) -> CreditScoreConfig
```

### Tier Rewards

#### `get_tier_rewards`

Get the tier rewards for a specific credit tier.

```rust
pub fn get_tier_rewards(
    env: Env,
    tier: CreditTier,
) -> TierRewards
```

## Reward Application

The credit score system automatically applies tiered rewards to various loan calculations:

### Yield Calculation

```rust
let adjusted_yield = apply_tier_rewards_to_yield(
    &env,
    &borrower,
    base_yield_bps,
);
```

Higher tiers receive yield bonuses as additional basis points.

### Max Loan Amount

```rust
let adjusted_max = apply_tier_rewards_to_max_loan(
    &env,
    &borrower,
    base_max_loan,
);
```

Higher tiers can borrow larger amounts (multiplier applied).

### Minimum Stake

```rust
let adjusted_min = apply_tier_rewards_to_min_stake(
    &env,
    &borrower,
    base_min_stake,
);
```

Higher tiers require less minimum stake (reduction applied).

### Loan Duration

```rust
let adjusted_duration = apply_tier_rewards_to_duration(
    &env,
    &borrower,
    base_duration,
);
```

Higher tiers receive longer loan durations (extension applied).

### Protocol Fee

```rust
let adjusted_fee = apply_tier_rewards_to_fee(
    &env,
    &borrower,
    base_fee_bps,
);
```

Higher tiers receive fee discounts.

## Workflow Example

### 1. Initial Credit Score

```rust
// New user requests first loan
client.request_loan(&borrower, &amount, &threshold, &purpose, &token)?;

// Calculate initial credit score
client.update_credit_score(&borrower)?;

// Check credit score
let credit_score = client.get_credit_score(borrower).unwrap();
println!("Initial score: {}", credit_score.score); // ~500 (neutral)
```

### 2. Building Credit

```rust
// Borrower successfully repays loan
client.repay(&borrower, &loan_id, &amount, &token)?;

// Update credit score
client.update_credit_score(&borrower)?;

// Score should improve
let credit_score = client.get_credit_score(borrower).unwrap();
println!("New score: {}", credit_score.score); // >500
```

### 3. Reaching Higher Tiers

```rust
// After multiple successful loans
for i in 0..5 {
    client.request_loan(&borrower, &amount, &threshold, &purpose, &token)?;
    // ... loan lifecycle ...
    client.repay(&borrower, &loan_id, &amount, &token)?;
    client.update_credit_score(&borrower)?;
}

// Check tier
let credit_score = client.get_credit_score(borrower).unwrap();
println!("Tier: {:?}", credit_score.tier); // Good or higher
```

### 4. Applying Tier Rewards

```rust
// Get tier rewards
let rewards = client.get_tier_rewards(credit_score.tier);

// Apply to loan calculations
let max_loan = base_max_loan * rewards.max_loan_multiplier / 100;
let min_stake = base_min_stake * (10000 - rewards.min_stake_reduction_bps) / 10000;
```

## Testing

Comprehensive tests are available in `src/credit_score_test.rs`:

- `test_update_credit_score_new_user` - Test initial credit score calculation
- `test_get_credit_score_not_found` - Test non-existent credit score
- `test_set_credit_score_config` - Test configuration update
- `test_set_credit_score_config_invalid_weights` - Test weight validation
- `test_get_tier_rewards` - Test tier reward retrieval
- `test_credit_tier_calculation` - Test tier boundary calculations
- `test_repayment_history_score` - Test repayment history component
- `test_loan_count_score` - Test loan count component
- `test_account_age_score` - Test account age component
- `test_vouching_score` - Test vouching activity component
- `test_timeliness_score` - Test repayment timeliness component
- `test_apply_tier_rewards_to_yield` - Test yield reward application
- `test_apply_tier_rewards_to_max_loan` - Test max loan reward application
- `test_apply_tier_rewards_to_min_stake` - Test min stake reward application
- `test_apply_tier_rewards_to_duration` - Test duration reward application
- `test_apply_tier_rewards_to_fee` - Test fee reward application
- `test_default_credit_score_config` - Test default configuration
- `test_tier_rewards_progression` - Test reward progression across tiers

Run tests with:
```bash
cargo test credit_score_test
```

## Event Topics

The following events are published by the credit score system:

- `("credit", "update")` - Credit score updated (borrower, score, tier)
- `("credit", "config")` - Credit score configuration updated

## Error Codes

- `CreditScoreCalculationFailed` - Credit score calculation failed
- `InvalidCreditTier` - Invalid credit tier
- `CreditScoreNotFound` - Credit score not found for borrower
- `InvalidCreditConfig` - Credit score configuration is invalid

## Best Practices

1. **Regular Updates**: Update credit scores after significant events (loan repayment, default, etc.)

2. **Configuration**: Adjust factor weights based on protocol needs and risk appetite

3. **Tier Alignment**: Ensure tier rewards align with protocol economics and risk management

4. **Monitoring**: Monitor credit score distribution and tier progression to ensure system health

5. **Transparency**: Make credit score calculation logic clear to users

## Integration with Existing Systems

The credit score system integrates with:

- **Loan System**: Credit scores influence loan terms and eligibility
- **Reputation System**: Complements the existing reputation NFT system
- **Vouching System**: Vouching activity contributes to credit score
- **Admin Functions**: Credit score configuration requires admin approval

## Future Enhancements

Potential future improvements:

- Dynamic factor weights based on market conditions
- Time-decay for credit scores to encourage recent activity
- Cross-chain credit score aggregation
- Social proof integration
- Machine learning-based score refinement
- Credit score delegation/transfer
- Credit score insurance

## Migration Path

### Phase 1: Deployment
- Deploy updated contract with credit score system
- Credit scoring disabled by default or enabled with conservative settings
- Existing users start with neutral scores

### Phase 2: Gradual Rollout
- Enable credit scoring for new users
- Gradually enable tiered rewards
- Monitor score distribution and tier progression

### Phase 3: Full Adoption
- Enable credit scoring for all users
- Full tiered rewards active
- Consider adjusting factors based on data

## Security Considerations

1. **Admin Control**: Credit score configuration requires multi-sig admin approval

2. **Weight Validation**: Factor weights must sum to 100% to prevent manipulation

3. **Score Bounds**: Credit scores are bounded between 0 and 1000

4. **No External Data**: All calculations use on-chain data only

5. **Transparent Logic**: Calculation logic is deterministic and verifiable

## Performance Considerations

- Credit score calculation is O(1) complexity
- Storage cost per borrower: ~200 bytes
- Calculation triggered on-demand, not automatic
- Can be cached if needed for high-frequency access

## Example Use Cases

### Use Case 1: New Borrower

A new borrower with no history receives a neutral score of 500 (Fair tier). They receive modest benefits to encourage participation.

### Use Case 2: Reliable Borrower

A borrower with 10 successful loans and no defaults reaches Excellent tier. They receive maximum benefits including 2x max loan amount and 20% stake reduction.

### Use Case 3: Community Contributor

A borrower who actively vouches for others but has fewer loans can still achieve a good tier through the vouching activity component.

### Use Case 4: Risk Management

A borrower with multiple defaults drops to Poor tier, receiving no benefits and requiring higher stake, protecting the protocol from risky borrowers.
