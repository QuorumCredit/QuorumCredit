use crate::errors::ContractError;
use crate::helpers::{
    config, deduct_slash_balance, get_active_loan_record, get_latest_loan_record,
    has_active_loan, next_loan_id, register_borrower_if_needed, require_allowed_token,
    require_not_paused, require_not_thawing, require_admin_approval,
    require_governance_participant,
};
use crate::reputation::ReputationNftExternalClient;
use crate::types::{
    BorrowerDynamicRate, DataKey, DynamicRateConfig, EscrowStatus,
    ForbearanceRecord, ForbearanceStatus, LoanRecord, LoanStatus, LoanStatusEx,
    RefinanceRecord, SlashRecord, VouchRecord, VoucherStats,
    YieldDistributionEntry,
    BPS_DENOMINATOR, DEFAULT_DYNAMIC_RATE_CONFIG, DEFAULT_FORBEARANCE_DURATION_SECS,
    MAX_FORBEARANCE_PERIODS, REPUTATION_BONUS_MAX_BPS, SLASH_ESCROW_PERIOD,
};
use soroban_sdk::{panic_with_error, symbol_short, Address, Env, Vec};

/// Vouch-age bonus constants
const VOUCH_AGE_BONUS_MIN_SECS: u64 = 30 * 24 * 60 * 60;   // 30 days
const VOUCH_AGE_BONUS_PERIOD_SECS: u64 = 30 * 24 * 60 * 60; // 30 days per period
const VOUCH_AGE_BONUS_BPS_PER_PERIOD: i128 = 25;             // +25 bps per period
const VOUCH_AGE_BONUS_MAX_BPS: i128 = 200;                   // cap at 200 bps

/// Get or compute the yield rate for a single vouch with caching (Issue #934).
pub fn vouch_yield_bps(env: &Env, vouch: &VouchRecord, borrower: &Address, now: u64) -> i128 {
    let cfg = config(env);
    
    // Try to get cached yield
    if let Some(cached_yield) = crate::cache::get_cached_yield(env, borrower, &vouch.voucher, cfg.yield_bps) {
        return cached_yield;
    }
    
    // Compute yield if not cached
    let yield_bps = vouch_yield_bps_uncached(env, vouch, borrower, now);
    
    // Cache the result
    crate::cache::set_cached_yield(env, borrower, &vouch.voucher, yield_bps, cfg.yield_bps);
    
    yield_bps
}

/// Compute the yield rate (in bps) for a single vouch, incorporating:
/// - base yield from config
/// - vouch-age bonus: +25 bps per 30-day period the vouch has been active (capped at 200 bps)
/// - borrower reputation bonus: up to +100 bps based on successful repayment history
/// - voucher reputation bonus: up to +100 bps based on voucher's successful vouch history (Issue #866)
fn vouch_yield_bps_uncached(env: &Env, vouch: &VouchRecord, borrower: &Address, now: u64) -> i128 {
    let base_bps = config(env).yield_bps;

    // ── Vouch-age bonus ───────────────────────────────────────────────────────
    let age_secs = now.saturating_sub(vouch.vouch_timestamp);
    let age_bonus = if age_secs >= VOUCH_AGE_BONUS_MIN_SECS {
        let periods = (age_secs / VOUCH_AGE_BONUS_PERIOD_SECS) as i128;
        (periods * VOUCH_AGE_BONUS_BPS_PER_PERIOD).min(VOUCH_AGE_BONUS_MAX_BPS)
    } else {
        0
    };

    // ── Borrower reputation bonus ─────────────────────────────────────────────
    // Successful repayments → higher bonus; defaults → penalty.
    let repayment_count: i128 = env
        .storage()
        .persistent()
        .get::<DataKey, u32>(&DataKey::RepaymentCount(borrower.clone()))
        .unwrap_or(0) as i128;
    let default_count: i128 = env
        .storage()
        .persistent()
        .get::<DataKey, u32>(&DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(0) as i128;
    // +10 bps per successful repayment, -20 bps per default, capped at [0, REPUTATION_BONUS_MAX_BPS]
    let rep_bonus = ((repayment_count * 10) - (default_count * 20))
        .max(0)
        .min(REPUTATION_BONUS_MAX_BPS);

    // ── Voucher reputation bonus (Issue #866) ─────────────────────────────────
    // Vouchers with more successful vouch history earn a yield bonus.
    let voucher_stats: Option<VoucherStats> = env
        .storage()
        .persistent()
        .get(&DataKey::VoucherStats(vouch.voucher.clone()));
    let voucher_rep_bonus = voucher_stats
        .as_ref()
        .map(|s| (s.successful_vouches as i128 * 10).min(REPUTATION_BONUS_MAX_BPS))
        .unwrap_or(0);

    // ── Voucher reliability bonus ─────────────────────────────────────────────
    // Vouchers with a clean track record (no slashed vouches) get a yield bonus.
    // - perfect record (successful > 0, slashed == 0): +150 bps
    // - high reliability: scaled by (successful / (successful + slashed + 1))
    const RELIABILITY_MAX_BPS: i128 = 150;
    let reliability_bonus = voucher_stats
        .map(|s| {
            if s.total_vouches_slashed == 0 && s.successful_vouches > 0 {
                RELIABILITY_MAX_BPS
            } else if s.total_vouches_slashed > 0 || s.successful_vouches > 0 {
                let total = s.successful_vouches as i128 + s.total_vouches_slashed as i128;
                if total > 0 {
                    let ratio_bps = s.successful_vouches as i128 * BPS_DENOMINATOR / total;
                    RELIABILITY_MAX_BPS * ratio_bps / BPS_DENOMINATOR
                } else {
                    0
                }
            } else {
                0
            }
        })
        .unwrap_or(0);

    // ── Diversification bonus ─────────────────────────────────────────────────
    // +50 bps per additional unique borrower, max 500 bps (5%)
    let history: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::VoucherHistory(vouch.voucher.clone()))
        .unwrap_or(Vec::new(env));
    let mut unique_borrowers = Vec::new(env);
    for b in history.iter() {
        if !unique_borrowers.iter().any(|ub| ub == &b) {
            unique_borrowers.push_back(b.clone());
        }
    }
    let unique_count = unique_borrowers.len() as i128;
    let diversification_bonus = ((unique_count - 1) * 50).min(500);

    (base_bps + age_bonus + rep_bonus + voucher_rep_bonus + reliability_bonus + diversification_bonus).max(0)
}

/// Calculate dynamic yield (legacy — used for backward-compat; prefer vouch_yield_bps per vouch).
pub fn calculate_dynamic_yield(env: &Env, borrower: &Address) -> i128 {
    let base_bps = config(env).yield_bps;

    let credit_score: i128 = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::ReputationNft)
        .map(|nft| ReputationNftExternalClient::new(env, &nft).balance(borrower) as i128)
        .unwrap_or(0);

    let default_count: i128 = env
        .storage()
        .persistent()
        .get::<DataKey, u32>(&DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(0) as i128;

    (base_bps + (credit_score / 100) - (default_count * 50)).max(0)
}

/// Request loan
pub fn request_loan(
    env: Env,
    borrower: Address,
    amount: i128,
    threshold: i128,
    loan_purpose: soroban_sdk::String,
    token_addr: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;
    crate::helpers::check_rate_limit(&env, &borrower)?;
    crate::helpers::check_permission(&env, &borrower, |p| p.can_request_loan)?;
    register_borrower_if_needed(&env, &borrower);

    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    let token = require_allowed_token(&env, &token_addr)?;
    let cfg = config(&env);

    if amount < cfg.min_loan_amount {
        return Err(ContractError::LoanBelowMinAmount);
    }

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));

    // Compute reputation-weighted total stake (Issue #866)
    let total_stake: i128 = vouches
        .iter()
        .filter(|v| v.token == token_addr)
        .map(|v| {
            let weight = crate::vouch::vouch_reputation_weight(&env, &v.voucher);
            v.stake * weight / BPS_DENOMINATOR
        })
        .sum();

    if total_stake < threshold {
        panic_with_error!(&env, ContractError::InsufficientFunds);
    }

    if amount > total_stake * (cfg.max_loan_to_collateral_ratio as i128) / 10_000 {
        return Err(ContractError::LoanExceedsMaxRatio);
    }

    let now = env.ledger().timestamp();
    let loan_id = next_loan_id(&env);

    // ── Credit score tier rewards ────────────────────────────────────────────
    // Apply tier-based yield bonus to base yield rate (Issue #866)
    let tier_adjusted_yield_bps = crate::credit_score::apply_tier_rewards_to_yield(
        &env, &borrower, config(&env).yield_bps,
    );

    // ── Per-vouch yield (age + reputation + tier + weight aware) ────────────
    // Compute each voucher's individual yield share using reputation-weighted stake.
    let mut yield_distribution: Vec<YieldDistributionEntry> = Vec::new(&env);
    let mut total_yield: i128 = 0;

    for v in vouches.iter() {
        if v.token != token_addr {
            continue;
        }
        let rate = vouch_yield_bps(&env, &v, &borrower, now);
        // Apply tier rewards on top of per-vouch rate (Issue #866)
        let effective_rate = rate + (tier_adjusted_yield_bps - config(&env).yield_bps);
        let weight = crate::vouch::vouch_reputation_weight(&env, &v.voucher);
        let weighted_stake = v.stake * weight / BPS_DENOMINATOR;
        let vouch_yield = amount * weighted_stake / total_stake * effective_rate / 10_000;
        total_yield += vouch_yield;
        yield_distribution.push_back(YieldDistributionEntry {
            voucher: v.voucher.clone(),
            yield_amount: vouch_yield,
        });
    }

    // Store yield distribution for repayment-time lookup
    env.storage()
        .persistent()
        .set(&DataKey::YieldDistribution(loan_id), &yield_distribution);

    let loan = LoanRecord {
        id: loan_id,
        borrower: borrower.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: EscrowStatus::None,
        co_borrowers: Vec::new(&env),
        amount,
        amount_repaid: 0,
        total_yield,
        status: LoanStatus::Active,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: None,
        deadline: now + cfg.loan_duration,
        loan_purpose,
        token_address: token_addr.clone(),
        amortization_schedule: Vec::new(&env),
        reminder_sent: false,
        risk_score: 0,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: crate::types::RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: false,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };

    env.storage().persistent().set(&DataKey::Loan(loan_id), &loan);
    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(borrower.clone()), &loan_id);
    env.storage()
        .persistent()
        .set(&DataKey::LatestLoan(borrower.clone()), &loan_id);

    token.transfer(&env.current_contract_address(), &borrower, &amount);

    // Issue #882: Collect insurance fee at loan disbursement
    crate::insurance::collect_loan_fee(&env, amount);
    env.storage()
        .persistent()
        .set(&DataKey::InsuranceLinked(loan_id), &true);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("created")),
        (borrower, amount),
    );

    Ok(())
}

/// Apply slash recovery to borrower when a defaulted loan is fully repaid.
pub fn apply_slash_recovery(env: &Env, borrower: &Address) -> Result<(), ContractError> {
    let cfg = config(env);
    if cfg.recovery_percentage == 0 {
        return Ok(());
    }

    let mut record: SlashRecord = match env
        .storage()
        .persistent()
        .get::<DataKey, SlashRecord>(&DataKey::SlashAudit(borrower.clone()))
    {
        Some(r) if !r.reversed => r,
        _ => return Ok(()),
    };

    if record.recovery_amount > 0 {
        return Ok(());
    }

    let recoverable = record.total_slashed * cfg.recovery_percentage as i128 / BPS_DENOMINATOR;
    if recoverable <= 0 {
        return Ok(());
    }

    deduct_slash_balance(env, recoverable)?;

    let loan = get_latest_loan_record(env, borrower).ok_or(ContractError::NoActiveLoan)?;
    let token = require_allowed_token(env, &loan.token_address)?;
    token.transfer(
        &env.current_contract_address(),
        borrower,
        &recoverable,
    );

    record.recovery_amount = recoverable;
    env.storage()
        .persistent()
        .set(&DataKey::SlashRecord(record.slash_id), &record);
    env.storage()
        .persistent()
        .set(&DataKey::SlashAudit(borrower.clone()), &record);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("recovery")),
        (borrower.clone(), recoverable),
    );

    Ok(())
}

/// Repay loan (active or defaulted).
pub fn repay(env: Env, borrower: Address, payment: i128) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;
    crate::helpers::check_rate_limit(&env, &borrower)?;
    crate::helpers::check_permission(&env, &borrower, |p| p.can_repay)?;

     let mut loan = match get_active_loan_record(&env, &borrower) {
         Ok(l) => l,
         Err(ContractError::NoActiveLoan) => {
             let l = get_latest_loan_record(&env, &borrower).ok_or(ContractError::NoActiveLoan)?;
             if l.status != LoanStatus::Defaulted {
                 env.events().publish(
                     (symbol_short!("loan"), symbol_short!("repay_err")),
                     (borrower.clone(), payment),
                 );
                 return Err(ContractError::NoActiveLoan);
             }
             l
         }
         Err(e) => {
             env.events().publish(
                 (symbol_short!("loan"), symbol_short!("repay_err")),
                 (borrower.clone(), payment),
             );
             return Err(e);
         }
     };

    let was_defaulted = loan.status == LoanStatus::Defaulted;

    if payment <= 0 {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let total_owed = loan.amount.checked_add(loan.total_yield).ok_or(ContractError::ArithmeticError)?;
    let outstanding = total_owed.checked_sub(loan.amount_repaid).ok_or(ContractError::ArithmeticError)?;

    if payment > outstanding {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let token = require_allowed_token(&env, &loan.token_address)?;
    token.transfer(&borrower, &env.current_contract_address(), &payment);

    loan.amount_repaid = loan.amount_repaid.checked_add(payment).ok_or(ContractError::ArithmeticError)?;

    let now = env.ledger().timestamp();
    let cfg = config(&env);

    let mut penalty: i128 = 0;
    if !was_defaulted && now < loan.deadline && cfg.prepayment_penalty_bps > 0 {
        let repaid_principal = loan.amount_repaid.checked_mul(loan.amount).ok_or(ContractError::ArithmeticError)? / total_owed;
        let remaining_principal = loan.amount.checked_sub(repaid_principal).ok_or(ContractError::ArithmeticError)?;
        penalty = remaining_principal.checked_mul(cfg.prepayment_penalty_bps as i128).ok_or(ContractError::ArithmeticError)? / BPS_DENOMINATOR;
    }

    let fully_repaid = loan.amount_repaid >= total_owed;

    if fully_repaid && was_defaulted {
        loan.status = LoanStatus::Repaid;
        loan.repayment_timestamp = Some(now);
        apply_slash_recovery(&env, &borrower)?;
        env.events().publish(
            (symbol_short!("loan"), symbol_short!("repaid")),
            (borrower.clone(), loan.amount),
        );
    } else if fully_repaid {
        loan.status = LoanStatus::Repaid;
        loan.repayment_timestamp = Some(now);

        // Process withdrawal queue BEFORE vouch payout (Issue #865: progressive stake unlock).
        // This removes queued withdrawals from the vouches list and transfers their stake back.
        crate::vouch::process_withdrawal_queue(&env, &borrower);

        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        // Load the per-vouch yield distribution locked in at disbursement.
        let yield_dist: Vec<YieldDistributionEntry> = env
            .storage()
            .persistent()
            .get(&DataKey::YieldDistribution(loan.id))
            .unwrap_or(Vec::new(&env));

        // Any penalty is distributed proportionally to stake (fallback).
        let total_stake: i128 = vouches
            .iter()
            .filter(|v| v.token == loan.token_address)
            .map(|v| v.stake)
            .sum();

        for v in vouches.iter() {
            if v.token != loan.token_address {
                continue;
            }

            // Per-vouch yield from the distribution locked at disbursement.
            let vouch_yield = yield_dist
                .iter()
                .find(|e| e.voucher == v.voucher)
                .map(|e| e.yield_amount)
                .unwrap_or(0);

            // Penalty share is proportional to stake.
            let penalty_share = if total_stake > 0 {
                penalty * v.stake / total_stake
            } else {
                0
            };

            let payout = v.stake + vouch_yield + penalty_share;
            
            // Issue #935: Queue transfer for batch processing
            crate::batch_transfer::queue_transfer(&env, v.voucher.clone(), payout, loan.token_address.clone());

            let mut stats: crate::types::VoucherStats = env
                .storage()
                .persistent()
                .get(&DataKey::VoucherStats(v.voucher.clone()))
                .unwrap_or(crate::types::VoucherStats {
                    successful_vouches: 0,
                    total_vouches_slashed: 0,
                    total_yield_earned: 0,
                    total_slashed: 0,
                });
            stats.successful_vouches += 1;
            stats.total_yield_earned += vouch_yield;
            env.storage()
                .persistent()
                .set(&DataKey::VoucherStats(v.voucher.clone()), &stats);
        }

        // Issue #935: Flush all queued transfers in a single batch
        crate::batch_transfer::flush_transfers(&env)?;

        // Increment borrower repayment count (feeds future reputation bonus).
        let prev_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RepaymentCount(borrower.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::RepaymentCount(borrower.clone()), &(prev_count + 1));

        // Issue #884: Apply prepayment bonus for early repayment
        let bonus = apply_prepayment_bonus(&env, &borrower, &loan);
        if bonus > 0 {
            let contract_balance = token.balance(&env.current_contract_address());
            if contract_balance >= bonus {
                // Issue #935: Queue bonus transfer for batch processing
                crate::batch_transfer::queue_transfer(&env, borrower.clone(), bonus, loan.token_address.clone());
                crate::batch_transfer::flush_transfers(&env)?;
                env.events().publish(
                    (symbol_short!("loan"), symbol_short!("bonus")),
                    (borrower.clone(), bonus),
                );
            }
        }

        // Try to mint excellent credit tier badge if eligible
        let _ = crate::reputation::mint_excellent_badge(&env, &borrower);

        // Update credit score after successful repayment
        let _ = crate::credit_score::update_credit_score(env.clone(), borrower.clone());

        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(borrower.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::YieldDistribution(loan.id));

        env.events().publish(
            (symbol_short!("loan"), symbol_short!("repaid")),
            (borrower.clone(), loan.amount),
        );
    }

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    Ok(())
}

/// Eligibility check
pub fn is_eligible(env: Env, borrower: Address, threshold: i128, token: Address) -> bool {
    if threshold <= 0 {
        return false;
    }

    if has_active_loan(&env, &borrower) {
        return false;
    }

    // O(1) eligibility check using cached total weighted stake
    let total = crate::vouch::get_cached_weighted_stake(&env, &borrower, &token);

    total >= threshold
}

/// Partial repay (FIXED DIRECTION BUG)
pub fn repay_partial(
    env: Env,
    borrower: Address,
    payment: i128,
    token: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;
    crate::helpers::check_rate_limit(&env, &borrower)?;
    crate::helpers::check_permission(&env, &borrower, |p| p.can_repay)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if payment <= 0 {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let token_client = require_allowed_token(&env, &token)?;

    // FIX: transfer should be FROM borrower TO contract
    token_client.transfer(&borrower, &env.current_contract_address(), &payment);

    loan.amount_repaid = loan.amount_repaid.checked_add(payment).ok_or(ContractError::ArithmeticError)?;

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("prt_rep")),
        (borrower, payment),
    );

    Ok(())
}

/// Set yield reserve
pub fn set_yield_reserve(
    env: Env,
    admins: Vec<Address>,
    amount: i128,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admins);

    if amount < 0 {
        return Err(ContractError::InvalidAmount);
    }

    env.storage()
        .persistent()
        .set(&DataKey::YieldReserve, &amount);

    Ok(())
}

/// Get yield reserve
pub fn get_yield_reserve_balance(env: Env) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::YieldReserve)
        .unwrap_or(0)
}

/// Set borrower risk score
pub fn set_borrower_risk_score(
    env: Env,
    admins: Vec<Address>,
    borrower: Address,
    risk_score: u32,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admins);

    if risk_score > 100 {
        return Err(ContractError::InvalidAmount);
    }

    let mut loan = get_active_loan_record(&env, &borrower)?;
    loan.risk_score = risk_score;

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    Ok(())
}

pub fn release_slash_escrow(
    env: Env,
    admin_signers: Vec<Address>,
    borrower: Address,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    let (amount, release_ts): (i128, u64) = env
        .storage()
        .persistent()
        .get(&DataKey::SlashEscrow(borrower.clone()))
        .ok_or(ContractError::NoActiveLoan)?;
    if env.ledger().timestamp() < release_ts {
        return Err(ContractError::TimelockNotReady);
    }
    crate::helpers::add_slash_balance(&env, amount);
    env.storage()
        .persistent()
        .remove(&DataKey::SlashEscrow(borrower));
    Ok(())
}

pub fn get_loan(env: Env, borrower: Address) -> Option<LoanRecord> {
    if let Ok(loan) = get_active_loan_record(&env, &borrower) {
        return Some(loan);
    }
    get_latest_loan_record(&env, &borrower)
}

pub fn loan_status_extended(env: Env, borrower: Address) -> LoanStatusEx {
    if let Some(loan) = get_loan(env, borrower) {
        if loan.status == LoanStatus::Active && loan.suspension_timestamp.is_some() {
            return LoanStatusEx::Suspended;
        }
        return loan.status.into();
    }
    LoanStatusEx::None
}

pub fn get_loan_by_id(env: Env, loan_id: u64) -> Option<LoanRecord> {
    env.storage().persistent().get(&DataKey::Loan(loan_id))
}

pub fn loan_status(env: Env, borrower: Address) -> LoanStatus {
    get_loan(env, borrower)
        .map(|l| l.status)
        .unwrap_or(LoanStatus::None)
}

pub fn suspend_loan_on_missed_payment(
    env: Env,
    caller: Address,
    borrower: Address,
) -> Result<(), ContractError> {
    caller.require_auth();
    require_not_paused(&env)?;
    require_governance_participant(&env, &caller)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;
    let now = env.ledger().timestamp();

    if loan.status != LoanStatus::Active || loan.suspension_timestamp.is_some() {
        return Err(ContractError::InvalidStateTransition);
    }
    if now >= loan.deadline {
        return Err(ContractError::LoanPastDeadline);
    }

    loan.suspension_timestamp = Some(now);
    loan.suspension_amount_repaid = loan.amount_repaid;

    env.storage().persistent().set(&DataKey::Loan(loan.id), &loan);
    env.events().publish(
        (symbol_short!("loan"), symbol_short!("suspended")),
        (borrower.clone(), loan.id, now),
    );

    Ok(())
}

pub fn resume_loan(
    env: Env,
    caller: Address,
    borrower: Address,
) -> Result<(), ContractError> {
    caller.require_auth();
    require_not_paused(&env)?;
    require_governance_participant(&env, &caller)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;
    let now = env.ledger().timestamp();

    let suspension_ts = loan
        .suspension_timestamp
        .ok_or(ContractError::InvalidStateTransition)?;

    if now < suspension_ts.checked_add(crate::types::PAYMENT_GRACE_PERIOD).ok_or(ContractError::ArithmeticError)? {
        return Err(ContractError::InvalidStateTransition);
    }
    if loan.amount_repaid <= loan.suspension_amount_repaid {
        return Err(ContractError::InvalidStateTransition);
    }

    loan.suspension_timestamp = None;
    loan.suspension_amount_repaid = 0;

    env.storage().persistent().set(&DataKey::Loan(loan.id), &loan);
    env.events().publish(
        (symbol_short!("loan"), symbol_short!("resumed")),
        (borrower.clone(), loan.id, now),
    );

    Ok(())
}

pub fn repayment_count(env: Env, borrower: Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::RepaymentCount(borrower))
        .unwrap_or(0)
}

pub fn loan_count(env: Env, borrower: Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::LoanCount(borrower))
        .unwrap_or(0)
}

pub fn default_count(env: Env, borrower: Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::DefaultCount(borrower))
        .unwrap_or(0)
}

pub fn register_referral(
    _env: Env,
    _borrower: Address,
    _referrer: Address,
) -> Result<(), ContractError> {
    Err(ContractError::InvalidStateTransition)
}

pub fn get_referrer(_env: Env, _borrower: Address) -> Option<Address> {
    None
}

// ── Issue #880: Co-Borrower Support ──────────────────────────────────────────

const MAX_CO_BORROWERS: u32 = 5;

pub fn add_co_borrower(
    env: Env,
    borrower: Address,
    co_borrower: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    if borrower == co_borrower {
        return Err(ContractError::SelfCoBorrowerNotAllowed);
    }

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if loan.co_borrowers.len() >= MAX_CO_BORROWERS {
        return Err(ContractError::MaxCoBorrowersExceeded);
    }

    if loan.co_borrowers.iter().any(|cb| cb == co_borrower) {
        return Err(ContractError::CoBorrowerAlreadyAdded);
    }

    loan.co_borrowers.push_back(co_borrower.clone());
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("co_add")),
        (borrower, co_borrower, loan.id),
    );

    Ok(())
}

pub fn remove_co_borrower(
    env: Env,
    borrower: Address,
    co_borrower: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    let idx = loan.co_borrowers.iter().position(|cb| cb == co_borrower);
    match idx {
        Some(i) => {
            loan.co_borrowers.remove(i as u32);
            env.storage()
                .persistent()
                .set(&DataKey::Loan(loan.id), &loan);

            env.events().publish(
                (symbol_short!("loan"), symbol_short!("co_rm")),
                (borrower, co_borrower, loan.id),
            );

            Ok(())
        }
        None => Err(ContractError::VoucherNotFound),
    }
}

pub fn get_co_borrowers(env: Env, borrower: Address) -> Vec<Address> {
    match get_active_loan_record(&env, &borrower) {
        Ok(loan) => loan.co_borrowers,
        Err(_) => Vec::new(&env),
    }
}

// ── Issue #879: Loan Refinancing ─────────────────────────────────────────────

pub fn refinance_loan(
    env: Env,
    borrower: Address,
    new_amount: i128,
    new_threshold: i128,
    new_token: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;
    crate::helpers::check_rate_limit(&env, &borrower)?;

    let mut old_loan = get_active_loan_record(&env, &borrower)?;

    let now = env.ledger().timestamp();
    if now >= old_loan.deadline {
        return Err(ContractError::LoanPastDeadline);
    }

    let total_owed = old_loan.amount
        .checked_add(old_loan.total_yield)
        .ok_or(ContractError::ArithmeticError)?;
    let outstanding = total_owed
        .checked_sub(old_loan.amount_repaid)
        .ok_or(ContractError::ArithmeticError)?;

    if outstanding <= 0 {
        return Err(ContractError::RefinanceNoOutstanding);
    }

    if new_amount < outstanding {
        return Err(ContractError::InvalidAmount);
    }

    let cfg = config(&env);
    if new_amount < cfg.min_loan_amount {
        return Err(ContractError::LoanBelowMinAmount);
    }

    let token = require_allowed_token(&env, &new_token)?;

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));

    let total_stake: i128 = vouches
        .iter()
        .filter(|v| v.token == new_token)
        .map(|v| {
            let weight = crate::vouch::vouch_reputation_weight(&env, &v.voucher);
            v.stake * weight / BPS_DENOMINATOR
        })
        .sum();

    if total_stake < new_threshold {
        return Err(ContractError::InsufficientFunds);
    }

    let old_rate_bps = if old_loan.amount > 0 {
        old_loan.total_yield * 10_000 / old_loan.amount
    } else {
        cfg.yield_bps
    };

    old_loan.amount_repaid = total_owed;
    old_loan.status = LoanStatus::Repaid;
    old_loan.repayment_timestamp = Some(now);
    env.storage()
        .persistent()
        .set(&DataKey::Loan(old_loan.id), &old_loan);
    env.storage()
        .persistent()
        .remove(&DataKey::ActiveLoan(borrower.clone()));

    let new_loan_id = next_loan_id(&env);
    let new_yield_bps = crate::credit_score::apply_tier_rewards_to_yield(
        &env, &borrower, cfg.yield_bps,
    );
    let new_yield = new_amount * new_yield_bps / 10_000;

    let new_loan = LoanRecord {
        id: new_loan_id,
        borrower: borrower.clone(),
        guarantor: old_loan.guarantor,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: EscrowStatus::None,
        co_borrowers: old_loan.co_borrowers,
        amount: new_amount,
        amount_repaid: 0,
        total_yield: new_yield,
        status: LoanStatus::Active,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: None,
        deadline: now + cfg.loan_duration,
        loan_purpose: old_loan.loan_purpose,
        token_address: new_token.clone(),
        amortization_schedule: Vec::new(&env),
        reminder_sent: false,
        risk_score: old_loan.risk_score,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: old_loan.rate_type,
        index_reference: old_loan.index_reference,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: false,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };

    env.storage()
        .persistent()
        .set(&DataKey::Loan(new_loan_id), &new_loan);
    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(borrower.clone()), &new_loan_id);
    env.storage()
        .persistent()
        .set(&DataKey::LatestLoan(borrower.clone()), &new_loan_id);

    let net_disbursement = new_amount
        .checked_sub(outstanding)
        .ok_or(ContractError::ArithmeticError)?;
    if net_disbursement > 0 {
        token.transfer(&env.current_contract_address(), &borrower, &net_disbursement);
    }

    let refinance_record = RefinanceRecord {
        old_loan_id: old_loan.id,
        new_loan_id,
        borrower: borrower.clone(),
        old_amount: old_loan.amount,
        new_amount,
        old_rate_bps,
        new_rate_bps: new_yield_bps,
        refinanced_at: now,
    };
    env.storage()
        .persistent()
        .set(&DataKey::RefinanceRecord(new_loan_id), &refinance_record);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("refinance")),
        (
            borrower,
            old_loan.id,
            old_loan.amount,
            old_loan.total_yield,
            new_loan_id,
            new_amount,
            new_yield_bps,
        ),
    );

    Ok(())
}

pub fn get_refinance_record(env: Env, loan_id: u64) -> Option<RefinanceRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::RefinanceRecord(loan_id))
}

pub fn deposit_collateral(
    _env: Env,
    _borrower: Address,
    _amount: i128,
    _token: Address,
) -> Result<(), ContractError> {
    Err(ContractError::InvalidStateTransition)
}

pub fn get_borrower_collateral(_env: Env, _borrower: Address) -> i128 {
    0
}

pub fn emit_repayment_reminders(_env: Env) {}
pub fn mint_reputation_nft(_env: Env, _borrower: Address) -> Result<(), ContractError> {
    Ok(())
}
pub fn send_repayment_reminder(_env: Env, _loan_id: u64) -> Result<(), ContractError> {
    Ok(())
}

/// Issue #883: Borrower requests a one-time loan term extension.
/// Charges an extension fee and creates a pending request that vouchers must approve.
pub fn request_extension(
    env: Env,
    borrower: Address,
    extension_secs: u64,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let loan = get_active_loan_record(&env, &borrower)?;

    if loan.status != LoanStatus::Active {
        return Err(ContractError::NoActiveLoan);
    }

    if env
        .storage()
        .persistent()
        .has(&DataKey::LoanExtension(borrower.clone()))
    {
        return Err(ContractError::ExtensionAlreadyRequested);
    }

    let current_count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::ExtensionConsents(borrower.clone()))
        .unwrap_or(0);

    if current_count >= crate::types::MAX_EXTENSIONS_PER_LOAN {
        return Err(ContractError::MaxExtensionsReached);
    }

    if extension_secs == 0 {
        return Err(ContractError::InvalidAmount);
    }

    let fee = loan.amount * crate::types::EXTENSION_FEE_BPS / crate::types::BPS_DENOMINATOR;
    if fee > 0 {
        let token_client = require_allowed_token(&env, &loan.token_address)?;
        token_client.transfer(&borrower, &env.current_contract_address(), &fee);
    }

    let now = env.ledger().timestamp();
    let request = crate::types::LoanExtensionRequest {
        borrower: borrower.clone(),
        loan_id: loan.id,
        extension_secs,
        requested_at: now,
        approvals: Vec::new(&env),
        fee_paid: fee,
        extension_count: current_count,
    };

    env.storage()
        .persistent()
        .set(&DataKey::LoanExtension(borrower.clone()), &request);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("ext_req")),
        (borrower, extension_secs, fee),
    );

    Ok(())
}

/// Issue #883: Voucher approves a pending loan extension request.
/// When a majority of vouchers (by count) approve, the deadline is extended.
pub fn approve_extension(
    env: Env,
    voucher: Address,
    borrower: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    let mut request: crate::types::LoanExtensionRequest = env
        .storage()
        .persistent()
        .get(&DataKey::LoanExtension(borrower.clone()))
        .ok_or(ContractError::InvalidStateTransition)?;

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));

    if !vouches.iter().any(|v| v.voucher == voucher) {
        return Err(ContractError::VoucherNotFound);
    }

    if request.approvals.iter().any(|a| a == voucher) {
        return Err(ContractError::AlreadyVoted);
    }

    request.approvals.push_back(voucher.clone());
    let total_vouchers = vouches.len();
    let required = (total_vouchers / 2) + 1;

    if request.approvals.len() >= required {
        let mut loan = get_active_loan_record(&env, &borrower)?;
        loan.deadline += request.extension_secs;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan.id), &loan);

        let new_count = request.extension_count + 1;
        env.storage()
            .persistent()
            .remove(&DataKey::LoanExtension(borrower.clone()));

        env.storage()
            .persistent()
            .set(&DataKey::ExtensionConsents(borrower.clone()), &new_count);

        env.events().publish(
            (symbol_short!("loan"), symbol_short!("extended")),
            (borrower, request.extension_secs, loan.deadline),
        );
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::LoanExtension(borrower.clone()), &request);

        env.events().publish(
            (symbol_short!("loan"), symbol_short!("ext_appr")),
            (voucher, borrower),
        );
    }

    Ok(())
}

/// Issue #883: Query the pending extension request for a borrower.
pub fn get_extension_request(
    env: Env,
    borrower: Address,
) -> Option<crate::types::LoanExtensionRequest> {
    env.storage()
        .persistent()
        .get(&DataKey::LoanExtension(borrower))
}

pub fn defer_payment(env: Env, borrower: Address) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;
    Err(ContractError::InvalidStateTransition)
}

pub fn check_acceleration(env: Env, _borrower: Address) -> Result<(), ContractError> {
    Err(ContractError::InvalidStateTransition)
}

pub fn set_maturity_date(
    env: Env,
    admin_signers: Vec<Address>,
    borrower: Address,
    _maturity_date: u64,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    let mut loan = get_active_loan_record(&env, &borrower)?;
    loan.maturity_date = Some(_maturity_date);
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);
    Ok(())
}

pub fn set_loan_rate(
    env: Env,
    admin_signers: Vec<Address>,
    borrower: Address,
    rate_type: crate::types::RateType,
    index_reference: Option<soroban_sdk::String>,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);
    let mut loan = get_active_loan_record(&env, &borrower)?;
    loan.rate_type = rate_type;
    loan.index_reference = index_reference;
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);
    Ok(())
}

// ── Issue #656: Loan Guarantee ────────────────────────────────────────────────

pub fn set_loan_guarantor(
    env: Env,
    borrower: Address,
    guarantor: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if loan.guarantor.is_some() {
        return Err(ContractError::InvalidStateTransition);
    }

    loan.guarantor = Some(guarantor.clone());

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("guarantor")),
        (borrower, guarantor),
    );

    Ok(())
}

pub fn remove_loan_guarantor(env: Env, borrower: Address) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;
    loan.guarantor = None;

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    Ok(())
}

// ── Issue #657: Loan Buyback ────────────────────────────────────────────────

pub fn set_buyback_price(
    env: Env,
    borrower: Address,
    price: i128,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_thawing(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if price < 0 {
        return Err(ContractError::InvalidAmount);
    }

    loan.buyback_price = price;

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("bkset")),
        (borrower, price),
    );

    Ok(())
}

pub fn buyback_loan(
    env: Env,
    voucher: Address,
    borrower: Address,
    amount: i128,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_paused(&env)?;

    let loan = get_active_loan_record(&env, &borrower)?;

    if loan.buyback_price == 0 {
        return Err(ContractError::InvalidStateTransition);
    }

    let vouches: soroban_sdk::Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(soroban_sdk::Vec::new(&env));
    let total_stake: i128 = vouches
        .iter()
        .filter(|v| v.voucher == voucher && v.token == loan.token_address)
        .map(|v| v.stake)
        .sum();

    if total_stake == 0 {
        return Err(ContractError::VoucherNotFound);
    }

    let cost = total_stake * loan.buyback_price / BPS_DENOMINATOR;
    if amount < cost {
        return Err(ContractError::InvalidAmount);
    }

    let token_client = require_allowed_token(&env, &loan.token_address)?;
    token_client.transfer(&voucher, &env.current_contract_address(), &cost);
    token_client.transfer(&env.current_contract_address(), &voucher, &total_stake);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("buyback")),
        (borrower, voucher, cost, total_stake),
    );

    Ok(())
}

// ── Issue #658: Automatic Repayment ───────────────────────────────────────────

pub fn enable_auto_repay(env: Env, borrower: Address) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if loan.auto_repay_enabled {
        return Err(ContractError::InvalidStateTransition);
    }

    loan.auto_repay_enabled = true;
    loan.auto_repay_attempts = 0;

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("arpay_on")),
        borrower,
    );

    Ok(())
}

// ── Issue #884: Prepayment Bonus ─────────────────────────────────────────────

/// Set the prepayment bonus rate in basis points (admin-only).
pub fn set_prepayment_bonus_bps(
    env: Env,
    admin_signers: Vec<Address>,
    bonus_bps: u32,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if bonus_bps > 10_000 {
        return Err(ContractError::InvalidAmount);
    }

    env.storage()
        .instance()
        .set(&DataKey::PrepaymentBonusBps, &bonus_bps);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("bonus_set")),
        bonus_bps,
    );

    Ok(())
}

/// Get the current prepayment bonus rate in basis points.
pub fn get_prepayment_bonus_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::PrepaymentBonusBps)
        .unwrap_or(crate::types::DEFAULT_PREPAYMENT_BONUS_BPS)
}

/// Calculate and apply the prepayment bonus for early repayment.
/// Returns the bonus amount awarded (0 if not eligible).
pub fn apply_prepayment_bonus(env: &Env, borrower: &Address, loan: &LoanRecord) -> i128 {
    let now = env.ledger().timestamp();
    if now >= loan.deadline {
        return 0;
    }

    let bonus_bps = get_prepayment_bonus_bps(env);
    if bonus_bps == 0 {
        return 0;
    }

    let total_duration = loan.deadline.saturating_sub(loan.disbursement_timestamp);
    if total_duration == 0 {
        return 0;
    }

    let time_remaining = loan.deadline.saturating_sub(now);
    let early_ratio_bps = (time_remaining as i128 * 10_000) / total_duration as i128;

    // Bonus scales with how early the repayment is
    let bonus = loan.amount * bonus_bps as i128 * early_ratio_bps / (10_000 * 10_000);
    bonus.max(0)
}

// ── Issue #885: Loan Status Privacy ─────────────────────────────────────────

/// Set the privacy level for a borrower's loan details.
pub fn set_loan_privacy(
    env: Env,
    borrower: Address,
    privacy: crate::types::LoanPrivacyLevel,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    env.storage()
        .persistent()
        .set(&DataKey::LoanPrivacy(borrower.clone()), &privacy);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("privacy")),
        (borrower, privacy),
    );

    Ok(())
}

/// Get the privacy level for a borrower's loan details.
pub fn get_loan_privacy(env: &Env, borrower: &Address) -> crate::types::LoanPrivacyLevel {
    env.storage()
        .persistent()
        .get(&DataKey::LoanPrivacy(borrower.clone()))
        .unwrap_or(crate::types::LoanPrivacyLevel::Public)
}

/// Check if a caller has permission to view a borrower's loan details.
pub fn check_loan_visibility(
    env: &Env,
    borrower: &Address,
    caller: &Address,
) -> Result<(), ContractError> {
    let privacy = get_loan_privacy(env, borrower);

    match privacy {
        crate::types::LoanPrivacyLevel::Public => Ok(()),
        crate::types::LoanPrivacyLevel::Private => {
            if caller == borrower {
                Ok(())
            } else {
                Err(ContractError::LoanPrivacyRestricted)
            }
        }
        crate::types::LoanPrivacyLevel::VouchersOnly => {
            if caller == borrower {
                return Ok(());
            }
            let vouches: Vec<VouchRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::Vouches(borrower.clone()))
                .unwrap_or(Vec::new(env));
            if vouches.iter().any(|v| v.voucher == *caller) {
                Ok(())
            } else {
                // Also allow admins
                let cfg = config(env);
                if cfg.admins.iter().any(|a| a == *caller) {
                    Ok(())
                } else {
                    Err(ContractError::LoanPrivacyRestricted)
                }
            }
        }
    }
}

/// Privacy-aware loan query — returns loan details only if caller has permission.
pub fn get_loan_with_privacy(
    env: Env,
    borrower: Address,
    caller: Address,
) -> Result<Option<LoanRecord>, ContractError> {
    check_loan_visibility(&env, &borrower, &caller)?;
    Ok(get_loan(env, borrower))
}

pub fn disable_auto_repay(env: Env, borrower: Address) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;
    loan.auto_repay_enabled = false;

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("arpay_off")),
        borrower,
    );

    Ok(())
}

// ── Issue #878: Loan Forbearance Period ──────────────────────────────────────

pub fn request_forbearance(
    env: Env,
    borrower: Address,
    duration_secs: Option<u64>,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if loan.status != LoanStatus::Active {
        return Err(ContractError::NoActiveLoan);
    }

    let existing: Option<ForbearanceRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Forbearance(loan.id));
    if let Some(ref fb) = existing {
        if fb.status == ForbearanceStatus::Active {
            return Err(ContractError::LoanInForbearance);
        }
    }

    let period_number = existing
        .map(|fb| fb.period_number)
        .unwrap_or(0) + 1;

    if period_number > MAX_FORBEARANCE_PERIODS {
        return Err(ContractError::MaxForbearanceExceeded);
    }

    let now = env.ledger().timestamp();
    let duration = duration_secs.unwrap_or(DEFAULT_FORBEARANCE_DURATION_SECS);
    let ends_at = now + duration;

    loan.deadline = loan.deadline + duration;
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    let forbearance = ForbearanceRecord {
        loan_id: loan.id,
        borrower: borrower.clone(),
        started_at: now,
        duration_secs: duration,
        ends_at,
        original_deadline: loan.deadline - duration,
        period_number,
        status: ForbearanceStatus::Active,
    };

    env.storage()
        .persistent()
        .set(&DataKey::Forbearance(loan.id), &forbearance);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("forbear")),
        (borrower, loan.id, duration, ends_at),
    );

    Ok(())
}

pub fn end_forbearance(env: Env, borrower: Address) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let loan = get_active_loan_record(&env, &borrower)?;

    let mut forbearance: ForbearanceRecord = env
        .storage()
        .persistent()
        .get(&DataKey::Forbearance(loan.id))
        .ok_or(ContractError::ForbearanceNotFound)?;

    if forbearance.status != ForbearanceStatus::Active {
        return Err(ContractError::ForbearanceNotActive);
    }

    let now = env.ledger().timestamp();
    if now >= forbearance.ends_at {
        forbearance.status = ForbearanceStatus::Expired;
    } else {
        forbearance.status = ForbearanceStatus::Ended;
    }

    env.storage()
        .persistent()
        .set(&DataKey::Forbearance(loan.id), &forbearance);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("forb_end")),
        (borrower, loan.id),
    );

    Ok(())
}

pub fn get_forbearance(env: Env, loan_id: u64) -> Option<ForbearanceRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::Forbearance(loan_id))
}

pub fn is_in_forbearance(env: &Env, loan_id: u64) -> bool {
    let fb: Option<ForbearanceRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Forbearance(loan_id));
    match fb {
        Some(fb) => {
            fb.status == ForbearanceStatus::Active
                && env.ledger().timestamp() < fb.ends_at
        }
        None => false,
    }
}

// ── Issue #881: Dynamic Interest Rate based on Risk Score ────────────────────

pub fn get_dynamic_rate_config(env: &Env) -> DynamicRateConfig {
    env.storage()
        .instance()
        .get(&DataKey::DynamicRateConfig)
        .unwrap_or(DEFAULT_DYNAMIC_RATE_CONFIG)
}

pub fn set_dynamic_rate_config(
    env: Env,
    admin_signers: Vec<Address>,
    config: DynamicRateConfig,
) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if config.rate_floor_bps > config.rate_cap_bps {
        return Err(ContractError::InvalidDynamicRateConfig);
    }
    if config.rate_cap_bps > 10_000 {
        return Err(ContractError::InvalidDynamicRateConfig);
    }

    env.storage()
        .instance()
        .set(&DataKey::DynamicRateConfig, &config);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("dyn_cfg")),
        (config.base_rate_bps, config.rate_cap_bps),
    );

    Ok(())
}

pub fn get_dynamic_rate_config_view(env: Env) -> DynamicRateConfig {
    get_dynamic_rate_config(&env)
}

pub fn calculate_dynamic_rate(
    env: &Env,
    borrower: &Address,
    risk_score: u32,
) -> u32 {
    let rate_cfg = get_dynamic_rate_config(env);

    if !rate_cfg.enabled {
        return rate_cfg.base_rate_bps;
    }

    let credit_tier_discount: u32 = match crate::credit_score::get_credit_score(
        env.clone(),
        borrower.clone(),
    ) {
        Some(cs) => {
            let rewards = crate::credit_score::get_tier_rewards(env.clone(), cs.tier);
            if rewards.yield_bonus_bps > 0 {
                rewards.yield_bonus_bps as u32
            } else {
                0
            }
        }
        None => 0,
    };

    let risk_adjustment = risk_score * rate_cfg.risk_adjustment_bps;
    let raw_rate = rate_cfg
        .base_rate_bps
        .saturating_add(risk_adjustment)
        .saturating_sub(credit_tier_discount);

    raw_rate.max(rate_cfg.rate_floor_bps).min(rate_cfg.rate_cap_bps)
}

pub fn compute_and_store_dynamic_rate(
    env: Env,
    admin_signers: Vec<Address>,
    borrower: Address,
) -> Result<u32, ContractError> {
    require_admin_approval(&env, &admin_signers);

    let loan = get_active_loan_record(&env, &borrower)?;
    let risk_score = loan.risk_score;

    let effective_rate = calculate_dynamic_rate(&env, &borrower, risk_score);

    let credit_tier = crate::credit_score::get_credit_score(env.clone(), borrower.clone())
        .map(|cs| cs.tier)
        .unwrap_or(crate::types::CreditTier::Fair);

    let rate_record = BorrowerDynamicRate {
        borrower: borrower.clone(),
        loan_id: loan.id,
        effective_rate_bps: effective_rate,
        risk_score,
        credit_tier,
        computed_at: env.ledger().timestamp(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::BorrowerDynamicRate(borrower.clone()), &rate_record);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("dyn_rate")),
        (borrower, loan.id, effective_rate, risk_score),
    );

    Ok(effective_rate)
}

pub fn get_borrower_dynamic_rate(env: Env, borrower: Address) -> Option<BorrowerDynamicRate> {
    env.storage()
        .persistent()
        .get(&DataKey::BorrowerDynamicRate(borrower))
}

