/// Issue #1071: Insurance Fund Mechanism
/// 
/// When slashing cannot cover all voucher losses (tail-risk defaults),
/// the insurance fund absorbs the shortfall. The fund is pre-funded by:
/// 1. Admin contributions
/// 2. A portion of protocol fees
/// 3. Percentage of loan disbursement
///
/// Provides insurance pool helpers and claim processing.
use soroban_sdk::{symbol_short, Address, Env, Vec};
use crate::types::{Config, DataKey};
use crate::errors::ContractError;

/// Collect loan insurance fee at disbursement time.
/// 
/// Routes `insurance_fund_premium_bps` percentage of the loan principal
/// to the dedicated insurance fund.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `loan_amount` - Loan principal in stroops
/// * `config` - Current protocol configuration
pub fn collect_insurance_fee(env: &Env, loan_amount: i128, config: &Config) -> Result<i128, ContractError> {
    if config.insurance_fund_premium_bps == 0 {
        return Ok(0);
    }

    let fee = (loan_amount as u128)
        .saturating_mul(config.insurance_fund_premium_bps as u128)
        .saturating_div(10_000)
        as i128;

    // Add fee to insurance fund balance
    let current_balance: i128 = env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128);

    let new_balance = current_balance.saturating_add(fee);
    env.storage().instance().set(&DataKey::InsuranceFund, &new_balance);

    // Record the contribution timestamp
    let now = env.ledger().timestamp();
    env.storage().instance().set(&DataKey::InsuranceFundLastContribution, &now);

    Ok(fee)
}

/// Admin contribution to the insurance fund.
/// 
/// Called by admins to pre-fund the insurance pool with additional capital.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `admin_signers` - Vector of admin addresses authorizing the contribution
/// * `amount` - Amount to contribute in stroops
/// * `config` - Current protocol configuration
///
/// # Errors
/// * `InvalidAmount` if `amount <= 0`.
/// * `InsuranceContributionTooLarge` (#1437) if `amount` exceeds
///   `config.insurance_fund_max_contrib` (when that cap is non-zero) —
///   a sanity check against fat-finger admin errors.
///
/// # Events
/// Emits `("insurance", "contrib")` with `(amount, new_balance, admin_signer_count)`
/// so manual top-ups leave an on-chain audit trail distinct from organic fee
/// accrual via `collect_insurance_fee`.
pub fn contribute_to_insurance_fund(
    env: &Env,
    admin_signers: Vec<Address>,
    amount: i128,
    config: &Config,
) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Issue #1437: reject contributions above the configured sanity bound.
    if config.insurance_fund_max_contrib > 0
        && amount > config.insurance_fund_max_contrib
    {
        return Err(ContractError::InsuranceContributionTooLarge);
    }

    let current_balance: i128 = env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128);

    let new_balance = current_balance.saturating_add(amount);
    env.storage().instance().set(&DataKey::InsuranceFund, &new_balance);

    let now = env.ledger().timestamp();
    env.storage().instance().set(&DataKey::InsuranceFundLastContribution, &now);

    // Issue #1437: emit an audit event for the manual top-up.
    env.events().publish(
        (symbol_short!("insurance"), symbol_short!("contrib")),
        (amount, new_balance, admin_signers.len()),
    );

    Ok(())
}

/// Claim insurance to cover slash shortfall.
///
/// Called during slash processing when the protocol cannot fully cover
/// all voucher losses from protocol fees or reserves. The insurance fund
/// absorbs the difference, capped at `insurance_max_payout_bps` of the shortfall.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `shortfall` - Amount needed to cover all voucher losses (in stroops)
/// * `config` - Current protocol configuration
///
/// # Returns
/// Amount actually paid out from insurance fund (may be less than shortfall if fund is depleted or payout cap is reached).
pub fn claim_insurance_for_shortfall(
    env: &Env,
    shortfall: i128,
    config: &Config,
) -> Result<i128, ContractError> {
    if shortfall <= 0 {
        return Ok(0);
    }

    let current_balance: i128 = env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128);

    if current_balance == 0 {
        return Err(ContractError::InsurancePoolEmpty);
    }

    let max_payout = (shortfall as u128)
        .saturating_mul(config.insurance_max_payout_bps as u128)
        .saturating_div(10_000)
        as i128;

    let payout = current_balance.min(max_payout);
    let remaining_balance = current_balance.saturating_sub(payout);

    env.storage().instance().set(&DataKey::InsuranceFund, &remaining_balance);

    // Issue #1436: warn operators before the fund is fully depleted. The event
    // fires only on the claim that pushes the balance across the threshold
    // (i.e. it was at/above the threshold before this claim and is below now),
    // so a depleted fund emits exactly one alert rather than one per claim.
    let threshold = config.insurance_fund_low_bal_thresh;
    if threshold > 0 && remaining_balance < threshold && current_balance >= threshold {
        env.events().publish(
            (symbol_short!("insurance"), symbol_short!("low_bal")),
            (remaining_balance, threshold),
        );
    }

    Ok(payout)
}

/// Get the current balance of the insurance fund.
pub fn get_insurance_fund_balance(env: &Env) -> i128 {
    env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128)
}

/// Get the timestamp of the most recent insurance fund contribution.
pub fn get_insurance_fund_last_contribution(env: &Env) -> u64 {
    env.storage().instance()
        .get(&DataKey::InsuranceFundLastContribution)
        .unwrap_or(0u64)
}

#[cfg(test)]
mod insurance_fund_hardening_tests {
    //! Issue #1436 (low-balance alert) and Issue #1437 (contribution cap + event).
    use super::*;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::testutils::{Address as _, Events};
    use soroban_sdk::{vec, Address, Env};

    fn setup() -> (Env, Address, Address, Config) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &vec![&env, admin.clone()], &1u32, &token);

        let cfg = env.as_contract(&contract_id, || crate::helpers::config(&env));
        (env, contract_id, admin, cfg)
    }

    #[test]
    fn contribution_over_configured_cap_is_rejected() {
        let (env, contract_id, admin, mut cfg) = setup();
        cfg.insurance_fund_max_contrib = 1_000;

        env.as_contract(&contract_id, || {
            // Over the cap → rejected, no balance change.
            let err = contribute_to_insurance_fund(
                &env,
                vec![&env, admin.clone()],
                5_000,
                &cfg,
            )
            .unwrap_err();
            assert_eq!(err, ContractError::InsuranceContributionTooLarge);
            assert_eq!(get_insurance_fund_balance(&env), 0);

            // At the cap → accepted.
            contribute_to_insurance_fund(&env, vec![&env, admin.clone()], 1_000, &cfg)
                .expect("contribution at cap should succeed");
            assert_eq!(get_insurance_fund_balance(&env), 1_000);
        });
    }

    #[test]
    fn contribution_emits_audit_event() {
        let (env, contract_id, admin, cfg) = setup();

        env.as_contract(&contract_id, || {
            let before = env.events().all().events().len();
            contribute_to_insurance_fund(&env, vec![&env, admin.clone()], 750, &cfg)
                .expect("contribution should succeed");
            assert!(
                env.events().all().events().len() > before,
                "contribute_to_insurance_fund must emit an event"
            );
        });
    }

    #[test]
    fn low_balance_event_fires_exactly_on_threshold_crossing() {
        let (env, contract_id, _admin, mut cfg) = setup();
        cfg.insurance_fund_low_bal_thresh = 500;
        cfg.insurance_max_payout_bps = 10_000; // allow full-shortfall payout

        env.as_contract(&contract_id, || {
            env.storage().instance().set(&DataKey::InsuranceFund, &1_000i128);

            // First claim keeps balance at/above threshold (1000 -> 600): no alert.
            let before = env.events().all().events().len();
            claim_insurance_for_shortfall(&env, 400, &cfg).unwrap();
            assert_eq!(get_insurance_fund_balance(&env), 600);
            assert_eq!(
                env.events().all().events().len(),
                before,
                "no alert while balance stays above threshold"
            );

            // Second claim crosses the threshold (600 -> 300): exactly one alert.
            let before = env.events().all().events().len();
            claim_insurance_for_shortfall(&env, 300, &cfg).unwrap();
            assert_eq!(get_insurance_fund_balance(&env), 300);
            assert_eq!(
                env.events().all().events().len(),
                before + 1,
                "alert must fire on the claim that crosses the threshold"
            );

            // Third claim while already below threshold: no further alert.
            let before = env.events().all().events().len();
            claim_insurance_for_shortfall(&env, 100, &cfg).unwrap();
            assert_eq!(
                env.events().all().events().len(),
                before,
                "alert must not repeat once already below threshold"
            );
        });
    }
}
