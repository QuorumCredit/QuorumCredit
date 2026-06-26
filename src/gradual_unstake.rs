//! # Issue #868: Gradual Unstaking
//!
//! Instead of withdrawing all stake at once, a voucher can schedule a
//! *progressive revocation* that releases stake in equal instalments over a
//! configurable period.  This prevents sudden collateral shocks to a borrower's
//! backing while still guaranteeing the voucher will eventually recover their
//! full stake.
//!
//! ## Flow
//! 1. Voucher calls `start_gradual_unstake` — a `GradualUnstakeSchedule` is
//!    stored and the portion to be released is locked (no immediate transfer).
//! 2. After each `interval_secs` the voucher calls `claim_gradual_instalment`
//!    to receive the next tranche.
//! 3. Once all instalments are paid the schedule is removed and the vouch
//!    record is fully withdrawn.
//!
//! Constraints:
//! - Cannot start while a borrower has an active loan (stake is locked).
//! - Each instalment must wait `interval_secs` since the previous release.
//! - Only one active schedule per (voucher, borrower) pair.

use crate::errors::ContractError;
use crate::helpers::{has_active_loan, require_not_thawing};
use crate::types::{
    DataKey, GradualUnstakeSchedule, VouchRecord,
    DEFAULT_GRADUAL_UNSTAKE_INSTALMENTS, DEFAULT_GRADUAL_UNSTAKE_INTERVAL_SECS,
};
use soroban_sdk::{symbol_short, Address, Env, Vec};

// ── Public API ────────────────────────────────────────────────────────────────

/// Begin a gradual-unstake schedule for the voucher's entire stake on `borrower`.
///
/// `instalments` — number of equal tranches (default if 0: `DEFAULT_GRADUAL_UNSTAKE_INSTALMENTS`).
/// `interval_secs` — seconds between releases (default if 0: `DEFAULT_GRADUAL_UNSTAKE_INTERVAL_SECS`).
///
/// The first instalment becomes claimable immediately (`next_release_at = now`).
pub fn start_gradual_unstake(
    env: Env,
    voucher: Address,
    borrower: Address,
    instalments: u32,
    interval_secs: u64,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_thawing(&env)?;

    // Cannot schedule while the borrower has an active loan — stake is locked.
    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    // Reject duplicate schedule.
    if env
        .storage()
        .persistent()
        .has(&DataKey::GradualUnstake(voucher.clone(), borrower.clone()))
    {
        return Err(ContractError::GradualUnstakeAlreadyActive);
    }

    // Locate the vouch record.
    let vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .ok_or(ContractError::NoVouchesForBorrower)?;

    let idx = vouches
        .iter()
        .position(|v| v.voucher == voucher)
        .ok_or(ContractError::VoucherNotFound)? as u32;

    let vouch_rec = vouches.get(idx).unwrap();
    let total_amount = vouch_rec.stake;

    if total_amount <= 0 {
        return Err(ContractError::InsufficientFunds);
    }

    let n = if instalments == 0 {
        DEFAULT_GRADUAL_UNSTAKE_INSTALMENTS
    } else {
        instalments
    };
    let interval = if interval_secs == 0 {
        DEFAULT_GRADUAL_UNSTAKE_INTERVAL_SECS
    } else {
        interval_secs
    };

    // Integer division; any remainder is added to the final instalment on claim.
    let instalment_amount = total_amount / n as i128;
    if instalment_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let now = env.ledger().timestamp();

    let schedule = GradualUnstakeSchedule {
        voucher: voucher.clone(),
        borrower: borrower.clone(),
        token: vouch_rec.token.clone(),
        total_amount,
        instalment_amount,
        instalments_paid: 0,
        total_instalments: n,
        interval_secs: interval,
        created_at: now,
        next_release_at: now, // first instalment claimable immediately
    };

    env.storage().persistent().set(
        &DataKey::GradualUnstake(voucher.clone(), borrower.clone()),
        &schedule,
    );

    env.events().publish(
        (symbol_short!("unstake"), symbol_short!("start")),
        (voucher, borrower, total_amount, n),
    );

    Ok(())
}

/// Claim the next instalment of a gradual-unstake schedule.
///
/// Transfers `instalment_amount` stroops to the voucher and advances the schedule.
/// The final instalment transfers the remaining balance (handles integer-division remainder).
/// When all instalments are paid, removes the vouch record entirely.
pub fn claim_gradual_instalment(
    env: Env,
    voucher: Address,
    borrower: Address,
) -> Result<i128, ContractError> {
    voucher.require_auth();
    require_not_thawing(&env)?;

    let mut schedule: GradualUnstakeSchedule = env
        .storage()
        .persistent()
        .get(&DataKey::GradualUnstake(voucher.clone(), borrower.clone()))
        .ok_or(ContractError::GradualUnstakeNotFound)?;

    let now = env.ledger().timestamp();

    if now < schedule.next_release_at {
        return Err(ContractError::GradualUnstakeNotDue);
    }

    // Cannot claim while an active loan is ongoing — stake is locked.
    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    let is_last = schedule.instalments_paid + 1 >= schedule.total_instalments;

    // For the last instalment, transfer everything that remains to avoid
    // leaving dust due to integer division.
    let payout = if is_last {
        schedule.total_amount
            - schedule
                .instalment_amount
                .saturating_mul(schedule.instalments_paid as i128)
    } else {
        schedule.instalment_amount
    };

    if payout <= 0 {
        return Err(ContractError::InsufficientFunds);
    }

    // Reduce the vouch's on-chain stake.
    let mut vouches: Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .ok_or(ContractError::NoVouchesForBorrower)?;

    let idx = vouches
        .iter()
        .position(|v| v.voucher == voucher)
        .ok_or(ContractError::VoucherNotFound)? as u32;

    if is_last {
        vouches.remove(idx);
    } else {
        let mut rec = vouches.get(idx).unwrap();
        rec.stake = rec
            .stake
            .checked_sub(payout)
            .ok_or(ContractError::ArithmeticError)?;
        vouches.set(idx, rec);
    }
    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches);

    // Transfer the instalment to the voucher.
    let token_client = crate::helpers::require_allowed_token(&env, &schedule.token)?;
    token_client.transfer(&env.current_contract_address(), &voucher, &payout);

    if is_last {
        // Schedule complete — remove it.
        env.storage()
            .persistent()
            .remove(&DataKey::GradualUnstake(voucher.clone(), borrower.clone()));
    } else {
        // Advance the schedule.
        schedule.instalments_paid += 1;
        schedule.next_release_at = now + schedule.interval_secs;
        env.storage().persistent().set(
            &DataKey::GradualUnstake(voucher.clone(), borrower.clone()),
            &schedule,
        );
    }

    env.events().publish(
        (symbol_short!("unstake"), symbol_short!("claim")),
        (voucher, borrower, payout),
    );

    Ok(payout)
}

/// Cancel an active gradual-unstake schedule and return all remaining stake
/// immediately to the voucher.  Only the schedule owner can cancel.
pub fn cancel_gradual_unstake(
    env: Env,
    voucher: Address,
    borrower: Address,
) -> Result<(), ContractError> {
    voucher.require_auth();
    require_not_thawing(&env)?;

    // Cannot cancel while an active loan is ongoing.
    if has_active_loan(&env, &borrower) {
        return Err(ContractError::ActiveLoanExists);
    }

    let schedule: GradualUnstakeSchedule = env
        .storage()
        .persistent()
        .get(&DataKey::GradualUnstake(voucher.clone(), borrower.clone()))
        .ok_or(ContractError::GradualUnstakeNotFound)?;

    // Compute remaining stake to return.
    let already_paid =
        schedule.instalment_amount.saturating_mul(schedule.instalments_paid as i128);
    let remaining = schedule
        .total_amount
        .checked_sub(already_paid)
        .ok_or(ContractError::ArithmeticError)?;

    if remaining > 0 {
        // Remove the vouch record.
        let mut vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .ok_or(ContractError::NoVouchesForBorrower)?;

        let idx = vouches
            .iter()
            .position(|v| v.voucher == voucher)
            .ok_or(ContractError::VoucherNotFound)? as u32;

        vouches.remove(idx);
        env.storage()
            .persistent()
            .set(&DataKey::Vouches(borrower.clone()), &vouches);

        let token_client = crate::helpers::require_allowed_token(&env, &schedule.token)?;
        token_client.transfer(&env.current_contract_address(), &voucher, &remaining);
    }

    env.storage()
        .persistent()
        .remove(&DataKey::GradualUnstake(voucher.clone(), borrower.clone()));

    env.events().publish(
        (symbol_short!("unstake"), symbol_short!("cancel")),
        (voucher, borrower, remaining),
    );

    Ok(())
}

/// Query the active gradual-unstake schedule for a (voucher, borrower) pair.
pub fn get_gradual_unstake_schedule(
    env: Env,
    voucher: Address,
    borrower: Address,
) -> Option<GradualUnstakeSchedule> {
    env.storage()
        .persistent()
        .get(&DataKey::GradualUnstake(voucher, borrower))
}
