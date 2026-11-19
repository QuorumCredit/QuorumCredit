//! Issue #1169: Conditional vouch release on loan performance milestones.
//!
//! Instead of a voucher's stake being locked in full for the life of the
//! loan, it is released in 25% increments as the loan hits performance
//! milestones (issued, first payment, half repaid, completed). Reaching a
//! milestone in the first half of the loan's term earns a small early-release
//! bonus, funded out of the voucher's own remaining stake so no external
//! funds are required.

use soroban_sdk::{symbol_short, Address, Env, Vec};

use crate::errors::ContractError;
use crate::helpers::{require_allowed_token, require_not_thawing};
use crate::types::{DataKey, LoanMilestone, LoanRecord, VouchRecord};
use crate::vouch::invalidate_weighted_stake_cache;

const ALL_MILESTONES: [LoanMilestone; 4] = [
    LoanMilestone::Issued,
    LoanMilestone::FirstPaymentMade,
    LoanMilestone::HalfRepaid,
    LoanMilestone::Completed,
];

/// Early-release bonus, in basis points of the milestone's release amount,
/// paid (out of the voucher's own remaining stake) when the milestone is hit
/// before the halfway point of the loan's [disbursement, deadline] window.
const EARLY_BONUS_BPS: u32 = 500; // 5%

/// Issue #2118: every release must be backed by a real repayment milestone.
/// `Issued` fires at disbursement, before any repayment, so it never unlocks
/// stake while the loan is outstanding.
fn milestone_reached(loan: &LoanRecord, milestone: LoanMilestone) -> bool {
    match milestone {
        LoanMilestone::Issued => false,
        LoanMilestone::FirstPaymentMade => loan.amount_repaid > 0,
        LoanMilestone::HalfRepaid => loan.amount > 0 && loan.amount_repaid >= loan.amount / 2,
        LoanMilestone::Completed => loan.repaid,
    }
}

fn is_early(env: &Env, loan: &LoanRecord) -> bool {
    if loan.deadline <= loan.disbursement_timestamp {
        return false;
    }
    let now = env.ledger().timestamp();
    let midpoint = loan.disbursement_timestamp + (loan.deadline - loan.disbursement_timestamp) / 2;
    now <= midpoint
}

/// Total amount already released to `voucher` for `loan_id` across all
/// milestones so far.
fn total_released(env: &Env, loan_id: u64, voucher: &Address) -> i128 {
    let mut total: i128 = 0;
    for milestone in ALL_MILESTONES.iter() {
        if let Some(amount) = env.storage().persistent().get::<DataKey, i128>(
            &DataKey::VouchMilestoneRelease(loan_id, voucher.clone(), milestone.index()),
        ) {
            total += amount;
        }
    }
    total
}

/// Release the portion of `voucher`'s stake unlocked by reaching `milestone`
/// on `loan_id`. Each milestone releases its share of the voucher's original
/// stake (see `LoanMilestone::release_bps`), plus an early-completion bonus
/// drawn from their own remaining stake. Refused when the release would leave
/// the borrower's remaining stake below the loan's outstanding principal.
pub fn release_vouch_at_milestone(
    env: Env,
    loan_id: u64,
    voucher: Address,
    milestone: LoanMilestone,
) -> Result<i128, ContractError> {
    voucher.require_auth();
    require_not_thawing(&env)?;

    let loan: LoanRecord = env
        .storage()
        .persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::NoActiveLoan)?;

    if !milestone_reached(&loan, milestone) {
        return Err(ContractError::MilestoneNotReached);
    }

    if env.storage().persistent().has(&DataKey::VouchMilestoneRelease(
        loan_id,
        voucher.clone(),
        milestone.index(),
    )) {
        return Err(ContractError::MilestoneAlreadyReleased);
    }

    let borrower = loan.borrower.clone();
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
    let current_stake = vouch_rec.stake;

    let original_stake = current_stake
        .checked_add(total_released(&env, loan_id, &voucher))
        .ok_or(ContractError::StakeOverflow)?;

    let mut release_amount = (original_stake * milestone.release_bps() as i128) / 10_000;
    if is_early(&env, &loan) {
        let bonus = (release_amount * EARLY_BONUS_BPS as i128) / 10_000;
        release_amount = release_amount
            .checked_add(bonus)
            .ok_or(ContractError::StakeOverflow)?;
    }
    if release_amount > current_stake {
        release_amount = current_stake;
    }
    if release_amount <= 0 {
        return Ok(0);
    }

    // Issue #2118: after the release, the borrower's remaining stake in this
    // token must still cover the loan's outstanding principal. Otherwise the
    // release would leave the loan under-collateralised.
    let new_stake = current_stake - release_amount;
    let mut remaining_stake: i128 = new_stake;
    for (i, v) in vouches.iter().enumerate() {
        if i as u32 != idx && v.token == vouch_rec.token {
            remaining_stake = remaining_stake.saturating_add(v.stake);
        }
    }
    let outstanding = (loan.amount - loan.amount_repaid).max(0);
    if remaining_stake < outstanding {
        return Err(ContractError::MinStakeNotMet);
    }

    let token_client = require_allowed_token(&env, &vouch_rec.token)?;
    let mut vouches_mut = vouches;
    if new_stake == 0 {
        vouches_mut.remove(idx);
    } else {
        let mut updated = vouch_rec.clone();
        updated.stake = new_stake;
        vouches_mut.set(idx, updated);
    }
    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches_mut);
    invalidate_weighted_stake_cache(&env, &borrower, &vouch_rec.token);

    token_client.transfer(&env.current_contract_address(), &voucher, &release_amount);

    env.storage().persistent().set(
        &DataKey::VouchMilestoneRelease(loan_id, voucher.clone(), milestone.index()),
        &release_amount,
    );
    if !env
        .storage()
        .persistent()
        .has(&DataKey::MilestoneAchieved(loan_id, milestone.index()))
    {
        env.storage().persistent().set(
            &DataKey::MilestoneAchieved(loan_id, milestone.index()),
            &env.ledger().timestamp(),
        );
    }

    env.events().publish(
        (symbol_short!("milestone"), symbol_short!("released")),
        (loan_id, voucher, release_amount),
    );

    Ok(release_amount)
}

pub fn get_milestone_achieved(env: Env, loan_id: u64, milestone: LoanMilestone) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&DataKey::MilestoneAchieved(loan_id, milestone.index()))
}

pub fn get_milestone_release(
    env: Env,
    loan_id: u64,
    voucher: Address,
    milestone: LoanMilestone,
) -> Option<i128> {
    env.storage().persistent().get(&DataKey::VouchMilestoneRelease(
        loan_id,
        voucher,
        milestone.index(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{release_vouch_at_milestone, ALL_MILESTONES};
    use crate::errors::ContractError;
    use crate::types::{DataKey, LoanMilestone, LoanRecord, VouchRecord};
    use crate::QuorumCreditContract;
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    /// Build a loan with a 500..2000 term (midpoint 1250, so ledger time
    /// 1000 counts as an early milestone).
    fn seed_loan(env: &Env, contract_id: &Address, borrower: &Address, token: &Address, amount: i128, repaid: i128, stake: i128, voucher: &Address) {
        env.as_contract(contract_id, || {
            let loan = LoanRecord {
                id: 1,
                borrower: borrower.clone(),
                co_borrowers: Vec::new(env),
                amount,
                amount_repaid: repaid,
                total_yield: 0,
                repaid: false,
                defaulted: false,
                created_at: 500,
                disbursement_timestamp: 500,
                repayment_timestamp: None,
                deadline: 2_000,
                loan_purpose: soroban_sdk::String::from_str(env, "test"),
                token_address: token.clone(),
            };
            env.storage().persistent().set(&DataKey::Loan(1), &loan);
            let mut vouches: Vec<VouchRecord> = Vec::new(env);
            vouches.push_back(VouchRecord {
                voucher: voucher.clone(),
                stake,
                vouch_timestamp: 400,
                token: token.clone(),
            });
            env.storage()
                .persistent()
                .set(&DataKey::Vouches(borrower.clone()), &vouches);
        });
    }

    /// Issue #2118: `Issued` is a disbursement event, so a voucher must not be
    /// able to pull stake out while the loan is outstanding.
    #[test]
    fn test_issued_milestone_cannot_release_stake() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let contract_id = env.register(QuorumCreditContract, ());
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);
        let token = Address::generate(&env);
        seed_loan(&env, &contract_id, &borrower, &token, 1_000, 0, 1_000, &voucher);

        let res = env.as_contract(&contract_id, || {
            release_vouch_at_milestone(env.clone(), 1, voucher.clone(), LoanMilestone::Issued)
        });
        assert!(matches!(res, Err(ContractError::MilestoneNotReached)));
    }

    /// Issue #2118: the per-milestone schedule must never release more than the
    /// voucher's full stake, and `Issued` releases nothing.
    #[test]
    fn test_release_schedule_sums_to_at_most_full_stake() {
        assert_eq!(LoanMilestone::Issued.release_bps(), 0);
        let total: u32 = ALL_MILESTONES.iter().map(|m| m.release_bps()).sum();
        assert!(total <= 10_000);
        assert_eq!(total, 10_000);
    }

    /// Issue #2118: a release that leaves the borrower's remaining stake below
    /// the loan's outstanding principal must be refused.
    #[test]
    fn test_release_refused_when_it_breaches_loan_to_stake_ratio() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let contract_id = env.register(QuorumCreditContract, ());
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);
        let token = Address::generate(&env);
        // Outstanding = 1_000 - 100 = 900. Stake 1_000 minus a 25% release
        // (plus the 5% early bonus) leaves less than 900.
        seed_loan(&env, &contract_id, &borrower, &token, 1_000, 100, 1_000, &voucher);

        let res = env.as_contract(&contract_id, || {
            release_vouch_at_milestone(
                env.clone(),
                1,
                voucher.clone(),
                LoanMilestone::FirstPaymentMade,
            )
        });
        assert!(matches!(res, Err(ContractError::MinStakeNotMet)));

        // The stake must be untouched after a refused release.
        let stake = env.as_contract(&contract_id, || {
            let v: Vec<VouchRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::Vouches(borrower.clone()))
                .unwrap();
            v.get(0).unwrap().stake
        });
        assert_eq!(stake, 1_000);
    }
}
