#![no_std]

pub mod admin;
mod contract;
pub mod errors;
pub mod governance;
pub mod helpers;
pub mod insurance;
pub mod loan;
pub mod reputation;
#[cfg(test)]
mod tests;
pub mod types;
pub mod vouch;

pub use contract::QuorumCreditContract;
pub use errors::ContractError;
pub use types::*;

#[cfg(test)]
mod get_loan_none_test;

// #[cfg(test)]
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
mod slash_multi_voucher_test;
#[cfg(test)]
mod voucher_balance_check_test;
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
