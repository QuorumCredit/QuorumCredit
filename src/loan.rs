use crate::errors::ContractError;
use crate::helpers::{
    config, get_active_loan_record, has_active_loan, next_loan_id, require_allowed_token,
    require_not_paused, require_admin_approval,
};
use crate::reputation::ReputationNftExternalClient;
use crate::types::{
    DataKey, LoanRecord, LoanStatus, VouchRecord, BPS_DENOMINATOR, DEFAULT_REFERRAL_BONUS_BPS,
    AmortizationEntry, SLASH_ESCROW_PERIOD,
};
use soroban_sdk::{panic_with_error, symbol_short, Address, Env, Vec};

/// Calculate dynamic yield in basis points for a borrower.
///
/// Formula: `base_yield_bps + (credit_score / 100) - (default_count * 50)`
/// Result is floored at 0.
///
/// * `credit_score` — reputation NFT balance (0 if no NFT contract configured)
/// * `default_count` — number of past defaults for the borrower
pub fn calculate_dynamic_yield(env: &Env, borrower: &Address) -> i128 {
    let base_bps = config(env).yield_bps;

    let credit_score: i128 = env
        .storage()
        .instance()
        .get::<DataKey, Address>(&DataKey::ReputationNft)
        .map(|nft_addr| ReputationNftExternalClient::new(env, &nft_addr).balance(borrower) as i128)
        .unwrap_or(0);

    let default_count: i128 = env
        .storage()
        .persistent()
        .get::<DataKey, u32>(&DataKey::DefaultCount(borrower.clone()))
        .unwrap_or(0) as i128;

    let dynamic_bps = base_bps + (credit_score / 100) - (default_count * 50);
    dynamic_bps.max(0)
}

/// Register a referrer for a borrower. Must be called before `request_loan`.
/// The referrer cannot be the borrower themselves.
pub fn register_referral(
    env: Env,
    borrower: Address,
    referrer: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    if borrower == referrer {
        panic_with_error!(&env, ContractError::UnauthorizedCaller);
    }
    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }
    // Idempotent: overwrite is fine (borrower signs).
    env.storage()
        .persistent()
        .set(&DataKey::ReferredBy(borrower.clone()), &referrer);

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

/// Request a loan disbursement.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `borrower` - Address of the borrower (must sign)
/// * `amount` - Loan amount, in stroops. Must be ≥ `min_loan_amount`.
///   1 XLM = 10,000,000 stroops.
/// * `threshold` - Minimum total vouched stake required, in stroops.
///   1 XLM = 10,000,000 stroops.
/// * `loan_purpose` - Human-readable description of the loan purpose
/// * `token_addr` - Address of the token contract to use for disbursement
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

    // Validate token is allowed before any other checks.
    let token_client = require_allowed_token(&env, &token_addr)?;

    let cfg = config(&env);

    if amount < cfg.min_loan_amount {
        return Err(ContractError::LoanBelowMinAmount);
    }
    if threshold <= 0 {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let max_loan_amount: i128 = env
        .storage()
        .instance()
        .get(&DataKey::MaxLoanAmount)
        .unwrap_or(0);
    if max_loan_amount > 0 && amount > max_loan_amount {
        return Err(ContractError::LoanExceedsMaxAmount);
    }

    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));

    // Only count vouches denominated in the requested token.
    let mut token_vouches: Vec<VouchRecord> = Vec::new(&env);
    for v in vouches.iter() {
        if v.token == token_addr {
            token_vouches.push_back(v);
        }
    }

    let mut total_stake: i128 = 0;
    for v in token_vouches.iter() {
        total_stake = total_stake
            .checked_add(v.stake)
            .ok_or(ContractError::StakeOverflow)?;
    }
    if total_stake < threshold {
        panic_with_error!(&env, ContractError::InsufficientFunds);
    }

    let min_vouchers: u32 = env
        .storage()
        .instance()
        .get(&DataKey::MinVouchers)
        .unwrap_or(0);
    if token_vouches.len() < min_vouchers {
        return Err(ContractError::InsufficientVouchers);
    }

    let now = env.ledger().timestamp();
    let min_vouch_age = cfg.min_vouch_age_secs;
    for v in token_vouches.iter() {
        if now < v.vouch_timestamp + min_vouch_age {
            return Err(ContractError::VouchTooRecent);
        }
    }

    let max_allowed_loan = total_stake * cfg.max_loan_to_stake_ratio as i128 / 100;
    if amount > max_allowed_loan {
        panic_with_error!(&env, ContractError::LoanExceedsMaxAmount);
    }

    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance < amount {
        return Err(ContractError::InsufficientFunds);
    }

    let deadline = now + cfg.loan_duration;
    let loan_id = next_loan_id(&env);
    let dynamic_yield_bps = calculate_dynamic_yield(&env, &borrower);
    let total_yield = amount * dynamic_yield_bps / 10_000; // stroops

    // Check yield reserve solvency before disbursing
    let yield_reserve: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::YieldReserve)
        .unwrap_or(0);
    if yield_reserve < total_yield {
        return Err(ContractError::InsufficientYieldReserve);
    }

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
            amortization_schedule: Vec::new(&env),
            reminder_sent: false,
            risk_score: 0,
        },
    );
    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(borrower.clone()), &loan_id);
    env.storage()
        .persistent()
        .set(&DataKey::LatestLoan(borrower.clone()), &loan_id);

    let count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCount(borrower.clone()))
        .unwrap_or(0);
    env.storage()
        .persistent()
        .set(&DataKey::LoanCount(borrower.clone()), &(count + 1));

    token_client.transfer(&env.current_contract_address(), &borrower, &amount);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("disbursed")),
        (borrower.clone(), amount, deadline, token_addr),
    );

    Ok(())
}

/// Repay a loan, partially or fully.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `borrower` - Address of the borrower (must sign)
/// * `payment` - Payment amount, in stroops (must be > 0 and ≤ outstanding balance).
///   1 XLM = 10,000,000 stroops.
pub fn repay(env: Env, borrower: Address, payment: i128) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if borrower != loan.borrower {
        return Err(ContractError::UnauthorizedCaller);
    }

    for cb in loan.co_borrowers.iter() {
        cb.require_auth();
    }

    if loan.status != LoanStatus::Active {
        return Err(ContractError::NoActiveLoan);
    }
    if env.ledger().timestamp() > loan.deadline {
        panic_with_error!(&env, ContractError::LoanPastDeadline);
    }

    let total_owed = loan.amount + loan.total_yield;
    let outstanding = total_owed - loan.amount_repaid;
    if payment <= 0 || payment > outstanding {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let token = soroban_sdk::token::Client::new(&env, &loan.token_address);

    // Issue #542: Calculate prepayment penalty if repaying early
    let cfg = config(&env);
    let now = env.ledger().timestamp();
    let time_remaining = if loan.deadline > now {
        loan.deadline - now
    } else {
        0
    };
    
    let mut prepayment_penalty: i128 = 0;
    if time_remaining > 0 && cfg.prepayment_penalty_bps > 0 {
        // Penalty is calculated on the remaining principal
        let remaining_principal = loan.amount - (loan.amount_repaid * loan.amount / total_owed);
        prepayment_penalty = remaining_principal * cfg.prepayment_penalty_bps as i128 / 10_000;
    }

    token.transfer(&borrower, &env.current_contract_address(), &payment);
    loan.amount_repaid += payment;
    let fully_repaid = loan.amount_repaid >= total_owed;

    if fully_repaid {
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        // Issue 112: Only distribute yield to vouches in the same token as the loan.
        let loan_token = soroban_sdk::token::Client::new(&env, &loan.token_address);

        // Issue #367: Collect protocol fee before distributing yield
        let protocol_fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0);
        let protocol_fee = crate::helpers::bps_of(loan.amount, protocol_fee_bps);

        if protocol_fee > 0 {
            if let Some(fee_treasury) = env
                .storage()
                .instance()
                .get::<DataKey, Address>(&DataKey::FeeTreasury)
            {
                loan_token.transfer(
                    &env.current_contract_address(),
                    &fee_treasury,
                    &protocol_fee,
                );
            }
        }

        let mut total_stake: i128 = 0;
        for v in vouches.iter() {
            if v.token == loan.token_address {
                total_stake += v.stake;
            }
        }

        // Issue 112: Ensure yield distribution respects available funds (excluding slash balance)
        // Issue #542: Add prepayment penalty to yield distribution
        let available_for_yield = loan.total_yield + prepayment_penalty;
        let mut total_distributed: i128 = 0;
        
        // Issue #553: Track yield distribution
        let mut yield_distribution: Vec<crate::types::YieldDistributionEntry> = Vec::new(&env);

        for v in vouches.iter() {
            if v.token != loan.token_address {
                continue;
            }
            let voucher_yield = if total_stake > 0 {
                (available_for_yield * v.stake) / total_stake
            } else {
                0
            };
            total_distributed += voucher_yield;

            if total_distributed > available_for_yield {
                panic_with_error!(&env, ContractError::InsufficientFunds);
            }

            // Issue #553: Record yield distribution
            yield_distribution.push_back(crate::types::YieldDistributionEntry {
                voucher: v.voucher.clone(),
                yield_amount: voucher_yield,
            });

            loan_token.transfer(
                &env.current_contract_address(),
                &v.voucher,
                &(v.stake + voucher_yield),
            );
        }

        // Issue #553: Store yield distribution for this loan
        env.storage()
            .persistent()
            .set(&DataKey::YieldDistribution(loan.id), &yield_distribution);

        loan.status = LoanStatus::Repaid;
        loan.repayment_timestamp = Some(env.ledger().timestamp());

        // Pay referral bonus if a referrer is registered.
        if let Some(referrer) = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::ReferredBy(borrower.clone()))
        {
            let bonus_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ReferralBonusBps)
                .unwrap_or(DEFAULT_REFERRAL_BONUS_BPS);
            let bonus = loan.amount * bonus_bps as i128 / BPS_DENOMINATOR;

            // Issue 369: Check contract balance before transferring bonus
            if bonus > 0 {
                let contract_balance = loan_token.balance(&env.current_contract_address());
                if contract_balance >= bonus {
                    loan_token.transfer(&env.current_contract_address(), &referrer, &bonus);
                    env.events().publish(
                        (symbol_short!("referral"), symbol_short!("bonus")),
                        (referrer, borrower.clone(), bonus),
                    );
                }
            }
        }

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RepaymentCount(borrower.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::RepaymentCount(borrower.clone()), &(count + 1));

        if let Some(nft_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ReputationNft)
        {
            ReputationNftExternalClient::new(&env, &nft_addr).mint(&borrower);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(borrower.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));

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

pub fn loan_status(env: Env, borrower: Address) -> LoanStatus {
    match crate::helpers::get_latest_loan_record(&env, &borrower) {
        None => LoanStatus::None,
        Some(loan) => loan.status,
    }
}

pub fn get_loan(env: Env, borrower: Address) -> Option<LoanRecord> {
    crate::helpers::get_latest_loan_record(&env, &borrower)
}

pub fn get_loan_by_id(env: Env, loan_id: u64) -> Option<LoanRecord> {
    env.storage().persistent().get(&DataKey::Loan(loan_id))
}

pub fn is_eligible(env: Env, borrower: Address, threshold: i128, token_addr: Address) -> bool {
    if threshold <= 0 {
        return false;
    }

    if let Some(loan) = crate::helpers::get_latest_loan_record(&env, &borrower) {
        if loan.status == LoanStatus::Active {
            return false;
        }
    }

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower))
        .unwrap_or(Vec::new(&env));

    let total_stake: i128 = vouches
        .iter()
        .filter(|v| v.token == token_addr)
        .map(|v| v.stake)
        .sum();
    total_stake >= threshold
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

/// Emit `repayment_reminder` events for all active loans whose deadline is within 7 days.
///
/// Off-chain systems can listen for these events to notify borrowers.
pub fn emit_repayment_reminders(env: Env) {
    const SEVEN_DAYS: u64 = 7 * 24 * 60 * 60;
    let now = env.ledger().timestamp();
    let counter: u64 = env
        .storage()
        .instance()
        .get(&DataKey::LoanCounter)
        .unwrap_or(0);

    for id in 1..=counter {
        if let Some(loan) = env
            .storage()
            .persistent()
            .get::<DataKey, crate::types::LoanRecord>(&DataKey::Loan(id))
        {
            if loan.status == LoanStatus::Active
                && loan.deadline > now
                && loan.deadline - now <= SEVEN_DAYS
            {
                env.events().publish(
                    (symbol_short!("repay"), symbol_short!("reminder")),
                    (loan.borrower, loan.deadline),
                );
            }
        }
    }
}

/// Add a co-borrower to an active loan. Only the primary borrower can call this.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `borrower` - Primary borrower address (must sign)
/// * `co_borrower` - Address of the co-borrower to add
///
/// # Errors
/// * `NoActiveLoan` — borrower has no active loan
/// * `UnauthorizedCaller` — caller is not the primary borrower
/// * `InvalidAmount` — co-borrower is the same as primary borrower
pub fn add_co_borrower(
    env: Env,
    borrower: Address,
    co_borrower: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    if borrower == co_borrower {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let mut loan = get_active_loan_record(&env, &borrower)?;

    // Check if co-borrower is already in the list
    for cb in loan.co_borrowers.iter() {
        if cb == co_borrower {
            return Err(ContractError::DuplicateVouch);
        }
    }

    loan.co_borrowers.push_back(co_borrower.clone());
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("coborrow")),
        (borrower, co_borrower),
    );

    Ok(())
}

/// Refinance an existing loan with new terms.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `borrower` - Address of the borrower (must sign)
/// * `new_amount` - New loan amount in stroops
/// * `new_threshold` - New minimum stake threshold in stroops
/// * `new_token` - Token contract address for the new loan
///
/// # Errors
/// * `NoActiveLoan` — borrower has no active loan
/// * `UnauthorizedCaller` — caller is not the borrower
/// * `InvalidAmount` — new_amount or new_threshold is not positive
/// * `LoanBelowMinAmount` — new_amount is below minimum
/// * `LoanExceedsMaxAmount` — new_amount exceeds maximum
/// * `InsufficientFunds` — contract has insufficient balance or total stake below threshold
/// * `InvalidToken` — token is not allowed
/// * `ContractPaused` — contract is paused
pub fn refinance_loan(
    env: Env,
    borrower: Address,
    new_amount: i128,
    new_threshold: i128,
    new_token: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let mut old_loan = get_active_loan_record(&env, &borrower)?;

    if borrower != old_loan.borrower {
        return Err(ContractError::UnauthorizedCaller);
    }

    if new_amount <= 0 || new_threshold <= 0 {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let token_client = require_allowed_token(&env, &new_token)?;
    let cfg = config(&env);

    if new_amount < cfg.min_loan_amount {
        return Err(ContractError::LoanBelowMinAmount);
    }

    let max_loan_amount: i128 = env
        .storage()
        .instance()
        .get(&DataKey::MaxLoanAmount)
        .unwrap_or(0);
    if max_loan_amount > 0 && new_amount > max_loan_amount {
        return Err(ContractError::LoanExceedsMaxAmount);
    }

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));

    let mut token_vouches: Vec<VouchRecord> = Vec::new(&env);
    for v in vouches.iter() {
        if v.token == new_token {
            token_vouches.push_back(v);
        }
    }

    let mut total_stake: i128 = 0;
    for v in token_vouches.iter() {
        total_stake = total_stake
            .checked_add(v.stake)
            .ok_or(ContractError::StakeOverflow)?;
    }
    if total_stake < new_threshold {
        panic_with_error!(&env, ContractError::InsufficientFunds);
    }

    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance < new_amount {
        return Err(ContractError::InsufficientFunds);
    }

    // Repay old loan with new loan proceeds
    let old_token = soroban_sdk::token::Client::new(&env, &old_loan.token_address);
    let old_total_owed = old_loan.amount + old_loan.total_yield;
    let old_outstanding = old_total_owed - old_loan.amount_repaid;

    old_token.transfer(
        &env.current_contract_address(),
        &env.current_contract_address(),
        &old_outstanding,
    );

    old_loan.status = LoanStatus::Repaid;
    old_loan.repayment_timestamp = Some(env.ledger().timestamp());
    env.storage()
        .persistent()
        .set(&DataKey::Loan(old_loan.id), &old_loan);

    // Create new loan record
    let now = env.ledger().timestamp();
    let deadline = now + cfg.loan_duration;
    let loan_id = next_loan_id(&env);
    let dynamic_yield_bps = calculate_dynamic_yield(&env, &borrower);
    let total_yield = new_amount * dynamic_yield_bps / 10_000;

    env.storage().persistent().set(
        &DataKey::Loan(loan_id),
        &LoanRecord {
            id: loan_id,
            borrower: borrower.clone(),
            co_borrowers: Vec::new(&env),
            amount: new_amount,
            amount_repaid: 0,
            total_yield,
            status: LoanStatus::Active,
            created_at: now,
            disbursement_timestamp: now,
            repayment_timestamp: None,
            deadline,
            loan_purpose: soroban_sdk::String::from_slice(&env, "refinance"),
            token_address: new_token.clone(),
            amortization_schedule: Vec::new(&env),
            reminder_sent: false,
            risk_score: 0,
        },
    );
    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(borrower.clone()), &loan_id);
    env.storage()
        .persistent()
        .set(&DataKey::LatestLoan(borrower.clone()), &loan_id);

    let count: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCount(borrower.clone()))
        .unwrap_or(0);
    env.storage()
        .persistent()
        .set(&DataKey::LoanCount(borrower.clone()), &(count + 1));

    token_client.transfer(&env.current_contract_address(), &borrower, &new_amount);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("refinance")),
        (borrower.clone(), new_amount, deadline, new_token),
    );

    Ok(())
}

/// Deposit collateral for a borrower. Required for high-risk borrowers (multiple defaults).
///
/// # Arguments
/// * `env` - Soroban environment
/// * `borrower` - Address of the borrower (must sign)
/// * `amount` - Collateral amount in stroops
/// * `token` - Token contract address for collateral
///
/// # Errors
/// * `InvalidAmount` — amount is not positive
/// * `ContractPaused` — contract is paused
pub fn deposit_collateral(
    env: Env,
    borrower: Address,
    amount: i128,
    token: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    if amount <= 0 {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let token_client = require_allowed_token(&env, &token)?;

    let current_collateral: i128 = env
        .storage()
        .persistent()
        .get(&DataKey::BorrowerCollateral(borrower.clone()))
        .unwrap_or(0);

    let new_collateral = current_collateral
        .checked_add(amount)
        .ok_or(ContractError::StakeOverflow)?;

    token_client.transfer(&borrower, &env.current_contract_address(), &amount);

    env.storage()
        .persistent()
        .set(&DataKey::BorrowerCollateral(borrower.clone()), &new_collateral);

    env.events().publish(
        (symbol_short!("coll"), symbol_short!("deposit")),
        (borrower, amount),
    );

    Ok(())
}

/// Get the collateral amount deposited by a borrower.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `borrower` - Address of the borrower
///
/// # Returns
/// * `i128` - Collateral amount in stroops
pub fn get_borrower_collateral(env: Env, borrower: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::BorrowerCollateral(borrower))
        .unwrap_or(0)
}

/// Mint a reputation NFT for a borrower who has successfully repaid at least one loan.
///
/// # Errors
/// * `NoActiveLoan` — borrower has never repaid a loan (repayment_count == 0)
/// * `NoActiveLoan` — no reputation NFT contract is configured
pub fn mint_reputation_nft(env: Env, borrower: Address) -> Result<(), ContractError> {
    borrower.require_auth();

    let repaid: u32 = env
        .storage()
        .persistent()
        .get(&DataKey::RepaymentCount(borrower.clone()))
        .unwrap_or(0);

    if repaid == 0 {
        return Err(ContractError::NoActiveLoan);
    }

    let nft_addr: Address = env
        .storage()
        .instance()
        .get(&DataKey::ReputationNft)
        .ok_or(ContractError::NoActiveLoan)?;

    ReputationNftExternalClient::new(&env, &nft_addr).mint(&borrower);

    env.events().publish(
        (symbol_short!("rep"), symbol_short!("minted")),
        borrower,
    );

    Ok(())
}

/// Get slash audit record for a borrower.
pub fn get_slash_audit(env: Env, borrower: Address) -> Option<crate::types::SlashAuditRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::SlashAudit(borrower))
}

/// Repay loan with partial payment support.
pub fn repay_partial(
    env: Env,
    borrower: Address,
    payment: i128,
    token: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let mut loan = get_active_loan_record(&env, &borrower)?;

    if borrower != loan.borrower {
        return Err(ContractError::UnauthorizedCaller);
    }

    for cb in loan.co_borrowers.iter() {
        cb.require_auth();
    }

    if payment <= 0 {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let outstanding = loan.amount + loan.total_yield - loan.amount_repaid;
    if payment > outstanding {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }

    let loan_token = require_allowed_token(&env, &token)?;
    loan_token.transfer(&env.current_contract_address(), &borrower, &payment);

    loan.amount_repaid = loan.amount_repaid + payment;

    if loan.amount_repaid >= loan.amount + loan.total_yield {
        loan.status = LoanStatus::Repaid;
        loan.repayment_timestamp = Some(env.ledger().timestamp());

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RepaymentCount(borrower.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::RepaymentCount(borrower.clone()), &(count + 1));

        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(borrower.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));
    }

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("part_rep")),
        (borrower.clone(), payment),
    );

    Ok(())
}


/// Send a repayment reminder for a loan. Anyone can call this.
pub fn send_repayment_reminder(env: Env, loan_id: u64) -> Result<(), ContractError> {
    let mut loan: LoanRecord = env
        .storage()
        .persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::NoActiveLoan)?;

    if loan.status != LoanStatus::Active {
        return Err(ContractError::InvalidStateTransition);
    }

    if loan.reminder_sent {
        return Err(ContractError::ReminderAlreadySent);
    }

    loan.reminder_sent = true;
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan_id), &loan);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("reminder")),
        (loan.borrower.clone(), loan.deadline),
    );

    Ok(())
}

/// Get the yield reserve balance.
pub fn get_yield_reserve_balance(env: Env) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::YieldReserve)
        .unwrap_or(0)
}

/// Release slashed funds from escrow after the escrow period expires.
/// Admin-only function.
pub fn release_slash_escrow(env: Env, admin_signers: Vec<Address>, borrower: Address) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    let escrow_data: Option<(i128, u64)> = env
        .storage()
        .persistent()
        .get(&DataKey::SlashEscrow(borrower.clone()));

    let (amount, release_timestamp) = escrow_data.ok_or(ContractError::NoActiveLoan)?;

    let now = env.ledger().timestamp();
    if now < release_timestamp {
        return Err(ContractError::InvalidStateTransition);
    }

    env.storage()
        .persistent()
        .remove(&DataKey::SlashEscrow(borrower.clone()));

    env.events().publish(
        (symbol_short!("slash"), symbol_short!("esc_rel")),
        (borrower.clone(), amount),
    );

    Ok(())
}

/// Set the yield reserve balance. Admin-only.
pub fn set_yield_reserve(env: Env, admin_signers: Vec<Address>, amount: i128) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if amount < 0 {
        return Err(ContractError::InvalidAmount);
    }

    env.storage()
        .persistent()
        .set(&DataKey::YieldReserve, &amount);

    env.events().publish(
        (symbol_short!("yield"), symbol_short!("rsrv_set")),
        amount,
    );

    Ok(())
}

/// Set the risk score for a borrower. Admin-only.
pub fn set_borrower_risk_score(env: Env, admin_signers: Vec<Address>, borrower: Address, risk_score: u32) -> Result<(), ContractError> {
    require_admin_approval(&env, &admin_signers);

    if risk_score > 100 {
        return Err(ContractError::InvalidAmount);
    }

    let mut loan: LoanRecord = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveLoan(borrower.clone()))
        .and_then(|loan_id| env.storage().persistent().get(&DataKey::Loan(loan_id)))
        .ok_or(ContractError::NoActiveLoan)?;

    loan.risk_score = risk_score;
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);

    env.events().publish(
        (symbol_short!("borrower"), symbol_short!("risk_set")),
        (borrower.clone(), risk_score),
    );

    Ok(())
}
