use crate::errors::ContractError;
use crate::helpers::{
    bps_of, config, extend_ttl, get_active_loan_record, get_slash_balance, has_active_loan,
    next_loan_id, require_allowed_token, require_not_paused, validate_loan_active,
};
use crate::reputation::ReputationNftExternalClient;
use crate::types::{
    DataKey, LoanCategory, LoanRecord, LoanStatus, VouchRecord, DEFAULT_REFERRAL_BONUS_BPS,
    MIN_VOUCH_AGE, CANCELLATION_WINDOW_SECONDS,
};
use soroban_sdk::{panic_with_error, symbol_short, Address, Env, Vec};

/// Register a referrer for a borrower. Must be called before `request_loan`.
/// The referrer cannot be the borrower themselves.
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
    // Idempotent: overwrite is fine (borrower signs).
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

    // Borrower whitelist check: if enabled, borrower must be whitelisted.
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

    // Validate token is allowed before any other checks.
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

    // Default to Personal category if not specified
    let loan_category = LoanCategory::Personal;

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
            loan_category: loan_category.clone(),
            token_address: token_addr.clone(),
        },
    );
    extend_ttl(&env, &DataKey::Loan(loan_id));

    // Task 4: Track loan by category
    let mut category_loans: Vec<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCategoryLoans(loan_category.clone()))
        .unwrap_or(Vec::new(&env));
    category_loans.push_back(loan_id);
    env.storage()
        .persistent()
        .set(&DataKey::LoanCategoryLoans(loan_category.clone()), &category_loans);
    extend_ttl(&env, &DataKey::LoanCategoryLoans(loan_category));
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

    // First try to get active loan record
    let mut loan = match get_active_loan_record(&env, &borrower) {
        Ok(loan) => loan,
        Err(ContractError::NoActiveLoan) => {
            // Check if there's a latest loan that is already repaid
            if let Some(latest_loan) = crate::helpers::get_latest_loan_record(&env, &borrower) {
                if latest_loan.status == LoanStatus::Repaid {
                    return Err(ContractError::AlreadyRepaid);
                }
            }
            return Err(ContractError::NoActiveLoan);
        }
        Err(e) => return Err(e),
    };

    for cb in loan.co_borrowers.iter() {
        cb.require_auth();
    }

    if borrower != loan.borrower {
        return Err(ContractError::UnauthorizedCaller);
    }
    validate_loan_active(&loan)?;
    assert!(
        env.ledger().timestamp() <= loan.deadline,
        "loan deadline has passed"
    );

    // Total obligation = principal + yield locked in at disbursement.
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
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        if vouches.is_empty() {
            panic!("no vouchers found for borrower");
        }

        // Issue 112: Only distribute yield to vouches in the same token as the loan.
        // Verify that available funds exclude slash balance to prevent fund leakage.
        let loan_token = soroban_sdk::token::Client::new(&env, &loan.token_address);
        let _slash_balance = get_slash_balance(&env);

        let mut total_stake: i128 = 0;
        for v in vouches.iter() {
            if v.token == loan.token_address {
                total_stake += v.amount;
            }
        }

        // Issue 112: Ensure yield distribution respects available funds (excluding slash balance)
        let available_for_yield = loan.total_yield;
        let mut total_distributed: i128 = 0;

        for v in vouches.iter() {
            if v.token != loan.token_address {
                continue;
            }
            let voucher_yield = if total_stake > 0 {
                (available_for_yield * v.amount) / total_stake
            } else {
                0
            };
            total_distributed += voucher_yield;

            // Assert that we're not exceeding available yield
            assert!(
                total_distributed <= available_for_yield,
                "yield distribution would exceed available funds"
            );

            loan_token.transfer(
                &env.current_contract_address(),
                &v.voucher,
                &(v.amount + voucher_yield),
            );
        }

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
            let bonus = loan.amount * bonus_bps as i128 / 10_000;

            // Issue 112: Ensure bonus doesn't use slash funds
            if bonus > 0 {
                loan_token.transfer(&env.current_contract_address(), &referrer, &bonus);
                env.events().publish(
                    (symbol_short!("referral"), symbol_short!("bonus")),
                    (referrer, borrower.clone(), bonus),
                );
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
        extend_ttl(&env, &DataKey::RepaymentCount(borrower.clone()));

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

// Task 1: Loan Cancellation - Allow borrower to cancel loan before disbursement
pub fn cancel_loan(env: Env, borrower: Address) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    let loan = get_active_loan_record(&env, &borrower)?;

    // Only allow cancellation if loan is Pending or within cancellation window of Active
    let now = env.ledger().timestamp();
    match loan.status {
        LoanStatus::Pending => {
            // Can always cancel pending loans
        }
        LoanStatus::Active => {
            // Can only cancel within 1 hour of disbursement
            if now > loan.disbursement_timestamp + CANCELLATION_WINDOW_SECONDS {
                return Err(ContractError::CancellationWindowExpired);
            }
        }
        _ => {
            return Err(ContractError::LoanNotCancellable);
        }
    }

    // Return all voucher stakes immediately
    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .unwrap_or(Vec::new(&env));

    for v in vouches.iter() {
        let token_client = soroban_sdk::token::Client::new(&env, &v.token);
        token_client.transfer(&env.current_contract_address(), &v.voucher, &v.amount);
    }

    // Remove vouches and loan records
    env.storage()
        .persistent()
        .remove(&DataKey::Vouches(borrower.clone()));
    env.storage()
        .persistent()
        .remove(&DataKey::ActiveLoan(borrower.clone()));

    // Mark loan as cancelled
    let mut updated_loan = loan.clone();
    updated_loan.status = LoanStatus::Cancelled;
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &updated_loan);
    extend_ttl(&env, &DataKey::Loan(loan.id));

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("cancelled")),
        (borrower, loan.amount),
    );

    Ok(())
}

// Task 4: Loan Category Analytics - Get all loan IDs by category
pub fn get_loans_by_category(env: Env, category: LoanCategory) -> Vec<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::LoanCategoryLoans(category))
        .unwrap_or(Vec::new(&env))
}

// Task 2: Large Loan Multi-Signature - Require admin approval for large loans
pub fn request_large_loan(
    env: Env,
    borrower: Address,
    amount: i128,
    threshold: i128,
    loan_purpose: soroban_sdk::String,
    loan_category: LoanCategory,
    token_addr: Address,
) -> Result<(), ContractError> {
    borrower.require_auth();
    require_not_paused(&env)?;

    // Check if loan amount exceeds large loan threshold
    let large_loan_threshold = crate::types::LARGE_LOAN_THRESHOLD;
    if amount <= large_loan_threshold {
        return Err(ContractError::LoanTooLarge);
    }

    // Check borrower doesn't already have an active loan
    assert!(
        !has_active_loan(&env, &borrower),
        "borrower already has an active loan"
    );

    // Validate token is allowed
    let _token_client = require_allowed_token(&env, &token_addr)?;

    // Store the large loan request
    let request_record = crate::types::LargeLoanRequestRecord {
        borrower: borrower.clone(),
        amount,
        requested_at: env.ledger().timestamp(),
        token_address: token_addr.clone(),
        threshold,
        loan_purpose: loan_purpose.clone(),
        loan_category,
    };

    env.storage()
        .persistent()
        .set(&DataKey::LargeLoanRequest(borrower.clone()), &request_record);
    extend_ttl(&env, &DataKey::LargeLoanRequest(borrower.clone()));

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("large_req")),
        (borrower, amount, token_addr),
    );

    Ok(())
}

pub fn approve_large_loan(
    env: Env,
    admin: Address,
    borrower: Address,
) -> Result<(), ContractError> {
    admin.require_auth();

    // Verify admin is in the admin list
    let cfg = config(&env);
    let is_admin = cfg.admins.iter().any(|a| a == admin);
    assert!(is_admin, "caller is not an admin");

    // Get the large loan request
    let request: crate::types::LargeLoanRequestRecord = env
        .storage()
        .persistent()
        .get(&DataKey::LargeLoanRequest(borrower.clone()))
        .ok_or(ContractError::LargeLoanNotApproved)?;

    // Check if delay has elapsed
    let now = env.ledger().timestamp();
    let delay_elapsed = now >= request.requested_at + crate::types::LARGE_LOAN_DELAY_SECONDS;
    assert!(delay_elapsed, "large loan delay has not elapsed");

    // Update or create approval record
    let approval_key = DataKey::LargeLoanApproval(borrower.clone());
    let mut approval: crate::types::LargeLoanApprovalRecord = env
        .storage()
        .persistent()
        .get(&approval_key)
        .unwrap_or(crate::types::LargeLoanApprovalRecord {
            borrower: borrower.clone(),
            amount: request.amount,
            approved_by: Vec::new(&env),
            approval_timestamp: now,
            executed: false,
        });

    // Check admin hasn't already approved
    assert!(
        !approval.approved_by.iter().any(|a| a == admin),
        "admin already approved this loan"
    );

    // Add admin to approval list
    approval.approved_by.push_back(admin.clone());
    approval.approval_timestamp = now;

    env.storage()
        .persistent()
        .set(&approval_key, &approval);
    extend_ttl(&env, &approval_key);

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("lg_appr")),
        (borrower, request.amount, admin),
    );

    Ok(())
}

pub fn execute_large_loan(env: Env, borrower: Address) -> Result<(), ContractError> {
    borrower.require_auth();

    // Get the large loan request
    let request: crate::types::LargeLoanRequestRecord = env
        .storage()
        .persistent()
        .get(&DataKey::LargeLoanRequest(borrower.clone()))
        .ok_or(ContractError::LargeLoanNotApproved)?;

    // Get the approval record
    let approval: crate::types::LargeLoanApprovalRecord = env
        .storage()
        .persistent()
        .get(&DataKey::LargeLoanApproval(borrower.clone()))
        .ok_or(ContractError::LargeLoanNotApproved)?;

    // Check if already executed
    assert!(!approval.executed, "large loan already executed");

    // Verify admin threshold is met
    let cfg = config(&env);
    assert!(
        approval.approved_by.len() as u32 >= cfg.admin_threshold,
        "insufficient admin approvals"
    );

    // Now execute the loan (similar to request_loan but for large loans)
    let now = env.ledger().timestamp();
    let deadline = now + cfg.loan_duration;
    let loan_id = next_loan_id(&env);
    let yield_bps = env
        .storage()
        .persistent()
        .get::<crate::types::DataKey, crate::types::TokenConfig>(
            &crate::types::DataKey::TokenConfig(request.token_address.clone()),
        )
        .map(|tc| tc.yield_bps)
        .unwrap_or(cfg.yield_bps);
    let total_yield = bps_of(request.amount, yield_bps);

    env.storage().persistent().set(
        &DataKey::Loan(loan_id),
        &LoanRecord {
            id: loan_id,
            borrower: borrower.clone(),
            co_borrowers: Vec::new(&env),
            amount: request.amount,
            amount_repaid: 0,
            total_yield,
            status: LoanStatus::Active,
            created_at: now,
            disbursement_timestamp: now,
            repayment_timestamp: None,
            deadline,
            loan_purpose: request.loan_purpose,
            loan_category: request.loan_category,
            token_address: request.token_address.clone(),
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

    // Disburse funds
    let token_client = soroban_sdk::token::Client::new(&env, &request.token_address);
    token_client.transfer(&env.current_contract_address(), &borrower, &request.amount);

    // Mark approval as executed
    let mut updated_approval = approval;
    updated_approval.executed = true;
    env.storage()
        .persistent()
        .set(&DataKey::LargeLoanApproval(borrower.clone()), &updated_approval);

    // Remove the request
    env.storage()
        .persistent()
        .remove(&DataKey::LargeLoanRequest(borrower.clone()));

    env.events().publish(
        (symbol_short!("loan"), symbol_short!("lg_exec")),
        (borrower, request.amount),
    );

    Ok(())
}
