#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, token, Address, BytesN, Env,
    Vec,
};

pub mod admin;
pub mod errors;
pub mod governance;
pub mod helpers;
pub mod insurance;
pub mod insurance_marketplace;
pub mod loan;
pub mod reputation;
pub mod types;
pub mod vouch;

#[cfg(test)]
mod governance_test;
#[cfg(test)]
mod insurance_marketplace_test;
#[cfg(test)]
mod loan_purpose_test;
#[cfg(test)]
mod multi_asset_test;
#[cfg(test)]
mod referral_test;

pub use errors::ContractError;
pub use types::*;

use helpers::{config, require_valid_token, validate_admin_config};
use reputation::ReputationNftExternalClient;

#[contract]
pub struct QuorumCreditContract;

#[contractimpl]
impl QuorumCreditContract {
    pub fn initialize(
        env: Env,
        deployer: Address,
        admins: Vec<Address>,
        admin_threshold: u32,
        token: Address,
    ) {
        deployer.require_auth();

        assert!(
            !env.storage().instance().has(&DataKey::Config),
            "already initialized"
        );

        Self::validate_admin_config(&admins, admin_threshold);

        env.storage().instance().set(&DataKey::Deployer, &deployer);
        env.storage().instance().set(
            &DataKey::Config,
            &Config {
                admins,
                admin_threshold,
                token,
                allowed_tokens: Vec::new(&env),
                yield_bps: DEFAULT_YIELD_BPS,
                slash_bps: DEFAULT_SLASH_BPS,
                max_vouchers: DEFAULT_MAX_VOUCHERS,
                min_loan_amount: DEFAULT_MIN_LOAN_AMOUNT,
                loan_duration: DEFAULT_LOAN_DURATION,
                max_loan_to_stake_ratio: DEFAULT_MAX_LOAN_TO_STAKE_RATIO,
                grace_period: 0,
            },
        );
    }

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
        vouch::transfer_vouch(env, from, to, borrower)
    }

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
            .unwrap_or(crate::types::DEFAULT_REFERRAL_BONUS_BPS)
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

    pub fn repay(env: Env, borrower: Address, payment: i128) -> Result<(), ContractError> {
        loan::repay(env, borrower, payment)
    }

    pub fn add_admin(env: Env, admin_signers: Vec<Address>, new_admin: Address) {
        admin::add_admin(env, admin_signers, new_admin)
    }

    pub fn remove_admin(env: Env, admin_signers: Vec<Address>, admin_to_remove: Address) {
        admin::remove_admin(env, admin_signers, admin_to_remove)
    }

    pub fn rotate_admin(
        env: Env,
        admin_signers: Vec<Address>,
        old_admin: Address,
        new_admin: Address,
    ) {
        admin::rotate_admin(env, admin_signers, old_admin, new_admin)
    }

    pub fn set_admin_threshold(env: Env, admin_signers: Vec<Address>, new_threshold: u32) {
        admin::set_admin_threshold(env, admin_signers, new_threshold)
    }

    pub fn set_protocol_fee(env: Env, admin_signers: Vec<Address>, fee_bps: u32) {
        admin::set_protocol_fee(env, admin_signers, fee_bps)
    }

    pub fn whitelist_voucher(env: Env, admin_signers: Vec<Address>, voucher: Address) {
        admin::whitelist_voucher(env, admin_signers, voucher)
    }

    pub fn set_fee_treasury(env: Env, admin_signers: Vec<Address>, treasury: Address) {
        admin::set_fee_treasury(env, admin_signers, treasury)
    }

    pub fn upgrade(env: Env, admin_signers: Vec<Address>, new_wasm_hash: BytesN<32>) {
        admin::upgrade(env, admin_signers, new_wasm_hash)
    }

    pub fn pause(env: Env, admin_signers: Vec<Address>) {
        admin::pause(env, admin_signers)
    }

    pub fn set_slash_vote_quorum(env: Env, admin_signers: Vec<Address>, quorum_bps: u32) {
        helpers::require_admin_approval(&env, &admin_signers);
        governance::set_slash_vote_quorum(&env, quorum_bps);
    }

    pub fn get_slash_vote_quorum(env: Env) -> u32 {
        governance::get_slash_vote_quorum(env)
    }

    // ── Admin & Config ────────────────────────────────────────────────────────

    pub fn unpause(env: Env, admin_signers: Vec<Address>) {
        admin::unpause(env, admin_signers)
    }

    pub fn blacklist(env: Env, admin_signers: Vec<Address>, borrower: Address) {
        admin::blacklist(env, admin_signers, borrower)
    }

    pub fn set_config(env: Env, admin_signers: Vec<Address>, config: Config) {
        admin::set_config(env, admin_signers, config)
    }

    pub fn update_config(
        env: Env,
        admin_signers: Vec<Address>,
        updates: Vec<(soroban_sdk::String, i128)>,
    ) {
        admin::update_config(env, admin_signers, updates)
    }

    pub fn set_reputation_nft(env: Env, admin_signers: Vec<Address>, nft_contract: Address) {
        admin::set_reputation_nft(env, admin_signers, nft_contract)
    }

    pub fn set_min_stake(env: Env, admin_signers: Vec<Address>, amount: i128) {
        admin::set_min_stake(env, admin_signers, amount)
    }

    pub fn get_min_stake(env: Env) -> i128 {
        admin::get_min_stake(env)
    }

    pub fn set_max_loan_amount(env: Env, admin_signers: Vec<Address>, amount: i128) {
        admin::set_max_loan_amount(env, admin_signers, amount)
    }

    pub fn get_max_loan_amount(env: Env) -> i128 {
        admin::get_max_loan_amount(env)
    }

    pub fn set_min_vouchers(env: Env, admin_signers: Vec<Address>, count: u32) {
        admin::set_min_vouchers(env, admin_signers, count)
    }

    pub fn get_min_vouchers(env: Env) -> u32 {
        admin::get_min_vouchers(env)
    }

    pub fn set_max_loan_to_stake_ratio(env: Env, admin_signers: Vec<Address>, ratio: u32) {
        admin::set_max_loan_to_stake_ratio(env, admin_signers, ratio)
    }

    pub fn get_max_loan_to_stake_ratio(env: Env) -> u32 {
        admin::get_max_loan_to_stake_ratio(env)
    }

    pub fn get_config(env: Env) -> Config {
        admin::get_config(env)
    }

    pub fn is_blacklisted(env: Env, borrower: Address) -> bool {
        admin::is_blacklisted(env, borrower)
    }

    pub fn is_whitelisted(env: Env, voucher: Address) -> bool {
        admin::is_whitelisted(env, voucher)
    }

    pub fn get_protocol_fee(env: Env) -> u32 {
        admin::get_protocol_fee(env)
    }

    pub fn get_fee_treasury(env: Env) -> Option<Address> {
        admin::get_fee_treasury(env)
    }

    pub fn add_allowed_token(env: Env, admin_signers: Vec<Address>, token: Address) {
        admin::add_allowed_token(env, admin_signers, token)
    }

    pub fn remove_allowed_token(env: Env, admin_signers: Vec<Address>, token: Address) {
        admin::remove_allowed_token(env, admin_signers, token)
    }

    pub fn get_admins(env: Env) -> Vec<Address> {
        admin::get_admins(env)
    }

    // ── Governance & Slashing ────────────────────────────────────────────────

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

    pub fn slash(env: Env, admin_signers: Vec<Address>, borrower: Address) {
        helpers::require_admin_approval(&env, &admin_signers);
        helpers::require_not_paused(&env).expect("contract is paused");

        let mut loan = helpers::get_active_loan_record(&env, &borrower).expect("no active loan");

        if loan.repaid || loan.defaulted {
            panic_with_error!(&env, ContractError::NoActiveLoan);
        }

        let cfg = helpers::config(&env);
        let token = helpers::token(&env);
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        loan.defaulted = true;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan.id), &loan);
        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(borrower.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));

        let dc: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::DefaultCount(borrower.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::DefaultCount(borrower.clone()), &(dc + 1));

        let mut total_slashed: i128 = 0;
        for v in vouches.iter() {
            let slash_amount = v.stake * cfg.slash_bps / 10_000;
            total_slashed += slash_amount;
        }

        helpers::add_slash_balance(&env, total_slashed);

        for v in vouches.iter() {
            let slash_amount = v.stake * cfg.slash_bps / 10_000;
            let returned = v.stake - slash_amount;
            if returned > 0 {
                token.transfer(&env.current_contract_address(), &v.voucher, &returned);
            }
        }

        if let Some(nft_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ReputationNft)
        {
            ReputationNftExternalClient::new(&env, &nft_addr).burn(&borrower);
        }

        env.events().publish(
            (symbol_short!("loan"), symbol_short!("slashed")),
            (borrower.clone(), loan.amount, total_slashed),
        );
    }

    pub fn auto_slash(env: Env, borrower: Address) {
        let mut loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(borrower.clone()))
            .expect("no active loan");

        assert!(!loan.repaid, "loan already repaid");
        assert!(!loan.defaulted, "loan already defaulted");

        let cfg = helpers::config(&env);
        let slash_threshold = loan.deadline.saturating_add(cfg.grace_period);
        assert!(
            env.ledger().timestamp() > slash_threshold,
            "loan grace period has not passed"
        );

        let token = helpers::token(&env);
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        loan.defaulted = true;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(borrower.clone()), &loan);

        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower.clone()));

        let mut total_slash: i128 = 0;
        for v in vouches.iter() {
            total_slash += v.stake * cfg.slash_bps / 10_000;
        }
        helpers::add_slash_balance(&env, total_slash);

        for v in vouches.iter() {
            let slash_amount = v.stake * cfg.slash_bps / 10_000;
            let returned = v.stake - slash_amount;
            if returned > 0 {
                token.transfer(&env.current_contract_address(), &v.voucher, &returned);
            }
        }

        if let Some(nft_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ReputationNft)
        {
            ReputationNftExternalClient::new(&env, &nft_addr).burn(&borrower);
        }
    }

    pub fn claim_expired_loan(env: Env, borrower: Address) {
        borrower.require_auth();

        let mut loan = helpers::get_active_loan_record(&env, &borrower).expect("no active loan");

        if loan.repaid || loan.defaulted {
            panic_with_error!(&env, ContractError::NoActiveLoan);
        }

        let now = env.ledger().timestamp();
        assert!(now >= loan.deadline, "loan has not expired yet");

        let token = helpers::token(&env);
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        for v in vouches.iter() {
            token.transfer(&env.current_contract_address(), &v.voucher, &v.stake);
        }

        loan.defaulted = true;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(loan.id), &loan);

        let dc: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::DefaultCount(borrower.clone()))
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&DataKey::DefaultCount(borrower.clone()), &(dc + 1));

        env.storage()
            .persistent()
            .remove(&DataKey::ActiveLoan(borrower.clone()));
        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower));
    }

    pub fn slash_treasury(env: Env, admin_signers: Vec<Address>, recipient: Address) {
        helpers::require_admin_approval(&env, &admin_signers);

        let amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::SlashTreasury)
            .unwrap_or(0);
        assert!(amount > 0, "no slashed funds to withdraw");

        env.storage()
            .instance()
            .set(&DataKey::SlashTreasury, &0i128);
        helpers::token(&env).transfer(&env.current_contract_address(), &recipient, &amount);
        env.events().publish(
            (symbol_short!("slash"), symbol_short!("withdraw")),
            (recipient, amount),
        );
    }

    // ── Queries ───────────────────────────────────────────────────────────────

    pub fn is_initialized(env: Env) -> bool {
        env.storage().instance().has(&DataKey::Config)
    }

    pub fn get_token(env: Env) -> Address {
        helpers::config(&env).token
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

    pub fn get_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn get_loan(env: Env, borrower: Address) -> Option<LoanRecord> {
        loan::get_loan(env, borrower)
    }

    pub fn get_loan_by_id(env: Env, loan_id: u64) -> Option<LoanRecord> {
        loan::get_loan_by_id(env, loan_id)
    }

    pub fn get_vouches(env: Env, borrower: Address) -> Option<Vec<VouchRecord>> {
        env.storage().persistent().get(&DataKey::Vouches(borrower))
    }

    pub fn get_all_loans(env: Env, page: u32, page_size: u32) -> Vec<LoanRecord> {
        let cfg = helpers::config(&env);
        assert!(cfg.admins.contains(&env.invoker()), "unauthorized");
        assert!(page_size > 0, "page_size must be greater than zero");

        let borrowers: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::BorrowerList)
            .unwrap_or(Vec::new(&env));

        let start = (page as u64).saturating_mul(page_size as u64);
        let end = start.saturating_add(page_size as u64);

        let mut result = Vec::new(&env);
        for i in start..end {
            if i >= borrowers.len() as u64 {
                break;
            }
            let borrower = borrowers.get(i as u32).unwrap();
            if let Some(loan) = helpers::get_latest_loan_record(&env, &borrower) {
                result.push_back(loan);
            }
        }
        result
    }

    pub fn is_eligible(env: Env, borrower: Address, threshold: i128) -> bool {
        loan::is_eligible(env, borrower, threshold)
    }

    pub fn get_contract_balance(env: Env) -> i128 {
        helpers::token(&env).balance(&env.current_contract_address())
    }

    pub fn vouch_exists(env: Env, voucher: Address, borrower: Address) -> bool {
        vouch::vouch_exists(env, voucher, borrower)
    }

    pub fn get_reputation(env: Env, borrower: Address) -> u32 {
        let nft_addr: Address = match env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ReputationNft)
        {
            Some(a) => a,
            None => return 0,
        };
        ReputationNftExternalClient::new(&env, &nft_addr).balance(&borrower)
    }

    pub fn total_vouched(env: Env, borrower: Address) -> Result<i128, ContractError> {
        vouch::total_vouched(env, borrower)
    }

    pub fn repayment_count(env: Env, borrower: Address) -> u32 {
        loan::repayment_count(env, borrower)
    }

    pub fn loan_count(env: Env, borrower: Address) -> u32 {
        loan::loan_count(env, borrower)
    }

    pub fn default_count(env: Env, borrower: Address) -> u32 {
        loan::default_count(env, borrower)
    }

    pub fn loan_status(env: Env, borrower: Address) -> LoanStatus {
        loan::loan_status(env, borrower)
    }

    pub fn voucher_history(env: Env, voucher: Address) -> Vec<Address> {
        vouch::voucher_history(env, voucher)
    }

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

        let cfg = helpers::config(&env);
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

            total_amount = total_amount
                .checked_add(amount)
                .expect("pool total_amount overflow");
        }

        let token = helpers::token(&env);
        let contract_balance = token.balance(&env.current_contract_address());
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

        let empty_purpose = soroban_sdk::String::from_str(&env, "pool");

        for i in 0..borrowers.len() {
            let borrower = borrowers.get(i).unwrap();
            let amount = amounts.get(i).unwrap();
            let loan_id = helpers::next_loan_id(&env);

            env.storage().persistent().set(
                &DataKey::Loan(loan_id),
                &LoanRecord {
                    id: loan_id,
                    borrower: borrower.clone(),
                    co_borrowers: Vec::new(&env),
                    amount,
                    amount_repaid: 0,
                    total_yield: amount * cfg.yield_bps / 10_000,
                    repaid: false,
                    defaulted: false,
                    created_at: now,
                    disbursement_timestamp: now,
                    repayment_timestamp: None,
                    deadline,
                    loan_purpose: empty_purpose.clone(),
                    token_address: cfg.token.clone(),
                },
            );
            env.storage()
                .persistent()
                .set(&DataKey::ActiveLoan(borrower.clone()), &loan_id);
            env.storage()
                .persistent()
                .set(&DataKey::LatestLoan(borrower.clone()), &loan_id);

            token.transfer(&env.current_contract_address(), &borrower, &amount);

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
            .unwrap_or(0u64)
    }

    // ── Insurance Marketplace ────────────────────────────────────────────────

    /// Register a new insurance provider with a quoting strategy.
    /// `adapter_tag` selects the QuoteProvider implementation (e.g. b"static").
    pub fn ins_register_provider(
        env: Env,
        name: soroban_sdk::String,
        adapter_tag: soroban_sdk::Bytes,
    ) -> Result<u64, ContractError> {
        insurance_marketplace::register_provider(env, name, adapter_tag)
    }

    /// Add a coverage product under an existing provider.
    pub fn ins_add_product(
        env: Env,
        provider_id: u64,
        name: soroban_sdk::String,
        coverage_pct_bps: u32,
        premium_bps: u32,
    ) -> Result<u64, ContractError> {
        insurance_marketplace::add_product(env, provider_id, name, coverage_pct_bps, premium_bps)
    }

    /// Compute and store a quote for `borrower` against `product_id`.
    pub fn ins_fetch_quote(
        env: Env,
        product_id: u64,
        borrower: Address,
        loan_amount: i128,
    ) -> Result<u64, ContractError> {
        insurance_marketplace::fetch_quote(env, product_id, borrower, loan_amount)
    }

    /// Accept a previously issued quote (activate coverage).
    pub fn ins_accept_quote(env: Env, quote_id: u64) -> Result<(), ContractError> {
        insurance_marketplace::accept_quote(env, quote_id)
    }

    /// File a claim against an accepted quote.
    pub fn ins_file_claim(env: Env, quote_id: u64) -> Result<u64, ContractError> {
        insurance_marketplace::file_claim(env, quote_id)
    }

    /// Admin: approve a pending claim.
    pub fn ins_approve_claim(env: Env, admin_signers: Vec<Address>, claim_id: u64) -> Result<(), ContractError> {
        insurance_marketplace::approve_claim(env, admin_signers, claim_id)
    }

    /// Admin: reject a pending claim.
    pub fn ins_reject_claim(env: Env, admin_signers: Vec<Address>, claim_id: u64) -> Result<(), ContractError> {
        insurance_marketplace::reject_claim(env, admin_signers, claim_id)
    }

    /// Admin: transfer payout to the borrower for an approved claim.
    pub fn ins_pay_claim(env: Env, admin_signers: Vec<Address>, claim_id: u64) -> Result<(), ContractError> {
        insurance_marketplace::pay_claim(env, admin_signers, claim_id)
    }

    /// Read a provider record.
    pub fn ins_get_provider(env: Env, provider_id: u64) -> Option<InsuranceProvider> {
        insurance_marketplace::get_provider(env, provider_id)
    }

    /// Read a product record.
    pub fn ins_get_product(env: Env, product_id: u64) -> Option<InsuranceProduct> {
        insurance_marketplace::get_product(env, product_id)
    }

    /// Read a quote record.
    pub fn ins_get_quote(env: Env, quote_id: u64) -> Option<InsuranceQuote> {
        insurance_marketplace::get_quote(env, quote_id)
    }

    /// Read a claim record.
    pub fn ins_get_claim(env: Env, claim_id: u64) -> Option<InsuranceClaim> {
        insurance_marketplace::get_claim(env, claim_id)
    }
}
