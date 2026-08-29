/// Issue #1071: Insurance Fund Mechanism
/// 
/// When slashing cannot cover all voucher losses (tail-risk defaults),
/// the insurance fund absorbs the shortfall. The fund is pre-funded by:
/// 1. Admin contributions
/// 2. A portion of protocol fees
/// 3. Percentage of loan disbursement
///
/// Provides insurance pool helpers and claim processing.
use soroban_sdk::{Address, Env, Vec};
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
/// * `_admin_signers` - Vector of admin addresses authorizing the contribution
/// * `amount` - Amount to contribute in stroops
pub fn contribute_to_insurance_fund(
    env: &Env,
    _admin_signers: Vec<Address>,
    amount: i128,
) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    let current_balance: i128 = env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128);

    let new_balance = current_balance.saturating_add(amount);
    env.storage().instance().set(&DataKey::InsuranceFund, &new_balance);

    let now = env.ledger().timestamp();
    env.storage().instance().set(&DataKey::InsuranceFundLastContribution, &now);

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
