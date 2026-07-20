#![no_std]

pub mod admin;
pub mod credit_score;
pub mod cross_chain;
pub mod errors;
pub mod governance;
pub mod helpers;
pub mod loan;
pub mod rbac;
pub mod reputation;
pub mod types;
pub mod vouch;

pub use errors::ContractError;
pub use types::*;
pub use cross_chain::{BridgeAttestation, CrossChainLoanMetadata, UnifiedReputation};

#[cfg(test)]
mod tests;

use crate::helpers::{
    acquire_lock, config, get_active_loan_record, has_active_loan, is_zero_address,
    loan_status as helper_loan_status, release_lock, require_allowed_token, require_not_paused,
};
use crate::types::{AdminOperationType, Config, DataKey, DEFAULT_LOAN_DURATION, DEFAULT_MAX_LOAN_TO_STAKE_RATIO, DEFAULT_MAX_VOUCHERS, DEFAULT_MIN_LOAN_AMOUNT, DEFAULT_SLASH_BPS, DEFAULT_YIELD_BPS, DEFAULT_MIN_VOUCH_AGE_SECS};
use crate::reputation::ReputationNftExternalClient;
use soroban_sdk::{
    contract, contractimpl, panic_with_error, symbol_short, token, Address, BytesN, Env, String,
    Vec,
};

#[contract]
pub struct QuorumCreditContract;

#[contractimpl]
impl QuorumCreditContract {
    // ── Initialization ────────────────────────────────────────────────────────

    pub fn initialize(
        env: Env,
        deployer: Address,
        admins: Vec<Address>,
        admin_threshold: u32,
        token: Address,
    ) -> Result<(), ContractError> {
        deployer.require_auth();

        if env.storage().instance().has(&DataKey::Config) {
            return Err(ContractError::AlreadyInitialized);
        }

        helpers::validate_admin_config(
            &env,
            &admins,
            admin_threshold,
            &Vec::new(&env),
            &Vec::new(&env),
        )?;
        helpers::require_valid_token(&env, &token)?;

        env.storage().instance().set(&DataKey::Deployer, &deployer);
        env.storage().instance().set(
            &DataKey::Config,
            &Config {
                admins: admins.clone(),
                admin_threshold,
                admin_whitelist: Vec::new(&env),
                admin_blacklist: Vec::new(&env),
                token: token.clone(),
                allowed_tokens: Vec::new(&env),
                yield_bps: DEFAULT_YIELD_BPS,
                slash_bps: DEFAULT_SLASH_BPS,
                max_vouchers: DEFAULT_MAX_VOUCHERS,
                min_loan_amount: DEFAULT_MIN_LOAN_AMOUNT,
                loan_duration: DEFAULT_LOAN_DURATION,
                max_loan_to_stake_ratio: DEFAULT_MAX_LOAN_TO_STAKE_RATIO,
                max_loan_to_collateral_ratio: DEFAULT_MAX_LOAN_TO_COLLATERAL_RATIO,
                grace_period: 0,
                min_vouch_age_secs: DEFAULT_MIN_VOUCH_AGE_SECS,
                prepayment_penalty_bps: 0,
                liquidity_mining_rate_bps: DEFAULT_LIQUIDITY_MINING_RATE_BPS,
                voting_period_seconds: DEFAULT_VOTING_PERIOD_SECONDS,
                slash_cooldown_seconds: 0,
                emergency_pause_enabled: false,
                early_repayment_discount_bps: 0,
                oracle_address: None,
                slash_delay_seconds: 0,
                successor_admin: None,
                rate_limit_config: RateLimitConfig {
                    window_secs: DEFAULT_RATE_LIMIT_WINDOW_SECS,
                    max_calls: DEFAULT_RATE_LIMIT_COUNT,
                    enabled: false,
                },
                multi_tier_thresholds: Vec::new(&env), // Issue #893: Initialize with no multi-tier thresholds
                dynamic_slash_threshold: DEFAULT_DYNAMIC_SLASH_THRESHOLD,
                loan_size_slash_enabled: DEFAULT_LOAN_SIZE_SLASH_ENABLED,
                loan_size_slash_max_bps: DEFAULT_LOAN_SIZE_SLASH_MAX_BPS,
                recovery_percentage: 0,
                admin_compensation_bps: 0,
                removal_vote_threshold: 0,
                confirmation_required: DEFAULT_CONFIRMATION_REQUIRED,
                redistribution_rule: RedistributionRule::Treasury,
                immunity_period_seconds: 0,
                insurance_premium_bps: 0,
            },
        );

        env.events().publish(
            (symbol_short!("contract"), symbol_short!("init")),
            (deployer, admins, admin_threshold, token),
        );

        Ok(())
    }

    // ── Vouching ──────────────────────────────────────────────────────────────


    pub fn vouch(
        env: Env,
        voucher: Address,
        borrower: Address,
        stake: i128,
        token: Address,
        chain_id: Option<u32>,
    ) -> Result<(), ContractError> {
        vouch::vouch(env, voucher, borrower, stake, token, chain_id)
    }

    /// Issue #632: Vouch with cross-chain support.
    /// chain_id=0 is native Stellar; non-zero requires prior bridge validation.
    pub fn vouch_cross_chain(
        env: Env,
        voucher: Address,
        borrower: Address,
        stake: i128,
        token: Address,
        chain_id: u32,
    ) -> Result<(), ContractError> {
        vouch::vouch_cross_chain(env, voucher, borrower, stake, token, chain_id)
    }

    /// Issue #632: Admin sets bridge validation status for a voucher on a given chain.
    pub fn set_bridge_validated(
        env: Env,
        admin_signers: Vec<Address>,
        voucher: Address,
        chain_id: u32,
        validated: bool,
    ) -> Result<(), ContractError> {
        vouch::set_bridge_validated(env, admin_signers, voucher, chain_id, validated)
    }

    /// Issue #632: Query bridge validation status.
    pub fn is_bridge_validated(env: Env, voucher: Address, chain_id: u32) -> bool {
        vouch::is_bridge_validated(env, voucher, chain_id)
    }

    pub fn batch_vouch(
        env: Env,
        voucher: Address,
        borrowers: Vec<Address>,
        stakes: Vec<i128>,
        token: Address,
        chain_id: Option<u32>,
    ) -> Result<Vec<crate::types::BatchVouchResult>, ContractError> {
        vouch::batch_vouch(env, voucher, borrowers, stakes, token, chain_id)
    }

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

    pub fn transfer_vouch(
        env: Env,
        from: Address,
        to: Address,
        borrower: Address,
    ) -> Result<(), ContractError> {
        acquire_lock(&env)?;
        let result = vouch::transfer_vouch(env.clone(), from, to, borrower);
        release_lock(&env);
        result
    }

    pub fn delegate_vouch(
        env: Env,
        voucher: Address,
        borrower: Address,
        delegate: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        acquire_lock(&env)?;
        let result = vouch::delegate_vouch(env.clone(), voucher, borrower, delegate, token);
        release_lock(&env);
        result
    }

    pub fn revoke_delegation(
        env: Env,
        voucher: Address,
        borrower: Address,
        token: Address,
    ) -> Result<(), ContractError> {
        acquire_lock(&env)?;
        let result = vouch::revoke_delegation(env.clone(), voucher, borrower, token);
        release_lock(&env);
        result
    }

    pub fn set_vouch_expiry(
        env: Env,
        voucher: Address,
        borrower: Address,
        expiry: u64,
        token: Address,
    ) -> Result<(), ContractError> {
        acquire_lock(&env)?;
        let result = vouch::set_vouch_expiry(env.clone(), voucher, borrower, expiry, token);
        release_lock(&env);
        result
    }

    // ── Loans ─────────────────────────────────────────────────────────────────

    pub fn register_referral(
        env: Env,
        borrower: Address,
        referrer: Address,
    ) -> Result<(), ContractError> {
        loan::register_referral(env, borrower, referrer)
    }

    pub fn get_referrer(env: Env, borrower: Address) -> Option<Address> {
        loan::get_referrer(env, borrower)
    }

    pub fn set_referral_bonus_bps(env: Env, admin_signers: Vec<Address>, bonus_bps: u32) {
        helpers::require_admin_approval(&env, &admin_signers);
        assert!(bonus_bps <= 10_000, "bonus_bps must not exceed 10000");
        env.storage()
            .instance()
            .set(&DataKey::ReferralBonusBps, &bonus_bps);
    }

    pub fn get_referral_bonus_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ReferralBonusBps)
            .unwrap_or(DEFAULT_REFERRAL_BONUS_BPS)
    }

    pub fn get_withdrawal_queue(env: Env, borrower: Address) -> Vec<QueuedWithdrawal> {
        vouch::get_withdrawal_queue(env, borrower)
    }

    pub fn process_withdrawal_batch(env: Env, borrower: Address, count: u32) -> u32 {
        vouch::process_withdrawal_batch(&env, &borrower, count)
    }

    pub fn request_loan(
        env: Env,
        borrower: Address,
        amount: i128,
        threshold: i128,
        loan_purpose: soroban_sdk::String,
        token: Address,
    ) -> Result<(), ContractError> {
        loan::request_loan(env, borrower, amount, threshold, loan_purpose, token)
    }


    pub fn dispute_vouch(
        env: Env,
        voucher: Address,
        borrower: Address,
        evidence_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        vouch::dispute_vouch(env, voucher, borrower, evidence_hash)
    }

    pub fn slash(env: Env, admin_signers: Vec<Address>, borrower: Address) {
        helpers::require_admin_approval(&env, &admin_signers);
        helpers::require_not_paused(&env).expect("contract is paused");

        let mut loan = helpers::get_active_loan_record(&env, &borrower)
            .expect("no active loan");

        if loan.status != LoanStatus::Active {
            panic_with_error!(&env, ContractError::NoActiveLoan);
        }

        let cfg = config(&env);
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        loan.status = LoanStatus::Defaulted;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan.id), &loan);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(borrower.clone()));

        // Process withdrawal queue before deleting vouches (Issue #865)
        vouch::process_withdrawal_queue(&env, &borrower);

        // Re-read vouches after queue processing removed queued withdrawals
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        let token_client = token::Client::new(&env, &loan.token_address);
        let mut total_slashed: i128 = 0;
        for v in vouches.iter() {
            if v.token == loan.token_address {
                let slash_amount = v.stake * cfg.slash_bps / 10_000;
                let returned = v.stake - slash_amount;
                if returned > 0 {
                    token_client.transfer(&env.current_contract_address(), &v.voucher, &returned);
                }
                total_slashed += slash_amount;
            } else if !is_zero_address(&env, &v.token) {
                // Non-matching token vouches are returned in full.
                let other_token = soroban_sdk::token::Client::new(&env, &v.token);
                other_token.transfer(&env.current_contract_address(), &v.voucher, &v.stake);
            }
        }

        helpers::add_slash_balance(&env, total_slashed);

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::DefaultCount(borrower.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::DefaultCount(borrower.clone()), &(count + 1));

        // Burn excellent credit tier badge on default
        reputation::burn_excellent_badge(&env, &borrower);

        // Update credit score after slash
        let _ = credit_score::update_credit_score(env.clone(), borrower.clone());

        // Clean up vouches storage last
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));
    }

    pub fn repay(env: Env, borrower: Address, payment: i128) -> Result<(), ContractError> {
        loan::repay(env, borrower, payment)
    }


    /// Confirm intent to repay the active loan.
    ///
    /// When `Config.confirmation_required` is `true`, borrowers must call this
    /// function before calling `repay`. The confirmation is stored per-loan and
    /// consumed on the first successful `repay` call, so it cannot be replayed.
    ///
    /// This is a no-op (succeeds silently) when `confirmation_required` is false,
    /// so callers can always call it without checking the config first.
    pub fn confirm_repayment(env: Env, borrower: Address) -> Result<(), ContractError> {
        borrower.require_auth();
        require_not_paused(&env)?;

        let loan = get_active_loan_record(&env, &borrower)?;

        env.storage()
            .persistent()
            .set(&DataKey::RepaymentConfirmation(loan.id), &true);

        env.events().publish(
            (symbol_short!("loan"), symbol_short!("repay_ok")),
            (borrower, loan.id),
        );

        Ok(())
    }

    /// #667: Called by the registered oracle to verify a repayment held in escrow.
    /// If `approved` is true, releases funds to vouchers. If false, returns funds to borrower.
    /// Called by the registered oracle to publish a fresh price for `key`
    /// (e.g. a collateral token's symbol). Used to inform dynamic-rate pricing.
    pub fn set_oracle_price(
        env: Env,
        oracle: Address,
        key: soroban_sdk::Symbol,
        price: i128,
    ) -> Result<(), ContractError> {
        helpers::set_oracle_price(&env, &oracle, key, price)
    }

    pub fn verify_repayment(
        env: Env,
        oracle: Address,
        borrower: Address,
        approved: bool,
    ) -> Result<(), ContractError> {
        oracle.require_auth();
        require_not_paused(&env)?;

        // Verify caller is the registered oracle
        let cfg = config(&env);
        let registered = cfg.oracle_address.ok_or(ContractError::OracleUnauthorized)?;
        if oracle != registered {
            return Err(ContractError::OracleUnauthorized);
        }

        let loan_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveLoan(borrower.clone()))
            .ok_or(ContractError::NoActiveLoan)?;
        let mut loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(loan_id))
            .ok_or(ContractError::NoActiveLoan)?;

        if loan.escrow_status != EscrowStatus::Pending {
            return Err(ContractError::NoEscrowFound);
        }

        let escrowed: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::EscrowAmount(borrower.clone()))
            .unwrap_or(0);

        let token_client = require_allowed_token(&env, &loan.token_address)?;
        let now = env.ledger().timestamp();

        if approved {
            loan.escrow_status = EscrowStatus::Released;
            loan.status = LoanStatus::Repaid;
            loan.repayment_timestamp = Some(now);

            let vouches: Vec<VouchRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::Vouches(borrower.clone()))
                .unwrap_or(Vec::new(&env));

            let total_stake: i128 = vouches
                .iter()
                .filter(|v| v.token == loan.token_address)
                .map(|v| v.stake)
                .sum();

            for v in vouches.iter() {
                if v.token != loan.token_address {
                    continue;
                }
                // Issue #633: Yield tiering — vouch age bonus.
                // Vouches older than 30 days get +50% of their yield share.
                // Vouches older than 7 days get +25% of their yield share.
                let vouch_age_secs = loan.disbursement_timestamp.saturating_sub(v.vouch_timestamp);
                let age_multiplier_bps: i128 = if vouch_age_secs >= 30 * 24 * 60 * 60 {
                    15_000 // 150%
                } else if vouch_age_secs >= 7 * 24 * 60 * 60 {
                    12_500 // 125%
                } else {
                    10_000 // 100% base
                };

                let base_yield_share = if total_stake > 0 {
                    loan.total_yield * v.stake / total_stake
                } else {
                    0
                };
                let tiered_yield = base_yield_share * age_multiplier_bps / 10_000;

                // Issue #634: Liquidity mining reward on top of yield.
                let cfg = config(&env);
                let mining_reward = if cfg.liquidity_mining_rate_bps > 0 {
                    v.stake * cfg.liquidity_mining_rate_bps as i128 / 10_000
                } else {
                    0
                };

                token_client.transfer(
                    &env.current_contract_address(),
                    &v.voucher,
                    &(v.stake + tiered_yield + mining_reward),
                );
            }

            // Process withdrawal queue before deleting vouches (Issue #865)
            vouch::process_withdrawal_queue(&env, &borrower);

            // Increment borrower repayment count
            let prev_count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::RepaymentCount(borrower.clone()))
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&DataKey::RepaymentCount(borrower.clone()), &(prev_count + 1));

            // Update credit score after successful repayment
            let _ = credit_score::update_credit_score(env.clone(), borrower.clone());

            env.storage()
                .persistent()
                .remove(&DataKey::ActiveLoan(borrower.clone()));
            env.storage()
                .persistent()
                .remove(&DataKey::Vouches(borrower.clone()));

            env.events().publish(
                (symbol_short!("loan"), symbol_short!("repaid")),
                (borrower.clone(), loan.amount),
            );
        } else {
            // Oracle rejected — return escrowed funds to borrower
            loan.escrow_status = EscrowStatus::Rejected;
            loan.amount_repaid -= escrowed;

            if escrowed > 0 {
                token_client.transfer(
                    &env.current_contract_address(),
                    &borrower,
                    &escrowed,
                );
            }

            env.events().publish(
                (symbol_short!("loan"), symbol_short!("escrw_rej")),
                (borrower.clone(), escrowed),
            );
        }

        env.storage()
            .persistent()
            .remove(&DataKey::EscrowAmount(borrower.clone()));
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan.id), &loan);

        release_lock(&env);
        Ok(())
    }


    /// Callable by anyone after the loan deadline has passed. Applies the standard slash penalty.
    pub fn auto_slash(env: Env, borrower: Address) {
        let mut loan = helpers::get_active_loan_record(&env, &borrower)
            .expect("no active loan");

        if loan.status != LoanStatus::Active {
            panic_with_error!(&env, ContractError::NoActiveLoan);
        }
        assert!(
            env.ledger().timestamp() > loan.deadline,
            "loan deadline has not passed"
        );

        let cfg = config(&env);
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        loan.status = LoanStatus::Defaulted;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan.id), &loan);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(borrower.clone()));

        // Process withdrawal queue before deleting vouches (Issue #865)
        vouch::process_withdrawal_queue(&env, &borrower);

        // Re-read vouches after queue processing
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        let token_client = token::Client::new(&env, &loan.token_address);
        let mut total_slash: i128 = 0;
        for v in vouches.iter() {
            if v.token == loan.token_address {
                let slash_amount = v.stake * cfg.slash_bps / 10_000;
                let returned = v.stake - slash_amount;
                total_slash += slash_amount;
                if returned > 0 {
                    token_client.transfer(&env.current_contract_address(), &v.voucher, &returned);
                }
            } else if !is_zero_address(&env, &v.token) {
                // Non-matching token vouches are returned in full.
                let other_token = soroban_sdk::token::Client::new(&env, &v.token);
                other_token.transfer(&env.current_contract_address(), &v.voucher, &v.stake);
            }
        }

        helpers::add_slash_balance(&env, total_slash);

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::DefaultCount(borrower.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::DefaultCount(borrower.clone()), &(count + 1));

        if let Some(nft_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ReputationNft)
        {
            ReputationNftExternalClient::new(&env, &nft_addr).burn(&borrower);
        }

        // Update credit score after auto slash
        let _ = credit_score::update_credit_score(env.clone(), borrower.clone());

        // Clean up vouches storage last
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));
    }

    /// Allows vouchers to claim back their stake if loan has expired without repayment or slash.
    pub fn claim_expired_loan(env: Env, borrower: Address) {
        borrower.require_auth();

        let mut loan = helpers::get_active_loan_record(&env, &borrower)
            .expect("no active loan");

        if loan.status != LoanStatus::Active {
            panic_with_error!(&env, ContractError::NoActiveLoan);
        }

        let now = env.ledger().timestamp();
        assert!(now >= loan.deadline, "loan has not expired yet");

        // Process withdrawal queue first (Issue #865)
        vouch::process_withdrawal_queue(&env, &borrower);

        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        let token_client = token::Client::new(&env, &loan.token_address);
        for v in vouches.iter() {
            if v.token == loan.token_address {
                token_client.transfer(&env.current_contract_address(), &v.voucher, &v.stake);
            } else if !is_zero_address(&env, &v.token) {
                // Non-matching token vouches are returned via their own token.
                let other_token = soroban_sdk::token::Client::new(&env, &v.token);
                other_token.transfer(&env.current_contract_address(), &v.voucher, &v.stake);
            }
        }

        loan.status = LoanStatus::Defaulted;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan.id), &loan);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(borrower.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower));
    }

    /// Admin withdraws accumulated slashed funds to a recipient address.
    pub fn slash_treasury(env: Env, admin_signers: Vec<Address>, recipient: Address) {
        helpers::require_admin_approval(&env, &admin_signers);

        let amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::SlashTreasury)
            .unwrap_or(0);
        assert!(amount > 0, "no slashed funds to withdraw");

        env.storage().instance().set(&DataKey::SlashTreasury, &0i128);
        helpers::primary_token(&env).transfer(&env.current_contract_address(), &recipient, &amount);
    }

    // ── Loan Pool ─────────────────────────────────────────────────────────────

    /// Admin function: atomically disburse a batch of small loans to multiple borrowers.
    pub fn create_loan_pool(
        env: Env,
        admin_signers: Vec<Address>,
        borrowers: Vec<Address>,
        amounts: Vec<i128>,
    ) -> Result<u64, ContractError> {
        helpers::require_admin_approval(&env, &admin_signers);

        if borrowers.len() != amounts.len() {
            return Err(ContractError::PoolLengthMismatch);
        }
        if borrowers.is_empty() {
            return Err(ContractError::PoolEmpty);
        }

        let cfg = config(&env);
        let now = env.ledger().timestamp();
        let deadline = now + cfg.loan_duration;

        let mut total_amount: i128 = 0;
        for i in 0..borrowers.len() {
            let borrower = borrowers.get(i).unwrap();
            let amount = amounts.get(i).unwrap();

            assert!(
                amount >= cfg.min_loan_amount,
                "pool: amount below minimum loan threshold"
            );

            if helpers::has_active_loan(&env, &borrower) {
                return Err(ContractError::PoolBorrowerActiveLoan);
            }

            let total_stake: i128 = env
                .storage()
                .persistent()
                .get::<DataKey, Vec<VouchRecord>>(&DataKey::Vouches(borrower.clone()))
                .unwrap_or(Vec::new(&env))
                .iter()
                .map(|v| v.stake)
                .sum();
            let max_allowed = total_stake * cfg.max_loan_to_stake_ratio as i128 / 100;
            assert!(
                amount <= max_allowed,
                "pool: loan amount exceeds maximum collateral ratio for borrower"
            );

            total_amount += amount;
        }

        let token_client = helpers::primary_token(&env);
        let contract_balance = token_client.balance(&env.current_contract_address());
        if contract_balance < total_amount {
            return Err(ContractError::PoolInsufficientFunds);
        }

        let pool_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LoanPoolCounter)
            .unwrap_or(0u64)
            .checked_add(1)
            .expect("pool ID overflow");
        env.storage()
            .instance()
            .set(&DataKey::LoanPoolCounter, &pool_id);

        for i in 0..borrowers.len() {
            let borrower = borrowers.get(i).unwrap();
            let amount = amounts.get(i).unwrap();
            let loan_id = helpers::next_loan_id(&env);

            env.storage().persistent().set(
                &DataKey::Loan(loan_id),
                &LoanRecord {
                    id: loan_id,
                    borrower: borrower.clone(),
                    guarantor: None,
                    buyback_price: 0,
                    auto_repay_enabled: false,
                    auto_repay_attempts: 0,
                    escrow_status: EscrowStatus::None,
                    co_borrowers: Vec::new(&env),
                    amount,
                    amount_repaid: 0,
                    total_yield: amount * cfg.yield_bps / 10_000,
                    status: LoanStatus::Active,
                    repaid: false,
                    defaulted: false,
                    created_at: now,
                    disbursement_timestamp: now,
                    repayment_timestamp: None,
                    deadline,
                    loan_purpose: soroban_sdk::String::from_str(&env, "pool"),
                    token_address: cfg.token.clone(),
                    amortization_schedule: Vec::new(&env),
                    reminder_sent: false,
                    risk_score: 0,
                    deferment_periods: 0,
                    maturity_date: None,
                    rate_type: RateType::Fixed,
                    index_reference: None,
                    last_interest_calc: now,
                    accrued_interest: 0,
                    milestone_bonus_applied: false,
                    retry_count: 0,
                    suspension_timestamp: None,
                    suspension_amount_repaid: 0,
                },
            );
            env.storage()
                .persistent()
                .set(&DataKey::ActiveLoan(borrower.clone()), &loan_id);
            env.storage()
                .persistent()
                .set(&DataKey::LatestLoan(borrower.clone()), &loan_id);

            token_client.transfer(&env.current_contract_address(), &borrower, &amount);

            env.events().publish(
                (symbol_short!("pool"), symbol_short!("loan")),
                (pool_id, borrower.clone(), amount, deadline),
            );
        }

        env.storage().persistent().set(
            &DataKey::LoanPool(pool_id),
            &LoanPoolRecord {
                pool_id,
                borrowers: borrowers.clone(),
                amounts: amounts.clone(),
                created_at: now,
                total_disbursed: total_amount,
            },
        );

        env.events().publish(
            (symbol_short!("pool"), symbol_short!("created")),
            (pool_id, borrowers.len(), total_amount),
        );

        Ok(pool_id)
    }

    pub fn get_loan_pool(env: Env, pool_id: u64) -> Option<LoanPoolRecord> {
        env.storage().persistent().get(&DataKey::LoanPool(pool_id))
    }

    pub fn get_loan_pool_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LoanPoolCounter)
            .unwrap_or(0)
    }

    // ── Liquidity Rebalancing (Issue #88) ─────────────────────────────────────




    // ── Admin ─────────────────────────────────────────────────────────────────

    pub fn add_admin(env: Env, admin_signers: Vec<Address>, new_admin: Address) {
        admin::add_admin(env, admin_signers, new_admin)
    }

    /// #669: Retry a failed repayment. Increments retry_count and re-attempts the transfer.
    /// Returns `MaxRetriesExceeded` if retry_count >= MAX_REPAYMENT_RETRIES.
    pub fn retry_repayment(
        env: Env,
        borrower: Address,
        payment: i128,
    ) -> Result<(), ContractError> {
        borrower.require_auth();
        require_not_paused(&env)?;

        let loan_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveLoan(borrower.clone()))
            .ok_or(ContractError::NoActiveLoan)?;
        let mut loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(loan_id))
            .ok_or(ContractError::NoActiveLoan)?;

        const MAX_REPAYMENT_RETRIES: u32 = 3;
        if loan.retry_count >= MAX_REPAYMENT_RETRIES {
            return Err(ContractError::MaxRetriesExceeded);
        }

        loan.retry_count += 1;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan.id), &loan);

        // Delegate to the standard repay logic
        Self::repay(env, borrower, payment)
    }

    pub fn get_loan(env: Env, borrower: Address) -> Option<LoanRecord> {
        let loan_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveLoan(borrower.clone()))?;
        env.storage().persistent().get(&DataKey::Loan(loan_id))
    }

    pub fn get_vouches(env: Env, borrower: Address) -> Vec<VouchRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::Vouches(borrower))
            .unwrap_or(Vec::new(&env))
    }

    pub fn vouch_exists(env: Env, voucher: Address, borrower: Address) -> bool {
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower))
            .unwrap_or(Vec::new(&env));
        vouches.iter().any(|v| v.voucher == voucher)
    }

    /// Returns the total primary-token stake for `borrower`.
    pub fn total_vouched(env: Env, borrower: Address) -> Result<i128, ContractError> {
        vouch::total_vouched(env, borrower)
    }

    pub fn get_config(env: Env) -> Config {
        config(&env)
    }

    pub fn loan_status(env: Env, borrower: Address) -> LoanStatus {
        helper_loan_status(&env, &borrower)
    }

    pub fn loan_status_extended(env: Env, borrower: Address) -> LoanStatusEx {
        loan::loan_status_extended(env, borrower)
    }

    pub fn suspend_loan_on_missed_payment(
        env: Env,
        caller: Address,
        borrower: Address,
    ) -> Result<(), ContractError> {
        loan::suspend_loan_on_missed_payment(env, caller, borrower)
    }

    pub fn resume_loan(env: Env, caller: Address, borrower: Address) -> Result<(), ContractError> {
        loan::resume_loan(env, caller, borrower)
    }

    // ── Issue #880: Loan Co-Borrower Support ─────────────────────────────────

    pub fn add_co_borrower(
        env: Env,
        borrower: Address,
        co_borrower: Address,
    ) -> Result<(), ContractError> {
        loan::add_co_borrower(env, borrower, co_borrower)
    }

    pub fn remove_co_borrower(
        env: Env,
        borrower: Address,
        co_borrower: Address,
    ) -> Result<(), ContractError> {
        loan::remove_co_borrower(env, borrower, co_borrower)
    }

    pub fn get_co_borrowers(env: Env, borrower: Address) -> Vec<Address> {
        loan::get_co_borrowers(env, borrower)
    }

    // ── Issue #881: Dynamic Interest Rate ────────────────────────────────────

    pub fn set_dynamic_rate_config(
        env: Env,
        admin_signers: Vec<Address>,
        config: DynamicRateConfig,
    ) -> Result<(), ContractError> {
        loan::set_dynamic_rate_config(env, admin_signers, config)
    }

    pub fn get_dynamic_rate_config(env: Env) -> DynamicRateConfig {
        loan::get_dynamic_rate_config_view(env)
    }

    pub fn compute_dynamic_rate(
        env: Env,
        admin_signers: Vec<Address>,
        borrower: Address,
    ) -> Result<u32, ContractError> {
        loan::compute_and_store_dynamic_rate(env, admin_signers, borrower)
    }

    pub fn get_borrower_dynamic_rate(env: Env, borrower: Address) -> Option<BorrowerDynamicRate> {
        loan::get_borrower_dynamic_rate(env, borrower)
    }

    // ── Issue #878: Loan Forbearance Period ──────────────────────────────────

    pub fn request_forbearance(
        env: Env,
        borrower: Address,
        duration_secs: Option<u64>,
    ) -> Result<(), ContractError> {
        loan::request_forbearance(env, borrower, duration_secs)
    }

    pub fn end_forbearance(env: Env, borrower: Address) -> Result<(), ContractError> {
        loan::end_forbearance(env, borrower)
    }

    pub fn get_forbearance(env: Env, loan_id: u64) -> Option<ForbearanceRecord> {
        loan::get_forbearance(env, loan_id)
    }

    // ── Issue #879: Loan Refinancing ─────────────────────────────────────────

    pub fn refinance_loan(
        env: Env,
        borrower: Address,
        new_amount: i128,
        new_threshold: i128,
        new_token: Address,
    ) -> Result<(), ContractError> {
        loan::refinance_loan(env, borrower, new_amount, new_threshold, new_token)
    }

    pub fn get_refinance_record(env: Env, loan_id: u64) -> Option<RefinanceRecord> {
        loan::get_refinance_record(env, loan_id)
    }

    pub fn set_borrower_risk_score(
        env: Env,
        admin_signers: Vec<Address>,
        borrower: Address,
        risk_score: u32,
    ) -> Result<(), ContractError> {
        loan::set_borrower_risk_score(env, admin_signers, borrower, risk_score)
    }

    // ── Governance: slash voting ──────────────────────────────────────────────

    pub fn vote_slash(
        env: Env,
        voucher: Address,
        borrower: Address,
        approve: bool,
    ) -> Result<(), ContractError> {
        governance::vote_slash(env, voucher, borrower, approve)
    }

    pub fn get_slash_vote(env: Env, borrower: Address) -> Option<SlashVoteRecord> {
        governance::get_slash_vote(env, borrower)
    }

    pub fn set_slash_vote_quorum(env: Env, admin_signers: Vec<Address>, quorum_bps: u32) {
        helpers::require_admin_approval(&env, &admin_signers);
        governance::set_slash_vote_quorum(&env, quorum_bps);
    }

    pub fn get_slash_vote_quorum(env: Env) -> u32 {
        governance::get_slash_vote_quorum(env)
    }

    pub fn execute_slash_vote(env: Env, borrower: Address) -> Result<(), ContractError> {
        governance::execute_slash_vote(env, borrower)
    }

    pub fn execute_pending_slash(env: Env, borrower: Address) -> Result<(), ContractError> {
        governance::execute_pending_slash(env, borrower)
    }

    // ── Issue #1069: Vote Delegation ─────────────────────────────────────────

    pub fn delegate_vote(
        env: Env,
        voucher: Address,
        delegate: Address,
    ) -> Result<(), ContractError> {
        governance::delegate_vote(env, voucher, delegate)
    }

    pub fn revoke_vote_delegation(
        env: Env,
        voucher: Address,
    ) -> Result<(), ContractError> {
        governance::revoke_vote_delegation(env, voucher)
    }

    pub fn get_vote_delegate(env: Env, voucher: Address) -> Option<Address> {
        governance::get_vote_delegate(env, voucher)
    }

    // ── Issue #680: slash threshold governance ────────────────────────────────

    pub fn propose_slash_threshold(
        env: Env,
        proposer: Address,
        new_threshold: i128,
    ) -> Result<u64, ContractError> {
        governance::propose_slash_threshold(env, proposer, new_threshold)
    }

    pub fn vote_slash_threshold(
        env: Env,
        voter: Address,
        proposal_id: u64,
        approve: bool,
    ) -> Result<(), ContractError> {
        governance::vote_slash_threshold(env, voter, proposal_id, approve)
    }

    pub fn finalize_slash_threshold(env: Env, proposal_id: u64) -> Result<(), ContractError> {
        governance::finalize_slash_threshold(env, proposal_id)
    }

    pub fn get_slash_threshold_proposal(
        env: Env,
        proposal_id: u64,
    ) -> Option<SlashThresholdProposal> {
        governance::get_slash_threshold_proposal(env, proposal_id)
    }

    // ── Config Timelock ───────────────────────────────────────────────────────

    pub fn propose_config_change(
        env: Env,
        proposer: Address,
        new_config: Config,
    ) -> Result<u64, ContractError> {
        governance::propose_config_change(env, proposer, new_config)
    }

    pub fn execute_config_change(env: Env, proposal_id: u64) -> Result<(), ContractError> {
        governance::execute_config_change(env, proposal_id)
    }

    pub fn cancel_config_change(
        env: Env,
        admin_signers: Vec<Address>,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        governance::cancel_config_change(env, admin_signers, proposal_id)
    }

    // ── Slash Appeal & Escrow (Issue #841) ────────────────────────────────────

    pub fn appeal_slash(env: Env, borrower: Address) -> Result<(), ContractError> {
        governance::appeal_slash(env, borrower)
    }

    pub fn vote_appeal(
        env: Env,
        voucher: Address,
        borrower: Address,
        approve: bool,
    ) -> Result<(), ContractError> {
        governance::vote_appeal(env, voucher, borrower, approve)
    }

    pub fn finalize_appeal(env: Env, borrower: Address) -> Result<(), ContractError> {
        governance::finalize_appeal(env, borrower)
    }

    // ── Admin management ─────────────────────────────────────────────────────

    pub fn remove_admin(env: Env, admin_signers: Vec<Address>, admin_to_remove: Address) {
        admin::remove_admin(env, admin_signers, admin_to_remove)
    }

    /// Emergency admin revocation — removes a compromised admin key with N-1 approval.
    ///
    /// This is an emergency mechanism: if one admin key is compromised, ALL remaining
    /// admins (N-1 of N) can instantly revoke the compromised key. The revoked address
    /// is permanently blacklisted from participating in admin approvals and is removed
    /// from the active admin list.
    ///
    /// Unlike `remove_admin` (which uses the standard `admin_threshold`), this function
    /// requires every non-compromised admin to sign — a stricter requirement that prevents
    /// a single admin from unilaterally removing another.
    ///
    /// # Arguments
    /// * `existing_admins` - All current admin signers excluding `target_admin` (must be N-1)
    /// * `target_admin` - The compromised admin address to revoke
    /// * `reason` - Human-readable reason for revocation (emitted in event)
    ///
    /// # Errors
    /// * `ContractError::AdminNotFound` - `target_admin` is not a registered admin
    /// * `ContractError::AdminAlreadyRevoked` - `target_admin` was already revoked
    /// * `ContractError::UnauthorizedCaller` - Fewer than N-1 valid signers provided
    /// * `ContractError::InvalidAdminThreshold` - Only 1 admin exists; cannot revoke
    pub fn revoke_admin(
        env: Env,
        existing_admins: Vec<Address>,
        target_admin: Address,
        reason: soroban_sdk::String,
    ) -> Result<(), ContractError> {
        admin::revoke_admin(env, existing_admins, target_admin, reason)
    }

    /// Check whether an admin address has been emergency-revoked.
    ///
    /// # Arguments
    /// * `admin` - Address to query
    ///
    /// # Returns
    /// * `true` if the address has been revoked via `revoke_admin`
    pub fn is_admin_revoked(env: Env, admin: Address) -> bool {
        admin::is_admin_revoked(env, admin)
    }

    pub fn set_admin_threshold(env: Env, admin_signers: Vec<Address>, new_threshold: u32) {
        admin::set_admin_threshold(env, admin_signers, new_threshold)
    }

    // ── RBAC (Issue #16) ──────────────────────────────────────────────────────

    pub fn assign_admin_role(
        env: Env,
        admin_signers: Vec<Address>,
        target_admin: Address,
        role: AdminRole,
    ) {
        rbac::assign_admin_role(&env, admin_signers, target_admin, role)
    }

    pub fn get_admin_role(env: Env, admin: Address) -> Result<AdminRole, ContractError> {
        rbac::get_admin_role(&env, &admin)
    }

    // ── Admin ─────────────────────────────────────────────────────────────────

    pub fn pause(env: Env, admin_signers: Vec<Address>) {
        admin::pause(env, admin_signers)
    }

    pub fn unpause(env: Env, admin_signers: Vec<Address>) {
        admin::unpause(env, admin_signers)
    }

    pub fn get_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn set_config(env: Env, admin_signers: Vec<Address>, cfg: Config) {
        admin::set_config(env, admin_signers, cfg)
    }

    // ── Issue #688: Admin whitelist management ────────────────────────────────

    pub fn add_to_admin_whitelist(env: Env, admin_signers: Vec<Address>, address: Address) {
        admin::add_to_admin_whitelist(env, admin_signers, address)
    }

    pub fn remove_from_admin_whitelist(env: Env, admin_signers: Vec<Address>, address: Address) {
        admin::remove_from_admin_whitelist(env, admin_signers, address)
    }

    // ── Issue #689: Admin blacklist management ────────────────────────────────

    pub fn add_to_admin_blacklist(env: Env, admin_signers: Vec<Address>, address: Address) {
        admin::add_to_admin_blacklist(env, admin_signers, address)
    }

    pub fn remove_from_admin_blacklist(env: Env, admin_signers: Vec<Address>, address: Address) {
        admin::remove_from_admin_blacklist(env, admin_signers, address)
    }

    pub fn update_config(
        env: Env,
        admin_signers: Vec<Address>,
        yield_bps: Option<i128>,
        slash_bps: Option<i128>,
    ) {
        admin::update_config(env, admin_signers, yield_bps, slash_bps)
    }

    pub fn batch_update_config(
        env: Env,
        admin_signers: Vec<Address>,
        yield_bps: Option<i128>,
        slash_bps: Option<i128>,
        max_vouchers: Option<u32>,
        min_loan_amount: Option<i128>,
        loan_duration: Option<u64>,
        max_loan_to_stake_ratio: Option<u32>,
        grace_period: Option<u64>,
        liquidity_mining_rate_bps: Option<u32>,
    ) {
        admin::batch_update_config(
            env,
            admin_signers,
            yield_bps,
            slash_bps,
            max_vouchers,
            min_loan_amount,
            loan_duration,
            max_loan_to_stake_ratio,
            grace_period,
            liquidity_mining_rate_bps,
        )
    }

    pub fn set_reputation_nft(env: Env, admin_signers: Vec<Address>, nft_contract: Address) {
        admin::set_reputation_nft(env, admin_signers, nft_contract)
    }

    pub fn set_min_stake(env: Env, admin_signers: Vec<Address>, amount: i128) {
        admin::set_min_stake(env, admin_signers, amount)
    }

    pub fn set_max_loan_amount(env: Env, admin_signers: Vec<Address>, amount: i128) {
        admin::set_max_loan_amount(env, admin_signers, amount)
    }

    pub fn set_min_vouchers(env: Env, admin_signers: Vec<Address>, count: u32) {
        admin::set_min_vouchers(env, admin_signers, count)
    }

    pub fn set_max_loan_to_stake_ratio(env: Env, admin_signers: Vec<Address>, ratio: u32) {
        admin::set_max_loan_to_stake_ratio(env, admin_signers, ratio)
    }

    pub fn set_max_vouchers_per_loan(env: Env, admin_signers: Vec<Address>, max: u32) {
        helpers::require_admin_approval(&env, &admin_signers);
        assert!(max > 0, "max_vouchers_per_loan must be greater than zero");
        let mut cfg = config(&env);
        cfg.max_vouchers = max;
        env.storage().instance().set(&DataKey::Config, &cfg);
    }

    pub fn add_allowed_token(env: Env, admin_signers: Vec<Address>, token: Address) -> Result<(), ContractError> {
        admin::add_allowed_token(env, admin_signers, token)
    }

    pub fn remove_allowed_token(env: Env, admin_signers: Vec<Address>, token: Address) {
        admin::remove_allowed_token(env, admin_signers, token)
    }

    // ── Views ─────────────────────────────────────────────────────────────────

    pub fn is_initialized(env: Env) -> bool {
        env.storage().instance().has(&DataKey::Config)
    }

    pub fn get_token(env: Env) -> Address {
        config(&env).token
    }

    pub fn get_admins(env: Env) -> Vec<Address> {
        admin::get_admins(env)
    }

    pub fn get_admin_threshold(env: Env) -> u32 {
        admin::get_admin_threshold(env)
    }

    pub fn get_slash_treasury_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::SlashTreasury)
            .unwrap_or(0)
    }


    pub fn get_protocol_fee(env: Env) -> u32 {
        admin::get_protocol_fee(env)
    }

    pub fn get_fee_treasury(env: Env) -> Option<Address> {
        admin::get_fee_treasury(env)
    }

    pub fn is_blacklisted(env: Env, borrower: Address) -> bool {
        admin::is_blacklisted(env, borrower)
    }

    pub fn get_min_stake(env: Env) -> i128 {
        admin::get_min_stake(env)
    }

    pub fn get_max_loan_amount(env: Env) -> i128 {
        admin::get_max_loan_amount(env)
    }

    pub fn get_min_vouchers(env: Env) -> u32 {
        admin::get_min_vouchers(env)
    }

    pub fn get_max_loan_to_stake_ratio(env: Env) -> u32 {
        admin::get_max_loan_to_stake_ratio(env)
    }

    pub fn get_max_vouchers_per_loan(env: Env) -> u32 {
        config(&env).max_vouchers
    }

    // ── Issue #682: multi-sig config updates ──────────────────────────────────

    pub fn propose_config_update(
        env: Env,
        proposer: Address,
        key: ConfigUpdateKey,
        new_value: u32,
    ) -> Result<u64, ContractError> {
        admin::propose_config_update(env, proposer, key, new_value)
    }

    pub fn approve_config_update(
        env: Env,
        admin: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        admin::approve_config_update(env, admin, proposal_id)
    }

    pub fn finalize_config_update(env: Env, proposal_id: u64) -> Result<(), ContractError> {
        admin::finalize_config_update(env, proposal_id)
    }

    pub fn get_config_update_proposal(
        env: Env,
        proposal_id: u64,
    ) -> Option<ConfigUpdateProposal> {
        admin::get_config_update_proposal(env, proposal_id)
    }

    // ── Admin Governance Queue with Multi-Signature Confirmation ─────────────

    pub fn set_governance_queue_config(
        env: Env,
        admin_signers: Vec<Address>,
        config: GovernanceQueueConfig,
    ) {
        admin::set_governance_queue_config(env, admin_signers, config)
    }

    pub fn propose_governance_action(
        env: Env,
        proposer: Address,
        action: GovernanceAction,
        description: soroban_sdk::String,
    ) -> Result<u64, ContractError> {
        admin::propose_governance_action(env, proposer, action, description)
    }

    pub fn approve_governance_action(
        env: Env,
        admin: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        admin::approve_governance_action(env, admin, proposal_id)
    }

    pub fn reject_governance_action(
        env: Env,
        admin: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        admin::reject_governance_action(env, admin, proposal_id)
    }

    pub fn execute_governance_action(
        env: Env,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        admin::execute_governance_action(env, proposal_id)
    }

    pub fn cancel_governance_action(
        env: Env,
        caller: Address,
        proposal_id: u64,
    ) -> Result<(), ContractError> {
        admin::cancel_governance_action(env, caller, proposal_id)
    }

    pub fn get_governance_proposal(
        env: Env,
        proposal_id: u64,
    ) -> Option<GovernanceProposal> {
        admin::get_governance_proposal(env, proposal_id)
    }

    pub fn get_governance_queue_config_view(env: Env) -> GovernanceQueueConfig {
        admin::get_governance_queue_config_view(env)
    }

    pub fn get_governance_proposal_count(env: Env) -> u64 {
        admin::get_governance_proposal_count(env)
    }

    // ── On-Chain Credit Score with Tiered Rewards ───────────────────────────────

    pub fn update_credit_score(env: Env, borrower: Address) -> Result<(), ContractError> {
        credit_score::update_credit_score(env, borrower)
    }

    pub fn get_credit_score(env: Env, borrower: Address) -> Option<CreditScore> {
        credit_score::get_credit_score(env, borrower)
    }

    pub fn set_credit_score_config(
        env: Env,
        admin_signers: Vec<Address>,
        config: CreditScoreConfig,
    ) -> Result<(), ContractError> {
        credit_score::set_credit_score_config(env, admin_signers, config)
    }

    pub fn get_credit_score_config_view(env: Env) -> CreditScoreConfig {
        credit_score::get_credit_score_config_view(env)
    }

    pub fn get_tier_rewards(env: Env, tier: CreditTier) -> TierRewards {
        credit_score::get_tier_rewards(env, tier)
    }

    // ── Issue #637: On-Demand Fraud Detection ──────────────────────────────────





    // ── Loan Pool Syndication for Multi-Borrower Loans ─────────────────────────









    // ── Data Archiving ────────────────────────────────────────────────────────





    // ── IPFS Archiving ────────────────────────────────────────────────────────













    // ── Issue #683: emergency pause ───────────────────────────────────────────

    pub fn emergency_pause(env: Env, admin: Address) -> Result<(), ContractError> {
        admin::emergency_pause(env, admin)
    }

    pub fn emergency_unpause(env: Env, admin_signers: Vec<Address>) -> Result<(), ContractError> {
        admin::emergency_unpause(env, admin_signers)
    }

    /// Toggle the borrower repayment confirmation requirement on/off.
    ///
    /// When enabled, borrowers must call `confirm_repayment` before `repay`.
    pub fn set_confirmation_required(
        env: Env,
        admin_signers: Vec<Address>,
        enabled: bool,
    ) {
        admin::set_confirmation_required(env, admin_signers, enabled)
    }

    pub fn set_successor_admin(
        env: Env,
        admin_signers: Vec<Address>,
        successor: Option<Address>,
    ) {
        admin::set_successor_admin(env, admin_signers, successor)
    }

    pub fn claim_successor_admin(env: Env) -> Result<(), ContractError> {
        admin::claim_successor_admin(env)
    }

    // ── Issue #14: Cross-chain loan portability ───────────────────────────────












    // ── Issue #969 (#86): Cross-Chain Event Relay ─────────────────────────────











    // ── Custom Attributes ────────────────────────────────────────────────────




    // ── Yield Stream ─────────────────────────────────────────────────────────




    // ── Vouch Groups ─────────────────────────────────────────────────────────






    // ── Periodic Payments ────────────────────────────────────────────────────





    // ── Issue #883: Loan Term Extension ─────────────────────────────────────

    pub fn request_extension(
        env: Env,
        borrower: Address,
        extension_secs: u64,
    ) -> Result<(), ContractError> {
        loan::request_extension(env, borrower, extension_secs)
    }

    pub fn approve_extension(
        env: Env,
        voucher: Address,
        borrower: Address,
    ) -> Result<(), ContractError> {
        loan::approve_extension(env, voucher, borrower)
    }

    pub fn get_extension_request(env: Env, borrower: Address) -> Option<LoanExtensionRequest> {
        loan::get_extension_request(env, borrower)
    }

    // ── Issue #882: Loan Insurance Integration ──────────────────────────────










    // ── Issue #884: Prepayment Bonus ────────────────────────────────────────

    pub fn set_prepayment_bonus_bps(
        env: Env,
        admin_signers: Vec<Address>,
        bonus_bps: u32,
    ) -> Result<(), ContractError> {
        loan::set_prepayment_bonus_bps(env, admin_signers, bonus_bps)
    }

    pub fn get_prepayment_bonus_bps(env: Env) -> u32 {
        loan::get_prepayment_bonus_bps(&env)
    }

    // ── Issue #885: Loan Status Privacy ─────────────────────────────────────

    pub fn set_loan_privacy(
        env: Env,
        borrower: Address,
        privacy: LoanPrivacyLevel,
    ) -> Result<(), ContractError> {
        loan::set_loan_privacy(env, borrower, privacy)
    }

    pub fn get_loan_privacy(env: Env, borrower: Address) -> LoanPrivacyLevel {
        loan::get_loan_privacy(&env, &borrower)
    }

    pub fn get_loan_with_privacy(
        env: Env,
        borrower: Address,
        caller: Address,
    ) -> Result<Option<LoanRecord>, ContractError> {
        loan::get_loan_with_privacy(env, borrower, caller)
    }

    // ── Issue #893: Multi-Tier Admin Approval ──────────────────────────────────

    pub fn set_multi_tier_thresholds(
        env: Env,
        admin_signers: Vec<Address>,
        thresholds: MultiTierAdminThresholds,
    ) {
        admin::set_multi_tier_thresholds(env, admin_signers, thresholds)
    }

    pub fn get_multi_tier_thresholds(env: Env) -> Option<MultiTierAdminThresholds> {
        admin::get_multi_tier_thresholds(env)
    }

    pub fn get_effective_approval_threshold(
        env: Env,
        operation_type: AdminOperationType,
    ) -> u32 {
        admin::get_effective_approval_threshold(env, operation_type)
    }

    // ── Cross-chain bridge management ─────────────────────────────────────────

    /// Register a new cross-chain bridge so vouchers may stake wrapped tokens from that chain.
    pub fn register_bridge(
        env: Env,
        admin_signers: Vec<Address>,
        chain_id: u32,
        chain_name: String,
        bridge_address: Address,
    ) -> Result<(), ContractError> {
        vouch::register_bridge(env, admin_signers, chain_id, chain_name, bridge_address)
    }

    /// Deactivate a registered bridge; prevents new cross-chain vouches for that chain.
    pub fn remove_bridge(
        env: Env,
        admin_signers: Vec<Address>,
        chain_id: u32,
    ) -> Result<(), ContractError> {
        vouch::remove_bridge(env, admin_signers, chain_id)
    }

    /// Return all registered bridges (active and inactive).
    pub fn get_bridges(env: Env) -> Vec<crate::types::BridgeRecord> {
        vouch::get_bridges(env)
    }

    /// Admin: configure or rotate the Ed25519 key used to verify attestations from `origin_chain`.
    pub fn set_bridge_public_key(
        env: Env,
        admin_signers: Vec<Address>,
        origin_chain: u32,
        public_key: BytesN<32>,
    ) -> Result<(), ContractError> {
        cross_chain::set_bridge_public_key(env, admin_signers, origin_chain, public_key)
    }

    /// Canonical bytes an origin-chain attestor key must sign for this payload.
    pub fn bridge_attestation_message(
        env: Env,
        metadata: CrossChainLoanMetadata,
        nonce: u64,
        timestamp: u64,
        confirmations: u32,
    ) -> soroban_sdk::Bytes {
        cross_chain::bridge_attestation_message(&env, &metadata, nonce, timestamp, confirmations)
    }

    /// Verify a bridge attestation and consume its nonce, so it cannot be replayed.
    pub fn validate_bridge_attestation(
        env: Env,
        metadata: CrossChainLoanMetadata,
        attestation: BridgeAttestation,
    ) -> Result<(), ContractError> {
        cross_chain::validate_bridge_attestation(env, metadata, attestation)
    }

    /// Issue #968/#85: Read-only integrity check — verifies signature, freshness,
    /// confirmations, and nonce without consuming any state. Safe to call multiple times.
    pub fn verify_bridge_message(
        env: Env,
        metadata: CrossChainLoanMetadata,
        attestation: BridgeAttestation,
    ) -> Result<(), ContractError> {
        cross_chain::verify_bridge_message(env, metadata, attestation)
    }

    /// Accept a bridge-attested loan-completion event and mirror it into local storage.
    pub fn mirror_loan_to_chain(
        env: Env,
        metadata: CrossChainLoanMetadata,
        attestation: BridgeAttestation,
    ) -> Result<(), ContractError> {
        cross_chain::mirror_loan_to_chain(env, metadata, attestation)
    }

    pub fn query_mirrored_loan(
        env: Env,
        origin_chain: u32,
        loan_id: u64,
    ) -> Option<CrossChainLoanMetadata> {
        cross_chain::query_mirrored_loan(env, origin_chain, loan_id)
    }

    pub fn query_reputation_cross_chain(env: Env, borrower: Address) -> Option<UnifiedReputation> {
        cross_chain::query_reputation_cross_chain(env, borrower)
    }

    pub fn is_bridge_nonce_used(env: Env, origin_chain: u32, nonce: u64) -> bool {
        cross_chain::is_bridge_nonce_used(env, origin_chain, nonce)
    }
}

