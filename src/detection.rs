use crate::errors::ContractError;
use crate::types::{
    DataKey, FraudScoreConfig, VoucherFraudScore, VouchRecord,
    DEFAULT_FRAUD_SCORE_CONFIG,
};
use soroban_sdk::{symbol_short, Address, Env, Vec};

/// Get the fraud score configuration, or default if not set.
pub fn get_fraud_score_config(env: &Env) -> FraudScoreConfig {
    env.storage()
        .instance()
        .get(&DataKey::FraudScoreConfig)
        .unwrap_or(DEFAULT_FRAUD_SCORE_CONFIG)
}

/// Calculate the rapidity score (0-1000) based on how many distinct
/// borrowers a voucher has backed. More borrowers in a short time
/// suggests sybil or automated vouch farming.
fn calculate_rapidity_score(total_borrowers: u32) -> u32 {
    let max_borrowers = 20u32;
    let raw = (total_borrowers as u64) * 1000u64 / (max_borrowers as u64);
    raw.min(1000) as u32
}

/// Calculate the default correlation score (0-1000) based on the ratio
/// of backed borrowers who eventually defaulted.
fn calculate_default_correlation_score(defaulted_count: u32, total_borrowers: u32) -> u32 {
    if total_borrowers == 0 {
        return 0;
    }
    ((defaulted_count as u64) * 1000u64 / (total_borrowers as u64)).min(1000) as u32
}

/// Calculate the churn score (0-1000) based on the ratio of vouches
/// that were withdrawn or decreased vs. total vouches made.
fn calculate_churn_score(total_borrowers: u32, active_count: u32) -> u32 {
    if total_borrowers == 0 {
        return 0;
    }
    let churned = total_borrowers.saturating_sub(active_count);
    ((churned as u64) * 1000u64 / (total_borrowers as u64)).min(1000) as u32
}

/// Calculate the dispute involvement score (0-1000).
fn calculate_dispute_score(dispute_count: u32, _total_borrowers: u32) -> u32 {
    let max_disputes = 5u32;
    let raw = (dispute_count as u64) * 1000u64 / (max_disputes as u64);
    raw.min(1000) as u32
}

/// Calculate the overall weighted fraud score for a voucher.
pub fn calculate_fraud_score(
    env: &Env,
    voucher: &Address,
) -> Result<VoucherFraudScore, ContractError> {
    let config = get_fraud_score_config(env);
    if !config.enabled {
        return Err(ContractError::CreditScoreCalculationFailed);
    }

    let factors = config.factors;

    let borrower_history: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::VoucherHistory(voucher.clone()))
        .unwrap_or(Vec::new(env));

    let total_borrowers = borrower_history.len();

    let mut defaulted_count: u32 = 0;
    let mut active_count: u32 = 0;

    for i in 0..total_borrowers {
        let borrower = borrower_history.get(i).unwrap();

        let def_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::DefaultCount(borrower.clone()))
            .unwrap_or(0);
        if def_count > 0 {
            defaulted_count += 1;
        }

        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(env));

        if vouches.iter().any(|v| v.voucher == voucher.clone()) {
            active_count += 1;
        }
    }

    let dispute_count: u32 = count_voucher_disputes(env, voucher, &borrower_history);

    let rapidity_score = calculate_rapidity_score(total_borrowers);
    let default_correlation_score =
        calculate_default_correlation_score(defaulted_count, total_borrowers);
    let churn_score = calculate_churn_score(total_borrowers, active_count);
    let dispute_score = calculate_dispute_score(dispute_count, total_borrowers);

    let weighted_score = (rapidity_score as u64 * factors.rapidity_weight as u64
        + default_correlation_score as u64 * factors.default_correlation_weight as u64
        + churn_score as u64 * factors.churn_weight as u64
        + dispute_score as u64 * factors.dispute_weight as u64)
        / 10000;

    let total_stake: i128 = env
        .storage()
        .persistent()
        .get::<DataKey, crate::types::VoucherStats>(&DataKey::VoucherStats(voucher.clone()))
        .map(|s| s.total_slashed)
        .unwrap_or(0);

    Ok(VoucherFraudScore {
        score: weighted_score as u32,
        last_updated: env.ledger().timestamp(),
        total_borrowers,
        defaulted_count,
        churn_count: total_borrowers.saturating_sub(active_count),
        dispute_count,
        total_stake,
        rapidity_score,
        default_correlation_score,
        churn_score,
        dispute_score,
    })
}

/// Count disputes that involve the given voucher by scanning borrower disputes.
fn count_voucher_disputes(
    env: &Env,
    voucher: &Address,
    borrower_history: &Vec<Address>,
) -> u32 {
    let mut count: u32 = 0;
    for i in 0..borrower_history.len() {
        let borrower = borrower_history.get(i).unwrap();
        if env
            .storage()
            .persistent()
            .has(&DataKey::RepaymentDispute(borrower, voucher.clone()))
        {
            count += 1;
        }
    }
    count
}

/// Update fraud score for a voucher (on-demand).
pub fn update_fraud_score(env: Env, voucher: Address) -> Result<(), ContractError> {
    let fraud_score = calculate_fraud_score(&env, &voucher)?;
    env.storage()
        .persistent()
        .set(&DataKey::VoucherFraudScore(voucher.clone()), &fraud_score);

    env.events().publish(
        (symbol_short!("fraud"), symbol_short!("update")),
        (voucher, fraud_score.score),
    );

    Ok(())
}

/// Get fraud score for a voucher (view function).
pub fn get_fraud_score(env: Env, voucher: Address) -> Option<VoucherFraudScore> {
    env.storage()
        .persistent()
        .get(&DataKey::VoucherFraudScore(voucher))
}

/// Set fraud score configuration (admin only).
pub fn set_fraud_score_config(
    env: Env,
    admin_signers: Vec<Address>,
    config: FraudScoreConfig,
) -> Result<(), ContractError> {
    crate::helpers::require_admin_approval(&env, &admin_signers);

    let total_weight = config.factors.rapidity_weight
        + config.factors.default_correlation_weight
        + config.factors.churn_weight
        + config.factors.dispute_weight;

    if total_weight != 10000 {
        return Err(ContractError::InvalidCreditConfig);
    }

    env.storage()
        .instance()
        .set(&DataKey::FraudScoreConfig, &config);

    env.events().publish(
        (symbol_short!("fraud"), symbol_short!("config")),
        admin_signers.get(0),
    );

    Ok(())
}

/// Get fraud score configuration (view function).
pub fn get_fraud_score_config_view(env: Env) -> FraudScoreConfig {
    get_fraud_score_config(&env)
}
