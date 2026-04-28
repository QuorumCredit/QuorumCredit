use crate::errors::ContractError;
use crate::helpers::{
    bps_of, config, extend_ttl, get_active_loan_record, get_slash_balance, has_active_loan,
    next_loan_id, require_allowed_token, require_not_paused, validate_loan_active,
};
use crate::reputation::ReputationNftExternalClient;
use crate::types::{
    DataKey, LoanRecord, LoanStatus, VouchRecord, DEFAULT_REFERRAL_BONUS_BPS, MIN_VOUCH_AGE,
};
use soroban_sdk::{panic_with_error, symbol_short, Address, Env, Vec};

/// ------------------------------
/// Dynamic Yield Calculation
/// ------------------------------
fn calculate_dynamic_yield(
    env: &Env,
    borrower: &Address,
    amount: i128,
    duration: i128,
    cfg: &crate::types::Config,
) -> i128 {
    // Utilization rate (proxy)
    let total_loans: i128 = env
        .storage()
        .instance()
        .get(&crate::types::DataKey::TotalLoans)
        .unwrap_or(1);

    let total_staked: i128 = env
        .storage()
        .instance()
        .get(&crate::types::DataKey::TotalStaked)
        .unwrap_or(1);

    let utilization = (total_loans * 10_000) / total_staked.max(1);

    // Risk from defaults vs repayments
    let default_count: u32 = env
        .storage()
        .persistent()
        .get(&crate::types::DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(0);

    let repayment_count: u32 = env
        .storage()
        .persistent()
        .get(&crate::types::DataKey::RepaymentCount(borrower.clone()))
        .unwrap_or(1);

    let risk_score = (default_count as i128 * 10_000)
        / (repayment_count as i128 + 1);

    // Credit strength (vouch-based proxy)
    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(env));

    let mut credit_score: i128 = 0;
    for v in vouches.iter() {
        credit_score += v.amount;
    }

    let credit_factor = credit_score / 1_000;

    // Size & duration adjustments
    let size_factor = amount / 10_000;
    let duration_factor = duration / 30;

    // Base rate
    let mut rate = cfg.base_yield_bps as i128;

    // Weighted adjustments
    rate += (utilization * cfg.utilization_weight as i128) / 10_000;
    rate += (risk_score * cfg.risk_weight as i128) / 10_000;
    rate -= (credit_factor * cfg.credit_weight as i128) / 10_000;
    rate += size_factor;
    rate += duration_factor;

    // Safety bounds
    if rate < cfg.min_yield_bps as i128 {
        rate = cfg.min_yield_bps as i128;
    }

    if rate > cfg.max_yield_bps as i128 {
        rate = cfg.max_yield_bps as i128;
    }

    rate
}

/// Register a referrer for a borrower. Must be called before `request_loan`.
pub fn register_referral(
    env: Env,
    borrower: Address,
    referrer: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    assert!(borrower != referrer, "borrower cannot refer themselves");
    assert!(
        !has_active_loan(&env, &borrower),
        "cannot set referral with active loan"
    );

    env.storage()
        .persistent()
        .set(&DataKey::ReferredBy(borrower.clone()), &referrer);

    extend_ttl(&env, &DataKey::ReferredBy(borrower.clone()));

    env.events().publish(
        (symbol_short!("referral"), symbol_short!("set")),
        (borrower, referrer),
    );

    Ok(())
}

pub fn get_referrer(env: Env, borrower: Address) -> Option<Address> {
    env.storage()
        .persistent()
        .get(&DataKey::ReferredBy(borrower))
}

pub fn request_loan(
    env: Env,
    borrower: Address,
    amount: i128,
    threshold: i128,
    loan_purpose: soroban_sdk::String,
    token_addr: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    if env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Blacklisted(borrower.clone()))
        .unwrap_or(false)
    {
        return Err(ContractError::Blacklisted);
    }

    let cfg = config(&env);

    assert!(
        amount >= cfg.min_loan_amount,
        "loan amount must meet minimum threshold"
    );
    assert!(threshold > 0, "threshold must be greater than zero");

    let token_client = require_allowed_token(&env, &token_addr)?;

    let max_loan_amount: i128 = env
        .storage()
        .instance()
        .get(&DataKey::MaxLoanAmount)
        .unwrap_or(0);

    if max_loan_amount > 0 && amount > max_loan_amount {
        return Err(ContractError::LoanExceedsMaxAmount);
    }

    assert!(
        !has_active_loan(&env, &borrower),
        "borrower already has an active loan"
    );

    let now = env.ledger().timestamp();

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));

    let mut total_stake: i128 = 0;
    for v in vouches.iter() {
        total_stake += v.amount;
    }

    if total_stake < threshold {
        panic_with_error!(&env, ContractError::InsufficientFunds);
    }

    let deadline = now + cfg.loan_duration;

    let loan_id = next_loan_id(&env);

    // ------------------------------
    // DYNAMIC YIELD REPLACEMENT
    // ------------------------------
    let yield_bps = calculate_dynamic_yield(
        &env,
        &borrower,
        amount,
        cfg.loan_duration as i128,
        &cfg,
    );

    let total_yield = bps_of(amount, yield_bps as u64);

    env.storage().persistent().set(
        &DataKey::Loan(loan_id),
        &LoanRecord {
            id: loan_id,
            borrower: borrower.clone(),
            co_borrowers: Vec::new(&env),
            amount,
            amount_repaid: 0,
            total_yield,
            status: LoanStatus::Active,
            created_at: now,
            disbursement_timestamp: now,
            repayment_timestamp: None,
            deadline,
            loan_purpose,
            token_address: token_addr.clone(),
        },
    );

    extend_ttl(&env, &DataKey::Loan(loan_id));

    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(borrower.clone()), &loan_id);

    extend_ttl(&env, &DataKey::ActiveLoan(borrower.clone()));

    env.storage()
        .persistent()
        .set(&DataKey::LatestLoan(borrower.clone()), &loan_id);

    extend_ttl(&env, &DataKey::LatestLoan(borrower.clone()));

    let count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCount(borrower.clone()))
        .unwrap_or(0);

    env.storage()
        .persistent()
        .set(&DataKey::LoanCount(borrower.clone()), &(count + 1));

    extend_ttl(&env, &DataKey::LoanCount(borrower.clone()));

    token_client.transfer(&env.current_contract_address(), &borrower, &amount);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("disbursed")),
        (borrower.clone(), amount, deadline, token_addr),
    );

    Ok(())
}

/* --------------------------------------------------
   Everything below remains unchanged (repay, views)
-------------------------------------------------- */

pub fn repay(env: Env, borrower: Address, payment: i128) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if borrower != loan.borrower {
        return Err(ContractError::UnauthorizedCaller);
    }

    validate_loan_active(&loan)?;

    let total_owed = loan.amount + loan.total_yield;
    let outstanding = total_owed - loan.amount_repaid;

    if payment <= 0 || payment > outstanding {
        return Err(ContractError::InvalidAmount);
    }

    let token = soroban_sdk::token::Client::new(&env, &loan.token_address);

    token.transfer(&borrower, &env.current_contract_address(), &payment);
    loan.amount_repaid += payment;

    if loan.amount_repaid >= total_owed {
        loan.status = LoanStatus::Repaid;
        loan.repayment_timestamp = Some(env.ledger().timestamp());
    }

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    Ok(())
}

pub fn loan_status(env: Env, borrower: Address) -> LoanStatus {
    match crate::helpers::get_latest_loan_record(&env, &borrower) {
        None => LoanStatus::None,
        Some(loan) => loan.status,
    }
}