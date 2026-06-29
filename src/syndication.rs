use crate::errors::ContractError;
use crate::helpers::config;
use crate::types::{
    DataKey, LoanSyndication, SyndicationConfig, SyndicationMember, SyndicationRepayment,
    SyndicationRole, SyndicationStatus, DEFAULT_SYNDICATION_CONFIG,
};
use soroban_sdk::{panic_with_error, symbol_short, Address, Env, Vec};

/// Get the syndication configuration, or default if not set.
fn get_syndication_config(env: &Env) -> SyndicationConfig {
    env.storage()
        .instance()
        .get(&DataKey::SyndicationConfig)
        .unwrap_or(DEFAULT_SYNDICATION_CONFIG)
}

/// Create a new loan syndication.
pub fn create_syndication(
    env: Env,
    creator: Address,
    loan_purpose: soroban_sdk::String,
    token_address: Address,
    total_amount: i128,
) -> Result<u64, ContractError> {
    creator.require_auth();
    crate::helpers::require_not_paused(&env)?;
    crate::helpers::check_rate_limit(&env, &creator)?;

    let cfg = get_syndication_config(&env);

    // Validate loan amount
    if total_amount <= 0 {
        return Err(ContractError::LoanBelowMinAmount);
    }
    if total_amount > cfg.max_loan_amount {
        return Err(ContractError::LoanAboveMaxAmount);
    }

    // Generate syndication ID
    let syndication_id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::SyndicationCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .expect("syndication ID overflow");
    env.storage()
        .instance()
        .set(&DataKey::SyndicationCounter, &syndication_id);

    let now = env.ledger().timestamp();
    let min_approvals = (cfg.min_members as u64 * cfg.min_approval_percentage as u64 / 10000) as u32;

    // Create syndication
    let syndication = LoanSyndication {
        syndication_id,
        loan_id: None,
        members: Vec::new(&env),
        total_amount,
        total_collateral: 0,
        total_vouch_stake: 0,
        loan_purpose,
        token_address,
        created_at: now,
        disbursed_at: None,
        status: SyndicationStatus::Forming,
        min_approvals,
        approval_count: 0,
    };

    env.storage()
        .persistent()
        .set(&DataKey::LoanSyndication(syndication_id), &syndication);

    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("created")),
        (syndication_id, creator, total_amount),
    );

    Ok(syndication_id)
}

/// Join a syndication as a member.
pub fn join_syndication(
    env: Env,
    syndication_id: u64,
    member: Address,
    role: SyndicationRole,
    share_bps: u32,
    collateral: i128,
    vouch_stake: i128,
) -> Result<(), ContractError> {
    member.require_auth();
    crate::helpers::require_not_paused(&env)?;
    crate::helpers::check_rate_limit(&env, &member)?;

    let cfg = get_syndication_config(&env);

    // Validate share percentage
    if share_bps == 0 || share_bps > 10000 {
        return Err(ContractError::InvalidSyndicationShare);
    }

    // Get syndication
    let mut syndication: LoanSyndication = env
        .storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
        .ok_or(ContractError::SyndicationNotFound)?;

    // Check status
    if syndication.status != SyndicationStatus::Forming {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    // Check if member already exists
    for existing_member in syndication.members.iter() {
        if existing_member.address == member {
            return Err(ContractError::SyndicationMemberExists);
        }
    }

    // Check max members
    if syndication.members.len() >= cfg.max_members {
        return Err(ContractError::SyndicationMaxMembersExceeded);
    }

    // Check total share doesn't exceed 100%
    let total_share: u32 = syndication.members.iter().map(|m| m.share_bps).sum();
    if total_share + share_bps > 10000 {
        return Err(ContractError::InvalidSyndicationShare);
    }

    let now = env.ledger().timestamp();

    // Create member
    let syndication_member = SyndicationMember {
        address: member.clone(),
        role: role.clone(),
        share_bps,
        collateral,
        vouch_stake,
        approved: false,
        joined_at: now,
    };

    // Store member index before moving into Vec
    env.storage().persistent().set(
        &DataKey::SyndicationMember(syndication_id, member.clone()),
        &syndication_member,
    );

    // Add member to syndication
    syndication.members.push_back(syndication_member);

    // Update totals
    syndication.total_collateral += collateral;
    syndication.total_vouch_stake += vouch_stake;

    // Update syndication
    env.storage()
        .persistent()
        .set(&DataKey::LoanSyndication(syndication_id), &syndication);

    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("joined")),
        (syndication_id, member, role),
    );

    Ok(())
}

/// Approve a syndication (member approval).
pub fn approve_syndication(
    env: Env,
    syndication_id: u64,
    member: Address,
) -> Result<(), ContractError> {
    member.require_auth();
    crate::helpers::require_not_paused(&env)?;

    // Get syndication
    let mut syndication: LoanSyndication = env
        .storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
        .ok_or(ContractError::SyndicationNotFound)?;

    // Check status
    if syndication.status != SyndicationStatus::Forming {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    // Find and update member
    let mut member_found = false;
    for i in 0..syndication.members.len() {
        let existing_member = syndication.members.get(i).unwrap();
        if existing_member.address == member {
            if existing_member.approved {
                return Err(ContractError::AlreadyApproved);
            }

            let mut updated_member = existing_member.clone();
            updated_member.approved = true;
            // Store updated member before moving into Vec
            env.storage()
                .persistent()
                .set(&DataKey::SyndicationMember(syndication_id, member.clone()), &updated_member);
            syndication.members.set(i, updated_member);

            member_found = true;
            syndication.approval_count += 1;
            break;
        }
    }

    if !member_found {
        return Err(ContractError::SyndicationMemberNotFound);
    }

    // Check if syndication is ready
    if syndication.approval_count >= syndication.min_approvals {
        syndication.status = SyndicationStatus::Ready;
    }

    // Update syndication
    env.storage()
        .persistent()
        .set(&DataKey::LoanSyndication(syndication_id), &syndication);

    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("approved")),
        (syndication_id, member, syndication.approval_count),
    );

    Ok(())
}

/// Leave a syndication (only allowed before loan disbursement).
pub fn leave_syndication(
    env: Env,
    syndication_id: u64,
    member: Address,
) -> Result<(), ContractError> {
    member.require_auth();
    crate::helpers::require_not_paused(&env)?;

    let cfg = get_syndication_config(&env);

    // Get syndication
    let mut syndication: LoanSyndication = env
        .storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
        .ok_or(ContractError::SyndicationNotFound)?;

    // Check status
    if syndication.status == SyndicationStatus::Active || syndication.status == SyndicationStatus::Repaid {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    // Find and remove member
    let mut member_found = false;
    let mut member_index = 0;
    for i in 0..syndication.members.len() {
        let existing_member = syndication.members.get(i).unwrap();
        if existing_member.address == member {
            member_index = i;
            member_found = true;

            // Update totals
            syndication.total_collateral -= existing_member.collateral;
            syndication.total_vouch_stake -= existing_member.vouch_stake;

            // Remove member index
            env.storage()
                .persistent()
                .remove(&DataKey::SyndicationMember(syndication_id, member.clone()));

            if existing_member.approved {
                syndication.approval_count -= 1;
            }
            break;
        }
    }

    if !member_found {
        return Err(ContractError::SyndicationMemberNotFound);
    }

    // Remove member from syndication
    syndication.members.remove(member_index);

    // Check min members
    if syndication.members.len() < cfg.min_members {
        syndication.status = SyndicationStatus::Cancelled;
    }

    // Update syndication
    env.storage()
        .persistent()
        .set(&DataKey::LoanSyndication(syndication_id), &syndication);

    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("left")),
        (syndication_id, member),
    );

    Ok(())
}

/// Cancel a syndication (only by lead borrower or before loan disbursement).
pub fn cancel_syndication(
    env: Env,
    syndication_id: u64,
    caller: Address,
) -> Result<(), ContractError> {
    caller.require_auth();
    crate::helpers::require_not_paused(&env)?;

    // Get syndication
    let mut syndication: LoanSyndication = env
        .storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
        .ok_or(ContractError::SyndicationNotFound)?;

    // Check status
    if syndication.status == SyndicationStatus::Active || syndication.status == SyndicationStatus::Repaid {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    // Check if caller is lead borrower
    let is_lead_borrower = syndication.members.iter().any(|m| {
        m.address == caller && m.role == SyndicationRole::LeadBorrower
    });

    if !is_lead_borrower {
        return Err(ContractError::Unauthorized);
    }

    syndication.status = SyndicationStatus::Cancelled;

    // Update syndication
    env.storage()
        .persistent()
        .set(&DataKey::LoanSyndication(syndication_id), &syndication);

    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("cancelled")),
        (syndication_id, caller),
    );

    Ok(())
}

/// Get syndication by ID.
pub fn get_syndication(env: Env, syndication_id: u64) -> Option<LoanSyndication> {
    env.storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
}

/// Get syndication member.
pub fn get_syndication_member(
    env: Env,
    syndication_id: u64,
    member: Address,
) -> Option<SyndicationMember> {
    env.storage()
        .persistent()
        .get(&DataKey::SyndicationMember(syndication_id, member))
}

/// Get syndication configuration.
pub fn get_syndication_config_view(env: Env) -> SyndicationConfig {
    get_syndication_config(&env)
}

/// Set syndication configuration (admin only).
pub fn set_syndication_config(
    env: Env,
    admin_signers: Vec<Address>,
    config: SyndicationConfig,
) -> Result<(), ContractError> {
    crate::helpers::require_admin_approval(&env, &admin_signers);

    // Validate configuration
    if config.min_members < 2 {
        return Err(ContractError::InvalidSyndicationConfig);
    }
    if config.max_members < config.min_members {
        return Err(ContractError::InvalidSyndicationConfig);
    }
    if config.min_approval_percentage < 5000 || config.min_approval_percentage > 10000 {
        return Err(ContractError::InvalidSyndicationConfig);
    }

    env.storage()
        .instance()
        .set(&DataKey::SyndicationConfig, &config);

    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("config")),
        admin_signers.get(0),
    );

    Ok(())
}

/// Get syndication count.
pub fn get_syndication_count(env: Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::SyndicationCounter)
        .unwrap_or(0)
}

/// Request a loan for a syndication (disburse the loan).
pub fn request_syndication_loan(
    env: Env,
    syndication_id: u64,
    lead_borrower: Address,
) -> Result<u64, ContractError> {
    lead_borrower.require_auth();
    crate::helpers::require_not_paused(&env)?;
    crate::helpers::check_rate_limit(&env, &lead_borrower)?;

    let cfg = config(&env);

    // Get syndication
    let mut syndication: LoanSyndication = env
        .storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
        .ok_or(ContractError::SyndicationNotFound)?;

    // Check status
    if syndication.status != SyndicationStatus::Ready {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    // Check if caller is lead borrower
    let is_lead_borrower = syndication.members.iter().any(|m| {
        m.address == lead_borrower && m.role == SyndicationRole::LeadBorrower
    });

    if !is_lead_borrower {
        return Err(ContractError::Unauthorized);
    }

    // Check if syndication already has a loan
    if syndication.loan_id.is_some() {
        return Err(ContractError::SyndicationHasLoan);
    }

    // Validate collateral ratio
    let collateral_ratio = syndication.total_collateral * 100 / syndication.total_amount;
    if collateral_ratio < cfg.max_loan_to_stake_ratio as i128 {
        return Err(ContractError::InsufficientFunds);
    }

    // Check contract balance
    let token_client = crate::helpers::primary_token(&env);
    let contract_balance = token_client.balance(&env.current_contract_address());
    if contract_balance < syndication.total_amount {
        return Err(ContractError::InsufficientFunds);
    }

    // Create loan record
    let loan_id = crate::helpers::next_loan_id(&env);
    let now = env.ledger().timestamp();
    let deadline = now + cfg.loan_duration;

    let loan_record = crate::types::LoanRecord {
        id: loan_id,
        borrower: lead_borrower.clone(),
        co_borrowers: {
            let mut co = Vec::new(&env);
            for m in syndication.members.iter() {
                if m.role == SyndicationRole::CoBorrower {
                    co.push_back(m.address.clone());
                }
            }
            co
        },
        guarantor: syndication
            .members
            .iter()
            .find(|m| m.role == SyndicationRole::Guarantor)
            .map(|m| m.address.clone()),
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: crate::types::EscrowStatus::None,
        amount: syndication.total_amount,
        amount_repaid: 0,
        total_yield: syndication.total_amount * cfg.yield_bps / 10_000,
        status: crate::types::LoanStatus::Active,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: None,
        deadline,
        loan_purpose: syndication.loan_purpose.clone(),
        token_address: syndication.token_address.clone(),
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
    };

    // Store loan
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan_id), &loan_record);

    // Set active loan for lead borrower
    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(lead_borrower.clone()), &loan_id);

    // Set latest loan for lead borrower
    env.storage()
        .persistent()
        .set(&DataKey::LatestLoan(lead_borrower.clone()), &loan_id);

    // Disburse funds to lead borrower
    token_client.transfer(&env.current_contract_address(), &lead_borrower, &syndication.total_amount);

    // Update syndication
    syndication.loan_id = Some(loan_id);
    syndication.disbursed_at = Some(now);
    syndication.status = SyndicationStatus::Active;

    env.storage()
        .persistent()
        .set(&DataKey::LoanSyndication(syndication_id), &syndication);

    // Publish events
    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("disbursed")),
        (syndication_id, loan_id, syndication.total_amount),
    );

    Ok(loan_id)
}

/// Repay a syndication loan (any member can contribute).
pub fn repay_syndication_loan(
    env: Env,
    syndication_id: u64,
    repayer: Address,
    amount: i128,
) -> Result<(), ContractError> {
    repayer.require_auth();
    crate::helpers::require_not_paused(&env)?;
    crate::helpers::check_rate_limit(&env, &repayer)?;

    // Get syndication
    let mut syndication: LoanSyndication = env
        .storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
        .ok_or(ContractError::SyndicationNotFound)?;

    // Check status
    if syndication.status != SyndicationStatus::Active {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    // Get loan ID
    let loan_id = syndication.loan_id.ok_or(ContractError::SyndicationHasLoan)?;

    // Get loan record
    let mut loan: crate::types::LoanRecord = env
        .storage()
        .persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::LoanNotFound)?;

    // Validate repayment amount
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let outstanding = loan.amount + loan.total_yield - loan.amount_repaid;
    let repayment_amount = amount.min(outstanding);

    // Transfer repayment
    let token_client = crate::helpers::primary_token(&env);
    token_client.transfer(&repayer, &env.current_contract_address(), &repayment_amount);

    // Update loan
    loan.amount_repaid += repayment_amount;

    // Record repayment
    let repayment_counter: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::SyndicationRepaymentCounter(syndication_id))
        .unwrap_or(0);

    let repayment_record = SyndicationRepayment {
        syndication_id,
        repayer: repayer.clone(),
        amount: repayment_amount,
        timestamp: env.ledger().timestamp(),
    };

    env.storage().persistent().set(
        &DataKey::SyndicationRepayment(syndication_id, repayment_counter),
        &repayment_record,
    );
    env.storage()
        .persistent()
        .set(&DataKey::SyndicationRepaymentCounter(syndication_id), &(repayment_counter + 1));

    // Check if loan is fully repaid
    if loan.amount_repaid >= loan.amount + loan.total_yield {
        loan.status = crate::types::LoanStatus::Repaid;
        loan.repayment_timestamp = Some(env.ledger().timestamp());
        syndication.status = SyndicationStatus::Repaid;

        // Clear active loan
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(loan.borrower.clone()));

        // Update repayment count for all members
        for member in syndication.members.iter() {
            if member.role == SyndicationRole::LeadBorrower || member.role == SyndicationRole::CoBorrower {
                let current_count: u32 = env
                    .storage()
                    .persistent()
                    .get(&DataKey::RepaymentCount(member.address.clone()))
                    .unwrap_or(0);
                env.storage()
                    .persistent()
                    .set(&DataKey::RepaymentCount(member.address.clone()), &(current_count + 1));
            }
        }
    }

    // Store updated records
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan_id), &loan);
    env.storage()
        .persistent()
        .set(&DataKey::LoanSyndication(syndication_id), &syndication);

    // Publish events
    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("repayment")),
        (syndication_id, repayer, repayment_amount),
    );

    Ok(())
}

/// Handle syndication default (slash collateral from all members).
pub fn handle_syndication_default(
    env: Env,
    syndication_id: u64,
    caller: Address,
) -> Result<(), ContractError> {
    caller.require_auth();
    crate::helpers::require_not_paused(&env)?;

    // Get syndication
    let mut syndication: LoanSyndication = env
        .storage()
        .persistent()
        .get(&DataKey::LoanSyndication(syndication_id))
        .ok_or(ContractError::SyndicationNotFound)?;

    // Check status
    if syndication.status != SyndicationStatus::Active {
        return Err(ContractError::InvalidSyndicationStatus);
    }

    // Get loan ID
    let loan_id = syndication.loan_id.ok_or(ContractError::SyndicationHasLoan)?;

    // Get loan record
    let mut loan: crate::types::LoanRecord = env
        .storage()
        .persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::LoanNotFound)?;

    let cfg = config(&env);
    let now = env.ledger().timestamp();

    // Check if loan is past deadline
    if now < loan.deadline + cfg.grace_period {
        return Err(ContractError::InvalidOperation);
    }

    // Mark loan as defaulted
    loan.status = crate::types::LoanStatus::Defaulted;

    // Slash collateral from all members
    let token_client = crate::helpers::primary_token(&env);
    let slash_treasury = env.storage().instance().get(&DataKey::SlashTreasury).unwrap_or(0);

    for member in syndication.members.iter() {
        if member.collateral > 0 {
            // Transfer collateral to treasury
            token_client.transfer(&env.current_contract_address(), &env.current_contract_address(), &member.collateral);
        }

        // Update default count for borrowers
        if member.role == SyndicationRole::LeadBorrower || member.role == SyndicationRole::CoBorrower {
            let current_count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::DefaultCount(member.address.clone()))
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&DataKey::DefaultCount(member.address.clone()), &(current_count + 1));
        }
    }

    // Update slash treasury
    env.storage()
        .instance()
        .set(&DataKey::SlashTreasury, &(slash_treasury + syndication.total_collateral));

    // Update syndication status
    syndication.status = SyndicationStatus::Defaulted;

    // Clear active loan
    env.storage()
        .persistent()
        .remove(&DataKey::ActiveLoan(loan.borrower.clone()));

    // Store updated records
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan_id), &loan);
    env.storage()
        .persistent()
        .set(&DataKey::LoanSyndication(syndication_id), &syndication);

    // Publish events
    env.events().publish(
        (symbol_short!("syndictn"), symbol_short!("defaulted")),
        (syndication_id, loan_id, syndication.total_collateral),
    );

    Ok(())
}
