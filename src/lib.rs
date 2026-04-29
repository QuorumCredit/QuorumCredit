#![no_std]

mod errors;
mod helpers;
mod types;
mod vouch;

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

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
use crate::types::VouchHistoryEntry;

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

    pub fn batch_vouch(
        env: Env,
        voucher: Address,
        borrowers: Vec<Address>,
        stakes: Vec<i128>,
        token: Address,
    ) -> Result<(), ContractError> {
        vouch::batch_vouch(env, voucher, borrowers, stakes, token)
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

    pub fn decrease_stake(
        env: Env,
        voucher: Address,
        borrower: Address,
        amount: i128,
    ) -> Result<(), ContractError> {
        vouch::decrease_stake(env, voucher, borrower, amount)
    }

    pub fn withdraw_vouch(
        env: Env,
        voucher: Address,
        borrower: Address,
    ) -> Result<(), ContractError> {
        vouch::withdraw_vouch(env, voucher, borrower)
    }

    // ─────────────────────────────────────────────
    // Transfer & Delegation
    // ─────────────────────────────────────────────

    pub fn transfer_vouch(
        env: Env,
        from: Address,
        to: Address,
        borrower: Address,
    ) -> Result<(), ContractError> {
        vouch::transfer_vouch(env, from, to, borrower)
    }

    pub fn delegate_vouch(
        env: Env,
        voucher: Address,
        borrower: Address,
        delegate: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        vouch::delegate_vouch(env, voucher, borrower, delegate, token)
    }

    pub fn revoke_delegation(
        env: Env,
        voucher: Address,
        borrower: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        vouch::revoke_delegation(env, voucher, borrower, token)
    }

    // ─────────────────────────────────────────────
    // Expiry
    // ─────────────────────────────────────────────

    pub fn set_vouch_expiry(
        env: Env,
        voucher: Address,
        borrower: Address,
        expiry_timestamp: u64,
        token: Address,
    ) -> Result<(), ContractError> {
        vouch::set_vouch_expiry(env, voucher, borrower, expiry_timestamp, token)
    }

    // ─────────────────────────────────────────────
    // Queries
    // ─────────────────────────────────────────────

    pub fn vouch_exists(
        env: Env,
        voucher: Address,
        borrower: Address,
    ) -> bool {
        vouch::vouch_exists(env, voucher, borrower)
    }

    pub fn total_vouched(
        env: Env,
        borrower: Address,
    ) -> Result<i128, ContractError> {
        vouch::total_vouched(env, borrower)
    }

    pub fn voucher_history(
        env: Env,
        voucher: Address,
    ) -> Vec<Address> {
        vouch::voucher_history(env, voucher)
    }

    pub fn get_vouch_history(
        env: Env,
        borrower: Address,
        voucher: Address,
        token: Address,
    ) -> Vec<VouchHistoryEntry> {
        vouch::get_vouch_history(env, borrower, voucher, token)
    }
}
