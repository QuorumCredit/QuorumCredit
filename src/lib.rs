#![no_std]

mod errors;
mod helpers;
mod types;
mod vouch;

use soroban_sdk::{contract, contractimpl, Address, Env};

#[cfg(test)]
mod get_loan_none_test;
#[cfg(test)]
mod loan_overwrite_protection_test;
#[cfg(test)]
mod max_vouchers_per_borrower_test;
#[cfg(test)]
mod paused_state_test;
#[cfg(test)]
mod repay_nonexistent_loan_test;
#[cfg(test)]
mod partial_repay_test;
#[cfg(test)]
mod slash_multi_voucher_test;
#[cfg(test)]
mod voucher_balance_check_test;
#[cfg(test)]
mod refinance_test;
#[cfg(test)]
mod co_borrower_test;
#[cfg(test)]
mod collateral_test;
#[cfg(test)]
mod prepayment_penalty_test;
#[cfg(test)]
mod vouch_cooldown_test;
#[cfg(test)]
mod vouch_active_loan_test;
#[cfg(test)]
mod request_loan_stake_threshold_test;
#[cfg(test)]
mod increase_stake_overflow_test;
#[cfg(test)]
mod batch_vouch_partial_failure_test;
#[cfg(test)]
mod decrease_stake_full_withdrawal_test;
#[cfg(test)]
mod initialize_admin_threshold_test;
#[cfg(test)]
mod repay_protocol_fee_test;
#[cfg(test)]
mod is_eligible_token_filter_test;
#[cfg(test)]
mod vote_slash_auto_execute_test;
#[cfg(test)]
mod repayment_reminder_test;
#[cfg(test)]
mod mint_reputation_nft_test;
#[cfg(test)]
mod insurance_test;
#[cfg(test)]
mod dynamic_yield_test;
#[cfg(test)]
mod multi_token_vouch_test;
#[cfg(test)]
mod yield_reserve_solvency_test;
#[cfg(test)]
mod slash_escrow_test;
#[cfg(test)]
mod fuzz_loan_state_machine_test;
#[cfg(test)]
mod property_based_yield_test;
use crate::errors::ContractError;

#[contract]
pub struct QuorumCreditContract;

#[contractimpl]
impl QuorumCreditContract {
    // ─────────────────────────────────────────────
    // Core Vouching
    // ─────────────────────────────────────────────

    pub fn vouch(
        env: Env,
        voucher: Address,
        borrower: Address,
        stake: i128,
        token: Address,
    ) -> Result<(), ContractError> {
        vouch::vouch(env, voucher, borrower, stake, token)
    }

    // ─────────────────────────────────────────────
    // Stake Management
    // ─────────────────────────────────────────────

    pub fn increase_stake(
        env: Env,
        voucher: Address,
        borrower: Address,
        additional: i128,
    ) -> Result<(), ContractError> {
        vouch::increase_stake(env, voucher, borrower, additional)
    }

}
