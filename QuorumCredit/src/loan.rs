use crate::errors::ContractError;
use crate::helpers::{
    bps_of, config, extend_ttl, get_active_loan_record, get_slash_balance, has_active_loan,
    next_loan_id, require_allowed_token, require_not_paused, require_not_paused_for,
    validate_loan_active,
};
use crate::reputation::ReputationNftExternalClient;
use crate::types::{
    DataKey, LoanRecord, LoanStatus, PauseFlag, VouchRecord, DEFAULT_REFERRAL_BONUS_BPS,
    MIN_VOUCH_AGE, SECONDS_PER_DAY, TIME_WEIGHTED_YIELD_BONUS_MULTIPLIER,
    TIME_WEIGHTED_YIELD_BONUS_THRESHOLD_DAYS,
};
use soroban_sdk::{panic_with_error, symbol_short, Address, Env, Vec};

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

/// Backward-compatible request_loan (no co-borrowers).
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
    require_not_paused_for(&env, PauseFlag::LoanRequest)?;
    let empty: Vec<Address> = Vec::new(&env);
    request_loan_internal(env, borrower, amount, threshold, loan_purpose, token_addr, empty)
}

/// Task 3: Request a loan with co-borrowers who share repayment responsibility.
pub fn request_loan_with_co_borrowers(
    env: Env,
    borrower: Address,
    amount: i128,
    threshold: i128,
    loan_purpose: soroban_sdk::String,
    token_addr: Address,
    co_borrowers: Vec<Address>,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;
    require_not_paused_for(&env, PauseFlag::LoanRequest)?;

    for i in 0..co_borrowers.len() {
        let cb = co_borrowers.get(i).unwrap();
        cb.require_auth();
        if cb == borrower {
            return Err(ContractError::SelfVouchNotAllowed);
        }
    }

    request_loan_internal(env, borrower, amount, threshold, loan_purpose, token_addr, co_borrowers)
}

fn request_loan_internal(
    env: Env,
    borrower: Address,
    amount: i128,
    threshold: i128,
    loan_purpose: soroban_sdk::String,
    token_addr: Address,
    co_borrowers: Vec<Address>,
) -> Result<(), ContractError> {
    if env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::Blacklisted(borrower.clone()))
        .unwrap_or(false)
    {
        return Err(ContractError::Blacklisted);
    }

    let whitelist_enabled: bool = env
        .storage()
        .instance()
        .get(&DataKey::BorrowerWhitelistEnabled)
        .unwrap_or(false);
    if whitelist_enabled {
        let whitelisted: bool = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowerWhitelist(borrower.clone()))
            .unwrap_or(false);
        if !whitelisted {
            return Err(ContractError::Blacklisted);
        }
    }

    let token_client = require_allowed_token(&env, &token_addr)?;
    let cfg = config(&env);

    assert!(
        amount >= cfg.min_loan_amount,
        "loan amount must meet minimum threshold"
    );
    assert!(threshold > 0, "threshold must be greater than zero");

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

    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));

    let mut token_vouches: Vec<VouchRecord> = Vec::new(&env);
    for v in vouches.iter() {
        if v.token == token_addr {
            token_vouches.push_back(v);
        }
    }

    let mut total_stake: i128 = 0;
    for v in token_vouches.iter() {
        total_stake = total_stake
            .checked_add(v.amount)
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
    for v in token_vouches.iter() {
        if now < v.vouch_timestamp + MIN_VOUCH_AGE {
            return Err(ContractError::VouchTooRecent);
        }
    }

    let max_allowed_loan = total_stake * cfg.max_loan_to_stake_ratio as i128 / 100;
    assert!(
        amount <= max_allowed_loan,
        "loan amount exceeds maximum collateral ratio"
    );

    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance < amount {
        return Err(ContractError::InsufficientFunds);
    }

    let deadline = now + cfg.loan_duration;
    let loan_id = next_loan_id(&env);
    let yield_bps = env
        .storage()
        .persistent()
        .get::<crate::types::DataKey, crate::types::TokenConfig>(
            &crate::types::DataKey::TokenConfig(token_addr.clone()),
        )
        .map(|tc| tc.yield_bps)
        .unwrap_or(cfg.yield_bps);
    let total_yield = bps_of(amount, yield_bps);

    env.storage().persistent().set(
        &DataKey::Loan(loan_id),
        &LoanRecord {
            id: loan_id,
            borrower: borrower.clone(),
            co_borrowers,
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

pub fn repay(env: Env, borrower: Address, payment: i128) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;
    require_not_paused_for(&env, PauseFlag::Repay)?;

    let mut loan = match get_active_loan_record(&env, &borrower) {
        Ok(loan) => loan,
        Err(ContractError::NoActiveLoan) => {
            if let Some(latest_loan) = crate::helpers::get_latest_loan_record(&env, &borrower) {
                if latest_loan.status == LoanStatus::Repaid {
                    return Err(ContractError::AlreadyRepaid);
                }
            }
            return Err(ContractError::NoActiveLoan);
        }
        Err(e) => return Err(e),
    };

    // Task 3: Allow primary borrower or any co-borrower to repay.
    // The auth was already required above for `borrower`; we just need to
    // verify they are actually associated with this loan.
    let is_primary = borrower == loan.borrower;
    let is_co = loan.co_borrowers.iter().any(|cb| cb == borrower);
    if !is_primary && !is_co {
        return Err(ContractError::UnauthorizedCaller);
    }

    validate_loan_active(&loan)?;
    assert!(
        env.ledger().timestamp() <= loan.deadline,
        "loan deadline has passed"
    );

    let total_owed = loan.amount + loan.total_yield;
    let outstanding = total_owed - loan.amount_repaid;
    if payment <= 0 || payment > outstanding {
        return Err(ContractError::InvalidAmount);
    }

    let token = soroban_sdk::token::Client::new(&env, &loan.token_address);
    token.transfer(&borrower, &env.current_contract_address(), &payment);
    loan.amount_repaid += payment;
    let fully_repaid = loan.amount_repaid >= total_owed;

    if fully_repaid {
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(loan.borrower.clone()))
            .unwrap_or(Vec::new(&env));

        if vouches.is_empty() {
            panic!("no vouchers found for borrower");
        }

        let loan_token = soroban_sdk::token::Client::new(&env, &loan.token_address);
        let _slash_balance = get_slash_balance(&env);

        // Task 2: Time-weighted yield distribution.
        let now = env.ledger().timestamp();
        let mut total_tw: i128 = 0;
        // Collect (voucher, original_stake, adjusted_tw) for token-matching vouches
        let mut tw_entries: Vec<(Address, i128, i128)> = Vec::new(&env);

        for v in vouches.iter() {
            if v.token != loan.token_address {
                continue;
            }
            let days = (now.saturating_sub(v.vouch_timestamp)) / SECONDS_PER_DAY;
            let tw = v.amount * days as i128;
            let multiplier = if days >= TIME_WEIGHTED_YIELD_BONUS_THRESHOLD_DAYS {
                TIME_WEIGHTED_YIELD_BONUS_MULTIPLIER
            } else {
                10
            };
            let adj = tw * multiplier / 10;
            total_tw = total_tw
                .checked_add(adj)
                .ok_or(ContractError::StakeOverflow)?;
            tw_entries.push_back((v.voucher.clone(), v.amount, adj));
        }

        let available_yield = loan.total_yield;
        let mut distributed: i128 = 0;

        for (voucher, stake, adj) in tw_entries.iter() {
            let voucher_yield = if total_tw > 0 {
                (available_yield * adj) / total_tw
            } else {
                0
            };
            distributed += voucher_yield;
            assert!(
                distributed <= available_yield,
                "yield distribution would exceed available funds"
            );
            loan_token.transfer(
                &env.current_contract_address(),
                &voucher,
                &(stake + voucher_yield),
            );
        }

        loan.status = LoanStatus::Repaid;
        loan.repayment_timestamp = Some(env.ledger().timestamp());

        if let Some(referrer) = env
            .storage()
            .persistent()
            .get::<DataKey, Address>(&DataKey::ReferredBy(loan.borrower.clone()))
        {
            let bonus_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ReferralBonusBps)
                .unwrap_or(DEFAULT_REFERRAL_BONUS_BPS);
            let bonus = loan.amount * bonus_bps as i128 / 10_000;
            if bonus > 0 {
                loan_token.transfer(&env.current_contract_address(), &referrer, &bonus);
                env.events().publish(
                    (symbol_short!("referral"), symbol_short!("bonus")),
                    (referrer, loan.borrower.clone(), bonus),
                );
            }
        }

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::RepaymentCount(loan.borrower.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::RepaymentCount(loan.borrower.clone()), &(count + 1));
        extend_ttl(&env, &DataKey::RepaymentCount(loan.borrower.clone()));

        if let Some(nft_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ReputationNft)
        {
            ReputationNftExternalClient::new(&env, &nft_addr).mint(&loan.borrower);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(loan.borrower.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(loan.borrower.clone()));

        env.events().publish(
            (symbol_short!("loan"), symbol_short!("repaid")),
            (loan.borrower.clone(), loan.amount),
        );
    }

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);
    extend_ttl(&env, &DataKey::Loan(loan.id));

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

pub fn get_loan_status(env: Env, loan_id: u64) -> LoanStatus {
    env.storage()
        .persistent()
        .get::<DataKey, LoanRecord>(&DataKey::Loan(loan_id))
        .map(|l| l.status)
        .unwrap_or(LoanStatus::None)
}

pub fn is_eligible(env: Env, borrower: Address, threshold: i128) -> bool {
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
    let total_stake: i128 = vouches.iter().map(|v| v.amount).sum();
    total_stake >= threshold
}

pub fn get_loan_purpose(env: Env, loan_id: u64) -> Option<soroban_sdk::String> {
    env.storage()
        .persistent()
        .get::<DataKey, LoanRecord>(&DataKey::Loan(loan_id))
        .map(|l| l.loan_purpose)
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
