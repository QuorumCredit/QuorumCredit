use crate::errors::ContractError;
use crate::helpers::{config, require_not_paused};
use crate::types::{
    BondInsuranceRecord, BondStats, BondStatus, DataKey, InsuranceStatus, VouchProtectionBond,
};
use soroban_sdk::{Address, Env};

/// Issue #1175: Bond insurance premium rate (3% = 300 basis points)
const BOND_INSURANCE_PREMIUM_BPS: i128 = 300;
/// Issue #1175: Maximum bond coverage as percentage of vouch stake (50%)
const MAX_BOND_COVERAGE_BPS: i128 = 5000;
/// Issue #1427: Term (in seconds) the insurance premium is priced to cover.
/// Releasing a bond before this term elapses earns a pro-rated premium refund
/// (time-remaining / full-term), provided no insurance claim was ever paid.
/// 30 days, matching the default loan window.
const BOND_INSURANCE_FULL_TERM_SECS: u64 = 30 * 24 * 60 * 60;

/// Issue #1427: Pro-rate an insurance premium refund by unused time.
///
/// `refund = premium * (full_term - time_held) / full_term`, clamped to
/// `[0, premium]`. An immediate release refunds the whole premium; a release
/// at or after the full term refunds nothing.
fn prorated_premium_refund(premium_paid: i128, created_at: u64, released_at: u64) -> i128 {
    if premium_paid <= 0 {
        return 0;
    }
    let time_held = released_at.saturating_sub(created_at);
    if time_held >= BOND_INSURANCE_FULL_TERM_SECS {
        return 0;
    }
    let time_remaining = (BOND_INSURANCE_FULL_TERM_SECS - time_held) as i128;
    let refund = (premium_paid * time_remaining) / (BOND_INSURANCE_FULL_TERM_SECS as i128);
    refund.clamp(0, premium_paid)
}

/// Issue #1175: Stake a bond for vouch protection.
/// The bond covers up to 50% of the vouch amount against slashing.
pub fn stake_bond_for_vouch_protection(
    env: Env,
    loan_id: u64,
    vouch_id: u64,
    voucher: Address,
    protected_stake: i128,
    bond_amount: i128,
) -> Result<(), ContractError> {
    require_not_paused(&env)?;

    // Validate bond amount
    if bond_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Bond can cover up to 50% of the vouch stake
    let max_bond = (protected_stake * MAX_BOND_COVERAGE_BPS) / 10_000;
    if bond_amount > max_bond {
        return Err(ContractError::InvalidAmount);
    }

    // Voucher must authorize the bond
    voucher.require_auth();

    // Issue #1428: Reject a second bond for the same (voucher, loan_id) pair while
    // a prior bond is still live. Storage is a plain overwrite, so without this
    // guard a repeat call would silently drop the first bond and lose track of the
    // coverage it had already committed, defeating the MAX_BOND_COVERAGE_BPS cap.
    if let Some(existing) = env
        .storage()
        .persistent()
        .get::<_, VouchProtectionBond>(&DataKey::VouchProtectionBond(voucher.clone(), loan_id))
    {
        if existing.status != BondStatus::Released {
            return Err(ContractError::BondAlreadyActive);
        }
    }

    // Create the protection bond
    let bond = VouchProtectionBond {
        voucher: voucher.clone(),
        loan_id,
        vouch_id,
        bond_amount,
        protected_stake,
        created_at: env.ledger().timestamp(),
        amount_used: 0,
        released_at: None,
        status: BondStatus::Active,
        has_insurance: false,
    };

    // Store the bond
    env.storage()
        .persistent()
        .set(&DataKey::VouchProtectionBond(voucher.clone(), loan_id), &bond);

    // Update bond stats
    let mut stats: BondStats = env
        .storage()
        .persistent()
        .get(&DataKey::BondStats(voucher.clone()))
        .unwrap_or(BondStats {
            voucher: voucher.clone(),
            total_bonded: 0,
            total_used: 0,
            active_bonds: 0,
            times_bond_used: 0,
            total_insurance_premiums: 0,
            insurance_claims_paid: 0,
            total_insurance_payout: 0,
            last_activity: env.ledger().timestamp(),
        });

    stats.total_bonded = stats.total_bonded.checked_add(bond_amount)
        .ok_or(ContractError::ArithmeticError)?;
    stats.active_bonds += 1;
    stats.last_activity = env.ledger().timestamp();

    env.storage()
        .persistent()
        .set(&DataKey::BondStats(voucher.clone()), &stats);

    // Issue #1426: announce the new bond so indexers/dashboards can track coverage.
    env.events().publish(
        ("bond", "staked"),
        (voucher, loan_id, vouch_id, bond_amount, protected_stake),
    );

    Ok(())
}

/// Issue #1175: Purchase optional bond insurance (3% premium).
pub fn purchase_bond_insurance(
    env: Env,
    loan_id: u64,
    voucher: Address,
    bond_amount: i128,
) -> Result<(), ContractError> {
    require_not_paused(&env)?;

    let mut bond: VouchProtectionBond = env
        .storage()
        .persistent()
        .get(&DataKey::VouchProtectionBond(voucher.clone(), loan_id))
        .ok_or(ContractError::InvalidAmount)?;

    if bond.has_insurance {
        return Err(ContractError::InvalidAmount);
    }

    // Calculate premium: 3% of bond amount
    let premium = (bond_amount * BOND_INSURANCE_PREMIUM_BPS) / 10_000;

    // Create insurance record
    let insurance = BondInsuranceRecord {
        voucher: voucher.clone(),
        loan_id,
        insured_bond_amount: bond_amount,
        premium_paid: premium,
        max_coverage: bond_amount,
        amount_claimed: 0,
        status: InsuranceStatus::Active,
        purchased_at: env.ledger().timestamp(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::BondInsurance(voucher.clone(), loan_id), &insurance);

    // Mark bond as having insurance
    bond.has_insurance = true;
    env.storage()
        .persistent()
        .set(&DataKey::VouchProtectionBond(voucher.clone(), loan_id), &bond);

    // Update stats
    let mut stats: BondStats = env
        .storage()
        .persistent()
        .get(&DataKey::BondStats(voucher.clone()))
        .ok_or(ContractError::InvalidAmount)?;

    stats.total_insurance_premiums = stats.total_insurance_premiums.checked_add(premium)
        .ok_or(ContractError::ArithmeticError)?;
    stats.last_activity = env.ledger().timestamp();

    env.storage()
        .persistent()
        .set(&DataKey::BondStats(voucher.clone()), &stats);

    // Issue #1426: announce the insurance purchase and premium charged.
    env.events().publish(
        ("bond", "insurance_purchased"),
        (voucher, loan_id, bond_amount, premium),
    );

    Ok(())
}

/// Issue #1175: Use bond to cover a slash.
/// Called when a vouch is slashed to apply bond coverage first.
pub fn apply_bond_coverage(
    env: &Env,
    loan_id: u64,
    voucher: &Address,
    slash_amount: i128,
) -> Result<i128, ContractError> {
    let mut bond: VouchProtectionBond = env
        .storage()
        .persistent()
        .get(&DataKey::VouchProtectionBond(voucher.clone(), loan_id))
        .ok_or(ContractError::InvalidAmount)?;

    if bond.status == BondStatus::Released || bond.status == BondStatus::Exhausted {
        return Err(ContractError::InvalidAmount);
    }

    let available_bond = bond.bond_amount - bond.amount_used;
    let bond_used = slash_amount.min(available_bond);

    bond.amount_used = bond.amount_used.checked_add(bond_used)
        .ok_or(ContractError::ArithmeticError)?;

    // Update bond status
    if bond.amount_used >= bond.bond_amount {
        bond.status = BondStatus::Exhausted;
    } else {
        bond.status = BondStatus::PartiallyUsed;
    }

    env.storage()
        .persistent()
        .set(&DataKey::VouchProtectionBond(voucher.clone(), loan_id), &bond);

    // Update stats
    let mut stats: BondStats = env
        .storage()
        .persistent()
        .get(&DataKey::BondStats(voucher.clone()))
        .ok_or(ContractError::InvalidAmount)?;

    stats.total_used = stats.total_used.checked_add(bond_used)
        .ok_or(ContractError::ArithmeticError)?;
    stats.times_bond_used += 1;

    // Check if insurance should cover the shortfall
    if bond.has_insurance && bond_used < slash_amount {
        let insurance: Option<BondInsuranceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::BondInsurance(voucher.clone(), loan_id));

        if let Some(mut insurance_record) = insurance {
            if insurance_record.status == InsuranceStatus::Active {
                let shortfall = slash_amount - bond_used;
                let insurance_payout = shortfall.min(insurance_record.max_coverage - insurance_record.amount_claimed);

                if insurance_payout > 0 {
                    insurance_record.amount_claimed = insurance_record.amount_claimed.checked_add(insurance_payout)
                        .ok_or(ContractError::ArithmeticError)?;

                    if insurance_record.amount_claimed >= insurance_record.max_coverage {
                        insurance_record.status = InsuranceStatus::Claimed;
                    }

                    env.storage()
                        .persistent()
                        .set(&DataKey::BondInsurance(voucher.clone(), loan_id), &insurance_record);

                    stats.insurance_claims_paid += 1;
                    stats.total_insurance_payout = stats.total_insurance_payout.checked_add(insurance_payout)
                        .ok_or(ContractError::ArithmeticError)?;
                    stats.last_activity = env.ledger().timestamp();

                    // Issue #1426: persist the stat mutations on the insurance
                    // path too (previously this early return dropped them).
                    env.storage()
                        .persistent()
                        .set(&DataKey::BondStats(voucher.clone()), &stats);

                    // Issue #1426: bond coverage applied, plus a dedicated
                    // insurance_claimed event for the shortfall payout.
                    env.events().publish(
                        ("bond", "coverage_applied"),
                        (voucher.clone(), loan_id, slash_amount, bond_used),
                    );
                    env.events().publish(
                        ("bond", "insurance_claimed"),
                        (voucher.clone(), loan_id, insurance_payout, insurance_record.amount_claimed),
                    );

                    return Ok(bond_used + insurance_payout);
                }
            }
        }
    }

    stats.last_activity = env.ledger().timestamp();
    env.storage()
        .persistent()
        .set(&DataKey::BondStats(voucher.clone()), &stats);

    // Issue #1426: announce bond coverage applied to a slash.
    env.events().publish(
        ("bond", "coverage_applied"),
        (voucher.clone(), loan_id, slash_amount, bond_used),
    );

    Ok(bond_used)
}

/// Issue #1175: Release bond after loan completion.
/// Refund any unused bond amount to the voucher.
pub fn release_bond(
    env: Env,
    loan_id: u64,
    voucher: Address,
) -> Result<i128, ContractError> {
    require_not_paused(&env)?;

    let mut bond: VouchProtectionBond = env
        .storage()
        .persistent()
        .get(&DataKey::VouchProtectionBond(voucher.clone(), loan_id))
        .ok_or(ContractError::InvalidAmount)?;

    if bond.status == BondStatus::Released {
        return Err(ContractError::InvalidAmount);
    }

    // Calculate refund amount
    let refund_amount = bond.bond_amount - bond.amount_used;

    // Update bond status
    bond.status = BondStatus::Released;
    bond.released_at = Some(env.ledger().timestamp());

    env.storage()
        .persistent()
        .set(&DataKey::VouchProtectionBond(voucher.clone(), loan_id), &bond);

    // Release insurance if present, and (Issue #1427) pro-rate a premium refund
    // when the insurance was never claimed against. Releasing early otherwise
    // means paying the full 3% premium for coverage that was never used, making
    // insurance a strictly punished choice for vouchers who exit early.
    let mut premium_refund = 0i128;
    if bond.has_insurance {
        let mut insurance: BondInsuranceRecord = env
            .storage()
            .persistent()
            .get(&DataKey::BondInsurance(voucher.clone(), loan_id))
            .ok_or(ContractError::InvalidAmount)?;

        if insurance.amount_claimed == 0 {
            premium_refund = prorated_premium_refund(
                insurance.premium_paid,
                insurance.purchased_at,
                env.ledger().timestamp(),
            );
        }

        if insurance.status == InsuranceStatus::Active {
            insurance.status = InsuranceStatus::Released;
        }
        env.storage()
            .persistent()
            .set(&DataKey::BondInsurance(voucher.clone(), loan_id), &insurance);
    }

    // Update stats
    let mut stats: BondStats = env
        .storage()
        .persistent()
        .get(&DataKey::BondStats(voucher.clone()))
        .ok_or(ContractError::InvalidAmount)?;

    stats.active_bonds = stats.active_bonds.saturating_sub(1);
    // Net out the refunded premium so lifetime totals stay accurate.
    stats.total_insurance_premiums = stats.total_insurance_premiums.saturating_sub(premium_refund);
    stats.last_activity = env.ledger().timestamp();

    env.storage()
        .persistent()
        .set(&DataKey::BondStats(voucher.clone()), &stats);

    let total_refund = refund_amount
        .checked_add(premium_refund)
        .ok_or(ContractError::ArithmeticError)?;

    // Issue #1426: announce the release with the principal + premium breakdown.
    env.events().publish(
        ("bond", "released"),
        (voucher, loan_id, refund_amount, premium_refund),
    );

    Ok(total_refund)
}

/// Issue #1175: Get bond protection record.
pub fn get_bond(env: Env, loan_id: u64, voucher: Address) -> Result<VouchProtectionBond, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::VouchProtectionBond(voucher, loan_id))
        .ok_or(ContractError::InvalidAmount)
}

/// Issue #1175: Get bond insurance record.
pub fn get_bond_insurance(
    env: Env,
    loan_id: u64,
    voucher: Address,
) -> Result<BondInsuranceRecord, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::BondInsurance(voucher, loan_id))
        .ok_or(ContractError::InvalidAmount)
}

/// Issue #1175: Get bond statistics for a voucher.
pub fn get_bond_stats(env: Env, voucher: Address) -> Result<BondStats, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::BondStats(voucher))
        .ok_or(ContractError::InvalidAmount)
}

/// Issue #1175: Get bond utilization rate (percentage of bonds used).
pub fn get_bond_utilization_rate(env: Env, voucher: Address) -> Result<u32, ContractError> {
    let stats = get_bond_stats(env, voucher)?;

    if stats.total_bonded == 0 {
        return Ok(0);
    }

    let utilization_bps = (stats.total_used * 10_000) / stats.total_bonded;
    Ok(utilization_bps.min(10_000) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QuorumCreditContract;
    use soroban_sdk::testutils::{Address as _, Ledger};

    const PROTECTED: i128 = 20_000;
    const BOND: i128 = 10_000;

    fn setup() -> (Env, soroban_sdk::Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let voucher = Address::generate(&env);
        (env, contract_id, voucher)
    }

    /// Issue #1428: a second bond for the same (voucher, loan_id) while the first
    /// is still active is rejected with BondAlreadyActive.
    #[test]
    fn sequential_bond_for_same_pair_is_rejected() {
        let (env, contract_id, voucher) = setup();
        env.as_contract(&contract_id, || {
            stake_bond_for_vouch_protection(env.clone(), 1, 1, voucher.clone(), PROTECTED, BOND).unwrap();

            let err = stake_bond_for_vouch_protection(env.clone(), 1, 1, voucher.clone(), PROTECTED, BOND)
                .unwrap_err();
            assert_eq!(err, ContractError::BondAlreadyActive);

            // After release, a fresh bond for the same pair is allowed again.
            release_bond(env.clone(), 1, voucher.clone()).unwrap();
            stake_bond_for_vouch_protection(env.clone(), 1, 1, voucher.clone(), PROTECTED, BOND).unwrap();
        });
    }

    /// Issue #1427: releasing immediately after buying insurance with zero claims
    /// refunds the entire premium on top of the unused principal.
    #[test]
    fn immediate_release_refunds_full_premium() {
        let (env, contract_id, voucher) = setup();
        env.as_contract(&contract_id, || {
            stake_bond_for_vouch_protection(env.clone(), 2, 2, voucher.clone(), PROTECTED, BOND).unwrap();
            purchase_bond_insurance(env.clone(), 2, voucher.clone(), BOND).unwrap();
            let premium = (BOND * BOND_INSURANCE_PREMIUM_BPS) / 10_000;

            let refund = release_bond(env.clone(), 2, voucher.clone()).unwrap();
            assert_eq!(refund, BOND + premium, "unused principal + full premium");

            let stats = get_bond_stats(env.clone(), voucher.clone()).unwrap();
            assert_eq!(stats.total_insurance_premiums, 0, "refunded premium netted out");
        });
    }

    /// Issue #1427: a release part-way through the term refunds a pro-rated slice
    /// of the premium; a release at/after the full term refunds nothing.
    #[test]
    fn partial_and_no_premium_refund_by_time_held() {
        let (env, contract_id, voucher) = setup();
        let premium = (BOND * BOND_INSURANCE_PREMIUM_BPS) / 10_000;

        // Halfway through the term -> ~half the premium back.
        env.as_contract(&contract_id, || {
            env.ledger().set_timestamp(1_000);
            stake_bond_for_vouch_protection(env.clone(), 3, 3, voucher.clone(), PROTECTED, BOND).unwrap();
            purchase_bond_insurance(env.clone(), 3, voucher.clone(), BOND).unwrap();
            env.ledger().set_timestamp(1_000 + BOND_INSURANCE_FULL_TERM_SECS / 2);
            let refund = release_bond(env.clone(), 3, voucher.clone()).unwrap();
            let premium_part = refund - BOND;
            assert!(premium_part > 0 && premium_part < premium, "pro-rated: {premium_part}");
            assert!((premium_part - premium / 2).abs() <= 1);
        });

        // At/after the full term -> no premium refund.
        env.as_contract(&contract_id, || {
            env.ledger().set_timestamp(5_000_000);
            stake_bond_for_vouch_protection(env.clone(), 4, 4, voucher.clone(), PROTECTED, BOND).unwrap();
            purchase_bond_insurance(env.clone(), 4, voucher.clone(), BOND).unwrap();
            env.ledger().set_timestamp(5_000_000 + BOND_INSURANCE_FULL_TERM_SECS + 1);
            let refund = release_bond(env.clone(), 4, voucher.clone()).unwrap();
            assert_eq!(refund, BOND, "no premium refund once the term is fully elapsed");
        });
    }

    /// Issue #1427: once an insurance claim has been paid, no premium is refunded.
    #[test]
    fn no_premium_refund_after_a_claim() {
        let (env, contract_id, voucher) = setup();
        env.as_contract(&contract_id, || {
            stake_bond_for_vouch_protection(env.clone(), 5, 5, voucher.clone(), PROTECTED, BOND).unwrap();
            purchase_bond_insurance(env.clone(), 5, voucher.clone(), BOND).unwrap();
            // Slash larger than the bond so the insurance covers the shortfall.
            apply_bond_coverage(&env, 5, &voucher, BOND + 1_000).unwrap();

            let refund = release_bond(env.clone(), 5, voucher.clone()).unwrap();
            assert_eq!(refund, 0, "bond fully used and premium not refundable after a claim");
        });
    }

    /// Issue #1426: each mutating entry point publishes an event, and an
    /// insurance payout adds a dedicated insurance_claimed event.
    #[test]
    fn mutating_paths_emit_events() {
        let (env, contract_id, voucher) = setup();
        env.as_contract(&contract_id, || {
            let before = env.events().all().len();
            stake_bond_for_vouch_protection(env.clone(), 6, 6, voucher.clone(), PROTECTED, BOND).unwrap();
            assert!(env.events().all().len() > before, "stake emits an event");

            let before = env.events().all().len();
            purchase_bond_insurance(env.clone(), 6, voucher.clone(), BOND).unwrap();
            assert!(env.events().all().len() > before, "purchase emits an event");

            let before = env.events().all().len();
            apply_bond_coverage(&env, 6, &voucher, BOND + 1_000).unwrap();
            // coverage_applied + insurance_claimed
            assert!(env.events().all().len() >= before + 2, "coverage + insurance_claimed events");

            let before = env.events().all().len();
            release_bond(env.clone(), 6, voucher.clone()).unwrap();
            assert!(env.events().all().len() > before, "release emits an event");
        });
    }
}
