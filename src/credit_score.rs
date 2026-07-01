use crate::errors::ContractError;
use crate::helpers::config;
use crate::types::{
    CreditFactors, CreditScore, CreditScoreConfig, CreditTier, DataKey, TierRewards,
    DEFAULT_CREDIT_SCORE_CONFIG,
};
use soroban_sdk::{panic_with_error, symbol_short, Address, Env, Vec};

/// Get the credit score configuration, or default if not set.
pub fn get_credit_score_config(env: &Env) -> CreditScoreConfig {
    env.storage()
        .instance()
        .get(&DataKey::CreditScoreConfig)
        .unwrap_or(DEFAULT_CREDIT_SCORE_CONFIG)
}

/// Calculate the credit tier from a score (0-1000).
pub fn calculate_tier(score: u32) -> CreditTier {
    if score < 350 {
        CreditTier::Poor
    } else if score < 550 {
        CreditTier::Fair
    } else if score < 700 {
        CreditTier::Good
    } else if score < 850 {
        CreditTier::VeryGood
    } else {
        CreditTier::Excellent
    }
}

/// Calculate repayment history score component (0-1000).
pub fn calculate_repayment_history_score(
    successful_repayments: u32,
    total_loans: u32,
    defaults: u32,
) -> u32 {
    if total_loans == 0 {
        return 500; // Neutral score for new users
    }

    let success_rate = successful_repayments as f64 / total_loans as f64;
    let default_penalty = defaults as f64 * 200.0; // Each default costs 200 points
    let base_score = success_rate * 1000.0;
    let adjusted_score = base_score - default_penalty;
    adjusted_score.max(0.0).min(1000.0) as u32
}

/// Calculate loan count score component (0-1000).
pub fn calculate_loan_count_score(total_loans: u32) -> u32 {
    // More loans with good history = higher score (up to 10 loans)
    let max_loans = 10;
    let score = (total_loans as f64 / max_loans as f64) * 1000.0;
    score.min(1000.0) as u32
}

/// Calculate account age score component (0-1000).
pub fn calculate_account_age_score(account_age: u64) -> u32 {
    // Account age in seconds, max benefit at 1 year
    let max_age = 365 * 24 * 60 * 60; // 1 year
    let score = (account_age as f64 / max_age as f64) * 1000.0;
    score.min(1000.0) as u32
}

/// Calculate vouching activity score component (0-1000).
pub fn calculate_vouching_score(voucher_count: u32) -> u32 {
    // More vouching = higher score (up to 20 vouches)
    let max_vouches = 20;
    let score = (voucher_count as f64 / max_vouches as f64) * 1000.0;
    score.min(1000.0) as u32
}

/// Calculate repayment timeliness score component (0-1000).
pub fn calculate_timeliness_score(avg_repayment_time: i64) -> u32 {
    // Positive = early repayment, Negative = late repayment
    // Max benefit for 7 days early, max penalty for 7 days late
    let early_threshold = 7 * 24 * 60 * 60; // 7 days
    let late_threshold = -7 * 24 * 60 * 60; // 7 days late

    if avg_repayment_time >= early_threshold as i64 {
        return 1000;
    } else if avg_repayment_time <= late_threshold {
        return 0;
    } else {
        // Linear interpolation
        let range = early_threshold - (-late_threshold);
        let position = avg_repayment_time - late_threshold as i64;
        let score = (position as f64 / range as f64) * 1000.0;
        score.max(0.0).min(1000.0) as u32
    }
}

/// Calculate overall credit score based on factors.
pub fn calculate_credit_score(
    env: &Env,
    borrower: &Address,
) -> Result<CreditScore, ContractError> {
    let config = get_credit_score_config(env);
    if !config.enabled {
        return Err(ContractError::CreditScoreCalculationFailed);
    }

    let factors = config.factors;

    // Get borrower statistics
    let total_loans: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCount(borrower.clone()))
        .unwrap_or(0);

    let successful_repayments: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::RepaymentCount(borrower.clone()))
        .unwrap_or(0);

    let defaults: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(0);

    let account_age: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::BorrowerRegistered(borrower.clone()))
        .unwrap_or(env.ledger().timestamp());

    let account_age = env.ledger().timestamp() - account_age;

    let voucher_count: u32 = env
        .storage()
        .instance()
        .get::<DataKey, soroban_sdk::Vec<crate::types::VouchHistoryEntry>>(&DataKey::VoucherHistory(borrower.clone()))
        .map(|history| history.len())
        .unwrap_or(0);

    // Calculate component scores
    let repayment_history_score =
        calculate_repayment_history_score(successful_repayments, total_loans, defaults);
    let loan_count_score = calculate_loan_count_score(total_loans);
    let account_age_score = calculate_account_age_score(account_age);
    let vouching_score = calculate_vouching_score(voucher_count);
    let timeliness_score = calculate_timeliness_score(0); // Default to neutral

    // Weighted average
    let weighted_score = (repayment_history_score as u64 * factors.repayment_history_weight as u64
        + loan_count_score as u64 * factors.loan_count_weight as u64
        + account_age_score as u64 * factors.account_age_weight as u64
        + vouching_score as u64 * factors.vouching_weight as u64
        + timeliness_score as u64 * factors.timeliness_weight as u64)
        / 10000;

    let score = weighted_score as u32;
    let tier = calculate_tier(score);

    let credit_score = CreditScore {
        score,
        tier,
        last_updated: env.ledger().timestamp(),
        total_loans,
        successful_repayments,
        defaults,
        total_borrowed: 0, // Would need to track this
        total_repaid: 0,    // Would need to track this
        account_age,
        voucher_count,
        avg_repayment_time: 0, // Would need to track this
    };

    Ok(credit_score)
}

/// Update credit score for a borrower.
pub fn update_credit_score(env: Env, borrower: Address) -> Result<(), ContractError> {
    let credit_score = calculate_credit_score(&env, &borrower)?;
    env.storage()
        .persistent()
        .set(&DataKey::CreditScore(borrower.clone()), &credit_score);

    env.events().publish(
        (symbol_short!("credit"), symbol_short!("update")),
        (borrower, credit_score.score, credit_score.tier),
    );

    Ok(())
}

/// Get credit score for a borrower.
pub fn get_credit_score(env: Env, borrower: Address) -> Option<CreditScore> {
    env.storage()
        .persistent()
        .get(&DataKey::CreditScore(borrower))
}

/// Get tier rewards for a given credit tier.
pub fn get_tier_rewards(env: Env, tier: CreditTier) -> TierRewards {
    let config = get_credit_score_config(&env);
    match tier {
        CreditTier::Poor => config.poor_rewards,
        CreditTier::Fair => config.fair_rewards,
        CreditTier::Good => config.good_rewards,
        CreditTier::VeryGood => config.very_good_rewards,
        CreditTier::Excellent => config.excellent_rewards,
    }
}

/// Set credit score configuration.
pub fn set_credit_score_config(
    env: Env,
    admin_signers: Vec<Address>,
    config: CreditScoreConfig,
) -> Result<(), ContractError> {
    crate::helpers::require_admin_approval(&env, &admin_signers);

    // Validate configuration
    let total_weight = config.factors.repayment_history_weight
        + config.factors.loan_count_weight
        + config.factors.account_age_weight
        + config.factors.vouching_weight
        + config.factors.timeliness_weight;

    if total_weight != 10000 {
        return Err(ContractError::InvalidCreditConfig);
    }

    env.storage()
        .instance()
        .set(&DataKey::CreditScoreConfig, &config);

    env.events().publish(
        (symbol_short!("credit"), symbol_short!("config")),
        admin_signers.get(0),
    );

    Ok(())
}

/// Get credit score configuration.
pub fn get_credit_score_config_view(env: Env) -> CreditScoreConfig {
    get_credit_score_config(&env)
}

/// Apply tier rewards to yield calculation.
pub fn apply_tier_rewards_to_yield(
    env: &Env,
    borrower: &Address,
    base_yield_bps: i128,
) -> i128 {
    let credit_score = match get_credit_score(env.clone(), borrower.clone()) {
        Some(score) => score,
        None => return base_yield_bps,
    };

    let rewards = get_tier_rewards(env.clone(), credit_score.tier);
    base_yield_bps + rewards.yield_bonus_bps as i128
}

/// Apply tier rewards to max loan amount.
pub fn apply_tier_rewards_to_max_loan(
    env: &Env,
    borrower: &Address,
    base_max_loan: i128,
) -> i128 {
    let credit_score = match get_credit_score(env.clone(), borrower.clone()) {
        Some(score) => score,
        None => return base_max_loan,
    };

    let rewards = get_tier_rewards(env.clone(), credit_score.tier);
    base_max_loan * rewards.max_loan_multiplier as i128 / 100
}

/// Apply tier rewards to minimum stake.
pub fn apply_tier_rewards_to_min_stake(
    env: &Env,
    borrower: &Address,
    base_min_stake: i128,
) -> i128 {
    let credit_score = match get_credit_score(env.clone(), borrower.clone()) {
        Some(score) => score,
        None => return base_min_stake,
    };

    let rewards = get_tier_rewards(env.clone(), credit_score.tier);
    let reduction = base_min_stake * rewards.min_stake_reduction_bps as i128 / 10000;
    base_min_stake - reduction.max(0)
}

/// Apply tier rewards to loan duration.
pub fn apply_tier_rewards_to_duration(
    env: &Env,
    borrower: &Address,
    base_duration: u64,
) -> u64 {
    let credit_score = match get_credit_score(env.clone(), borrower.clone()) {
        Some(score) => score,
        None => return base_duration,
    };

    let rewards = get_tier_rewards(env.clone(), credit_score.tier);
    base_duration + rewards.duration_extension
}

/// Apply tier rewards to protocol fee.
pub fn apply_tier_rewards_to_fee(
    env: &Env,
    borrower: &Address,
    base_fee_bps: u32,
) -> u32 {
    let credit_score = match get_credit_score(env.clone(), borrower.clone()) {
        Some(score) => score,
        None => return base_fee_bps,
    };

    let rewards = get_tier_rewards(env.clone(), credit_score.tier);
    let discount = base_fee_bps * rewards.fee_discount_bps as u32 / 10000;
    base_fee_bps - discount
}
