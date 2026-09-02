use crate::errors::ContractError;
use crate::helpers::{
    config, get_loan_by_id, require_admin_approval, require_allowed_token, require_not_paused,
};
use crate::types::{
    DataKey, GuarantorObligation, GuarantorRecord, GuarantorStats, GuaranteeStatus, LoanStatus,
};
use soroban_sdk::{symbol_short, token, Address, Env, Vec};

/// Issue #1172: Request a guarantor for a loan.
/// The guarantor must sign off on their backing commitment.
///
/// #1406: the guarantor's stake is now actually locked here — transferred
/// into the contract in `token` — rather than only being bookkept. Without
/// this, `claim_guarantor_coverage` had nothing real backing the "coverage"
/// it later paid out.
pub fn request_guarantor_for_loan(
    env: Env,
    loan_id: u64,
    guarantor_address: Address,
    guarantee_amount: i128,
    token: Address,
) -> Result<(), ContractError> {
    // Verify the loan exists and is active
    let loan = get_loan_by_id(&env, &loan_id)?;
    require_not_paused(&env)?;

    // Guarantor must be different from borrower
    if guarantor_address == loan.borrower {
        return Err(ContractError::InvalidGuarantor);
    }

    // Validate guarantee amount is positive and doesn't exceed loan amount
    if guarantee_amount <= 0 || guarantee_amount > loan.amount {
        return Err(ContractError::InvalidGuaranteeAmount);
    }

    // Check if a guarantor is already assigned
    if loan.guarantor.is_some() {
        return Err(ContractError::GuarantorAlreadyAssigned);
    }

    // Guarantor must authorize themselves — covers both this invocation and the
    // token transfer below, which the token contract separately requires
    // `guarantor_address.require_auth()` for.
    guarantor_address.require_auth();

    // Lock the guarantor's stake in the contract (#1406). Done before any
    // storage write below so a failed transfer (insufficient balance, no
    // trustline, etc.) leaves no partial guarantor record behind.
    // require_allowed_token guards against an arbitrary/malicious token
    // contract being used to fake a lock (same guard vouch_syndication uses).
    let token_client = require_allowed_token(&env, &token)?;
    token_client.transfer(&guarantor_address, &env.current_contract_address(), &guarantee_amount);

    // Create guarantor record with verified signature
    let guarantor_record = GuarantorRecord {
        loan_id,
        guarantor: guarantor_address.clone(),
        signature_verified: true,
        guarantee_amount,
        token,
        requested_at: env.ledger().timestamp(),
        released_at: None,
        status: GuaranteeStatus::Active,
    };

    // Store the guarantor record
    env.storage()
        .persistent()
        .set(&DataKey::GuarantorRecord(loan_id), &guarantor_record);

    // Create guarantor obligation tracking
    let obligation = GuarantorObligation {
        guarantor: guarantor_address.clone(),
        loan_id,
        borrower: loan.borrower.clone(),
        max_liability: guarantee_amount,
        amount_paid: 0,
        created_at: env.ledger().timestamp(),
        closed_at: None,
    };

    env.storage()
        .persistent()
        .set(&DataKey::GuarantorObligation(guarantor_address.clone(), loan_id), &obligation);

    // Initialize or update guarantor stats
    let mut stats: GuarantorStats = env
        .storage()
        .persistent()
        .get(&DataKey::GuarantorStats(guarantor_address.clone()))
        .unwrap_or(GuarantorStats {
            total_guarantees: 0,
            successful_guarantees: 0,
            triggered_guarantees: 0,
            total_guaranteed: 0,
            total_paid_out: 0,
            reputation_score: 500, // Start with neutral reputation (0-1000 scale)
            last_activity: env.ledger().timestamp(),
        });

    stats.total_guarantees += 1;
    stats.total_guaranteed = stats.total_guaranteed.checked_add(guarantee_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;
    stats.last_activity = env.ledger().timestamp();

    env.storage()
        .persistent()
        .set(&DataKey::GuarantorStats(guarantor_address), &stats);

    Ok(())
}

/// Issue #1172: Release a guarantor from their obligation.
/// Called after loan completion (repaid or defaulted and settled).
pub fn release_guarantor(env: Env, loan_id: u64) -> Result<(), ContractError> {
    require_not_paused(&env)?;

    // Get the guarantor record
    let mut guarantor_record: GuarantorRecord = env
        .storage()
        .persistent()
        .get(&DataKey::GuarantorRecord(loan_id))
        .ok_or(ContractError::GuarantorNotFound)?;

    if guarantor_record.status != GuaranteeStatus::Active {
        return Err(ContractError::InvalidGuaranteeStatus);
    }

    let guarantor_addr = guarantor_record.guarantor.clone();
    let now = env.ledger().timestamp();

    // Update guarantor record status
    guarantor_record.status = GuaranteeStatus::Released;
    guarantor_record.released_at = Some(now);

    env.storage()
        .persistent()
        .set(&DataKey::GuarantorRecord(loan_id), &guarantor_record);

    // #1406: return the locked stake — a release means the guarantee was never
    // triggered, so the collateral request_guarantor_for_loan locked belongs
    // back with the guarantor, not stuck in the contract indefinitely.
    let token_client = token::Client::new(&env, &guarantor_record.token);
    token_client.transfer(
        &env.current_contract_address(),
        &guarantor_addr,
        &guarantor_record.guarantee_amount,
    );

    // Close the obligation
    let mut obligation: GuarantorObligation = env
        .storage()
        .persistent()
        .get(&DataKey::GuarantorObligation(guarantor_addr.clone(), loan_id))
        .ok_or(ContractError::GuarantorNotFound)?;

    obligation.closed_at = Some(now);

    env.storage()
        .persistent()
        .set(
            &DataKey::GuarantorObligation(guarantor_addr.clone(), loan_id),
            &obligation,
        );

    // Update guarantor stats - increment successful guarantees if no payment was required
    let mut stats: GuarantorStats = env
        .storage()
        .persistent()
        .get(&DataKey::GuarantorStats(guarantor_addr.clone()))
        .ok_or(ContractError::GuarantorNotFound)?;

    // If obligation was fulfilled without needing to pay (loan was repaid), mark as successful
    if obligation.amount_paid == 0 {
        stats.successful_guarantees += 1;
        // Increase reputation for successful guarantee
        stats.reputation_score = (stats.reputation_score + 50).min(1000);
    }

    stats.last_activity = now;

    env.storage()
        .persistent()
        .set(&DataKey::GuarantorStats(guarantor_addr), &stats);

    Ok(())
}

/// Issue #1172: Get guarantor record for a loan.
pub fn get_guarantor_record(env: Env, loan_id: u64) -> Result<GuarantorRecord, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::GuarantorRecord(loan_id))
        .ok_or(ContractError::GuarantorNotFound)
}

/// Issue #1172: Get guarantor's stats and reputation.
pub fn get_guarantor_stats(env: Env, guarantor: Address) -> Result<GuarantorStats, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::GuarantorStats(guarantor))
        .ok_or(ContractError::GuarantorNotFound)
}

/// Issue #1172/#1406: Claim guarantor coverage - called when a loan defaults.
/// Transfers the locked guarantee stake from the contract's escrow out to the
/// borrower's current vouchers, pro-rata by stake (the parties actually
/// harmed by the default) — or, if the borrower has no matching-token vouches
/// to compensate, to the borrower directly as a recovery credit.
///
/// Pays out in `guarantor_record.token` — the token actually locked at
/// `request_guarantor_for_loan` time — never a caller-supplied token address,
/// which would let a caller redirect the payout to an arbitrary asset (#1406).
pub fn claim_guarantor_coverage(
    env: Env,
    loan_id: u64,
) -> Result<i128, ContractError> {
    require_not_paused(&env)?;

    let loan = get_loan_by_id(&env, &loan_id)?;
    // #1406: previously nothing checked the loan had actually defaulted —
    // this could be called on a healthy loan to drain the guarantor's stake.
    if loan.status != LoanStatus::Defaulted {
        return Err(ContractError::InvalidStateTransition);
    }

    let mut guarantor_record: GuarantorRecord = env
        .storage()
        .persistent()
        .get(&DataKey::GuarantorRecord(loan_id))
        .ok_or(ContractError::GuarantorNotFound)?;

    if guarantor_record.status == GuaranteeStatus::Claimed {
        return Err(ContractError::GuarantorAlreadyClaimed);
    }

    let guarantor_addr = guarantor_record.guarantor.clone();
    let coverage_amount = guarantor_record.guarantee_amount;
    let coverage_token = guarantor_record.token.clone();

    // Mark guarantee as claimed
    guarantor_record.status = GuaranteeStatus::Claimed;

    env.storage()
        .persistent()
        .set(&DataKey::GuarantorRecord(loan_id), &guarantor_record);

    // Update obligation with payment
    let mut obligation: GuarantorObligation = env
        .storage()
        .persistent()
        .get(&DataKey::GuarantorObligation(guarantor_addr.clone(), loan_id))
        .ok_or(ContractError::GuarantorNotFound)?;

    obligation.amount_paid = coverage_amount;
    obligation.closed_at = Some(env.ledger().timestamp());

    env.storage()
        .persistent()
        .set(
            &DataKey::GuarantorObligation(guarantor_addr.clone(), loan_id),
            &obligation,
        );

    // #1406: actually move the locked collateral. Distribute pro-rata to the
    // borrower's current vouchers denominated in the guarantee's token — the
    // parties whose stake absorbed the default — falling back to the
    // borrower's own recovery credit if there are none to compensate, so the
    // payout is never simply stranded in the contract.
    let token_client = token::Client::new(&env, &coverage_token);
    let vouches: Vec<crate::types::VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(loan.borrower.clone()))
        .unwrap_or(Vec::new(&env));
    let total_stake: i128 = vouches
        .iter()
        .filter(|v| v.token == coverage_token)
        .map(|v| v.stake)
        .sum();

    if total_stake > 0 {
        let mut distributed: i128 = 0;
        for v in vouches.iter() {
            if v.token != coverage_token {
                continue;
            }
            let share = coverage_amount * v.stake / total_stake;
            if share > 0 {
                token_client.transfer(&env.current_contract_address(), &v.voucher, &share);
                distributed += share;
            }
        }
        // Integer-division remainder (unavoidable when stake doesn't divide
        // coverage_amount evenly) goes to the borrower rather than staying
        // trapped in the contract.
        let remainder = coverage_amount - distributed;
        if remainder > 0 {
            token_client.transfer(&env.current_contract_address(), &loan.borrower, &remainder);
        }
    } else {
        token_client.transfer(&env.current_contract_address(), &loan.borrower, &coverage_amount);
    }

    // Update guarantor stats - mark as triggered, decrease reputation
    let mut stats: GuarantorStats = env
        .storage()
        .persistent()
        .get(&DataKey::GuarantorStats(guarantor_addr.clone()))
        .ok_or(ContractError::GuarantorNotFound)?;

    stats.triggered_guarantees += 1;
    stats.total_paid_out = stats.total_paid_out.checked_add(coverage_amount)
        .ok_or(ContractError::ArithmeticOverflow)?;
    // Decrease reputation for triggered guarantee
    stats.reputation_score = stats.reputation_score.saturating_sub(100).max(0);
    stats.last_activity = env.ledger().timestamp();

    env.storage()
        .persistent()
        .set(&DataKey::GuarantorStats(guarantor_addr.clone()), &stats);

    // #1406: event covers the payout, not just the status flip — event
    // indexers/dashboards need to see funds actually moved.
    env.events().publish(
        (symbol_short!("guarantor"), symbol_short!("claimed")),
        (guarantor_addr, loan_id, coverage_token, coverage_amount, loan.borrower),
    );

    Ok(coverage_amount)
}

/// Issue #1172: Get guarantor's reputation impact.
/// Higher reputation guarantors improve loan terms.
pub fn get_guarantor_reputation_multiplier(
    env: Env,
    guarantor: Address,
) -> Result<u32, ContractError> {
    let stats = get_guarantor_stats(env, guarantor)?;

    // Reputation score (0-1000) maps to yield multiplier (100-150 bps)
    // 0 reputation = 100 bps (no bonus)
    // 1000 reputation = 150 bps (50 bps bonus)
    let multiplier_bps = 100 + ((stats.reputation_score as u32 * 50) / 1000);

    Ok(multiplier_bps)
}
