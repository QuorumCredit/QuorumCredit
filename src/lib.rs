#![no_std]

mod errors;
mod helpers;
mod types;
mod vouch;

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

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