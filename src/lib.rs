#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, token, Address, Env, Vec,
};

pub mod reputation;
use reputation::ReputationNftContractClient;

// ── Constants ─────────────────────────────────────────────────────────────────

const DEFAULT_YIELD_BPS: i128 = 200;
const DEFAULT_SLASH_BPS: i128 = 5000;
const _: () = assert!(DEFAULT_SLASH_BPS <= 10_000, "DEFAULT_SLASH_BPS must not exceed 10_000");
const DEFAULT_MAX_VOUCHERS: u32 = 100;
const DEFAULT_MIN_LOAN_AMOUNT: i128 = 100_000;
const DEFAULT_LOAN_DURATION: u64 = 30 * 24 * 60 * 60;
const DEFAULT_MAX_LOAN_TO_STAKE_RATIO: u32 = 150;

// ── Errors ────────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    InsufficientFunds = 1,
    DuplicateVouch = 2,
    NoActiveLoan = 3,
    ContractPaused = 4,
    LoanPastDeadline = 5,
    PoolLengthMismatch = 6,
    PoolEmpty = 7,
    PoolBorrowerActiveLoan = 8,
    PoolInsufficientFunds = 9,
    MinStakeNotMet = 10,
    LoanExceedsMaxAmount = 11,
    InsufficientVouchers = 12,
    UnauthorizedCaller = 13,
}

// ── Loan Status ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoanStatus {
    None,
    Active,
    Repaid,
    Defaulted,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Loan(Address),           // borrower → LoanRecord
    Vouches(Address),        // borrower → Vec<VouchRecord>
    VoucherHistory(Address), // voucher  → Vec<Address> (borrowers backed)
    Admin,                   // single admin address (legacy; kept for two-step transfer)
    Token,                   // XLM token contract address
    Deployer,                // Address that deployed the contract; guards initialize
    SlashTreasury,           // i128 accumulated slashed funds
    Paused,                  // bool: true when contract is paused
    ReputationNft,           // Address of the ReputationNftContract
    Config,                  // Config struct: all configurable protocol parameters
    PendingAdmin,            // Address of the pending admin (two-step transfer)
    RepaymentCount(Address), // borrower → u32 total successful repayments
    LoanPool(u64),           // pool_id → LoanPoolRecord
    LoanPoolCounter,         // u64: monotonically increasing pool ID counter
    ProtocolFeeBps,          // u32: protocol fee in basis points
}

// ── Config ────────────────────────────────────────────────────────────────────

/// All configurable protocol parameters, stored under DataKey::Config.
#[contracttype]
#[derive(Clone)]
pub struct Config {
    /// Admin addresses allowed to call privileged functions.
    pub admins: Vec<Address>,
    /// Number of admin signatures required to approve privileged operations.
    pub admin_threshold: u32,
    /// XLM token contract address.
    pub token: Address,
    /// Yield paid to vouchers on repayment in basis points (default 200 = 2%).
    pub yield_bps: i128,
    /// Slash penalty on default in basis points (default 5000 = 50%).
    pub slash_bps: i128,
    /// Maximum number of vouchers per loan (default 100).
    pub max_vouchers: u32,
    /// Minimum loan amount in stroops (default 100_000 = 0.01 XLM).
    pub min_loan_amount: i128,
    /// Loan duration in seconds (default 30 days).
    pub loan_duration: u64,
    /// Maximum loan amount as a percentage of total stake (default 150 = 150%).
    pub max_loan_to_stake_ratio: u32,
    /// Minimum XLM stake required per vouch in stroops (0 = no minimum).
    /// Sybil resistance: forces each voucher to have real economic skin-in-the-game.
    pub min_stake: i128,
    /// Minimum number of distinct vouchers required before a loan can be disbursed (0 = no minimum).
    /// Sybil resistance: prevents a single entity from self-vouching with one address.
    pub min_vouchers: u32,
    /// Maximum individual loan amount in stroops (0 = no cap).
    pub max_loan_amount: i128,
}

// ── Data Types ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct LoanRecord {
    pub borrower: Address,
    pub amount: i128,
    pub amount_repaid: i128,
    pub repaid: bool,
    pub defaulted: bool,
    pub created_at: u64,
    pub disbursement_timestamp: u64,
    pub deadline: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct VouchRecord {
    pub voucher: Address,
    pub stake: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct LoanPoolRecord {
    pub pool_id: u64,
    pub borrowers: Vec<Address>,
    pub amounts: Vec<i128>,
    pub created_at: u64,
    pub total_disbursed: i128,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct QuorumCreditContract;

#[contractimpl]
impl QuorumCreditContract {
    /// One-time initialisation: set admins, XLM token address, and default config.
    ///
    /// `deployer` must be the address that deployed this contract and must sign
    /// this transaction. This prevents front-running attacks.
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
                yield_bps: DEFAULT_YIELD_BPS,
                slash_bps: DEFAULT_SLASH_BPS,
                max_vouchers: DEFAULT_MAX_VOUCHERS,
                min_loan_amount: DEFAULT_MIN_LOAN_AMOUNT,
                loan_duration: DEFAULT_LOAN_DURATION,
                max_loan_to_stake_ratio: DEFAULT_MAX_LOAN_TO_STAKE_RATIO,
                min_stake: 0,
                min_vouchers: 0,
                max_loan_amount: 0,
            },
        );
    }

    /// Stake XLM to vouch for a borrower.
    ///
    /// Sybil resistance is enforced here via two config parameters:
    /// - `min_stake`: each voucher must lock a meaningful economic stake.
    /// - `min_vouchers` (enforced at loan request): a minimum number of
    ///   *distinct* vouchers must back the borrower before a loan is disbursed.
    pub fn vouch(
        env: Env,
        voucher: Address,
        borrower: Address,
        stake: i128,
    ) -> Result<(), ContractError> {
        voucher.require_auth();
        Self::require_not_paused(&env)?;

        assert!(voucher != borrower, "voucher cannot vouch for self");

        let cfg = Self::config(&env);

        // Sybil resistance: enforce minimum stake per vouch.
        if cfg.min_stake > 0 && stake < cfg.min_stake {
            return Err(ContractError::MinStakeNotMet);
        }

        let mut vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        // Reject duplicate vouch before any state mutation or transfer.
        for v in vouches.iter() {
            if v.voucher == voucher {
                return Err(ContractError::DuplicateVouch);
            }
        }

        assert!(
            vouches.len() < cfg.max_vouchers,
            "maximum vouchers per loan exceeded"
        );

        // Transfer stake from voucher into the contract.
        let token = Self::token_client(&env);
        token.transfer(&voucher, &env.current_contract_address(), &stake);

        // Track voucher → borrowers history.
        let mut history: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::VoucherHistory(voucher.clone()))
            .unwrap_or(Vec::new(&env));
        history.push_back(borrower.clone());
        env.storage()
            .persistent()
            .set(&DataKey::VoucherHistory(voucher.clone()), &history);

        vouches.push_back(VouchRecord { voucher: voucher.clone(), stake });
        env.storage()
            .persistent()
            .set(&DataKey::Vouches(borrower.clone()), &vouches);

        env.events().publish(
            (symbol_short!("vouch"), symbol_short!("added")),
            (voucher, borrower, stake),
        );

        Ok(())
    }

    /// Add more stake to an existing vouch for a borrower.
    pub fn increase_stake(
        env: Env,
        voucher: Address,
        borrower: Address,
        additional: i128,
    ) -> Result<(), ContractError> {
        voucher.require_auth();
        Self::require_not_paused(&env)?;

        assert!(additional > 0, "additional stake must be greater than zero");

        let mut vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .expect("vouch not found");

        let idx = vouches
            .iter()
            .position(|v| v.voucher == voucher)
            .expect("vouch not found") as u32;

        let mut vouch = vouches.get(idx).unwrap();
        Self::token_client(&env).transfer(&voucher, &env.current_contract_address(), &additional);

        vouch.stake += additional;
        vouches.set(idx, vouch);

        env.storage()
            .persistent()
            .set(&DataKey::Vouches(borrower), &vouches);

        Ok(())
    }

    /// Disburse a microloan if total vouched stake meets the threshold.
    pub fn request_loan(
        env: Env,
        borrower: Address,
        amount: i128,
        threshold: i128,
    ) -> Result<(), ContractError> {
        borrower.require_auth();
        Self::require_not_paused(&env)?;

        let cfg = Self::config(&env);

        assert!(
            amount >= cfg.min_loan_amount,
            "loan amount must meet minimum threshold"
        );
        assert!(threshold > 0, "threshold must be greater than zero");

        // Enforce max loan amount cap if configured.
        if cfg.max_loan_amount > 0 && amount > cfg.max_loan_amount {
            return Err(ContractError::LoanExceedsMaxAmount);
        }

        // Prevent overwriting an active loan record.
        if let Some(existing) = env
            .storage()
            .persistent()
            .get::<DataKey, LoanRecord>(&DataKey::Loan(borrower.clone()))
        {
            assert!(
                existing.repaid || existing.defaulted,
                "borrower already has an active loan"
            );
        }

        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        let total_stake: i128 = vouches.iter().map(|v| v.stake).sum();
        assert!(total_stake >= threshold, "insufficient trust stake");

        // Sybil resistance: enforce minimum distinct voucher count.
        if cfg.min_vouchers > 0 && vouches.len() < cfg.min_vouchers {
            return Err(ContractError::InsufficientVouchers);
        }

        // Enforce collateral ratio: amount must not exceed total_stake * ratio / 100.
        let max_allowed_loan = total_stake * cfg.max_loan_to_stake_ratio as i128 / 100;
        assert!(
            amount <= max_allowed_loan,
            "loan amount exceeds maximum collateral ratio"
        );

        // Verify the contract holds enough XLM to cover the loan.
        let token = Self::token_client(&env);
        let contract_balance = token.balance(&env.current_contract_address());
        if contract_balance < amount {
            return Err(ContractError::InsufficientFunds);
        }

        let now = env.ledger().timestamp();
        let deadline = now + cfg.loan_duration;

        env.storage().persistent().set(
            &DataKey::Loan(borrower.clone()),
            &LoanRecord {
                borrower: borrower.clone(),
                amount,
                amount_repaid: 0,
                repaid: false,
                defaulted: false,
                created_at: now,
                disbursement_timestamp: now,
                deadline,
            },
        );

        token.transfer(&env.current_contract_address(), &borrower, &amount);

        env.events().publish(
            (symbol_short!("loan"), symbol_short!("disbursed")),
            (borrower.clone(), amount, deadline),
        );

        Ok(())
    }

    /// Borrower repays all or part of the loan.
    ///
    /// When cumulative `amount_repaid` reaches `amount`, the loan is marked
    /// fully repaid and each voucher receives their stake back plus a
    /// proportional share of the yield (proportional to stake / total_stake).
    pub fn repay(env: Env, borrower: Address, payment: i128) -> Result<(), ContractError> {
        borrower.require_auth();
        Self::require_not_paused(&env)?;

        let mut loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(borrower.clone()))
            .ok_or(ContractError::NoActiveLoan)?;

        if borrower != loan.borrower {
            return Err(ContractError::UnauthorizedCaller);
        }
        assert!(!loan.defaulted, "loan already defaulted");
        assert!(!loan.repaid, "loan already repaid");
        assert!(
            env.ledger().timestamp() <= loan.deadline,
            "loan deadline has passed"
        );

        let outstanding = loan.amount - loan.amount_repaid;
        assert!(payment > 0 && payment <= outstanding, "invalid payment amount");

        let token = Self::token_client(&env);
        token.transfer(&borrower, &env.current_contract_address(), &payment);
        loan.amount_repaid += payment;

        if loan.amount_repaid >= loan.amount {
            // Fully repaid — distribute stake + proportional yield to each voucher.
            let cfg = Self::config(&env);
            let vouches: Vec<VouchRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::Vouches(borrower.clone()))
                .unwrap_or(Vec::new(&env));

            let total_stake: i128 = vouches.iter().map(|v| v.stake).sum();
            // Total yield pool = loan.amount * yield_bps / 10_000
            let total_yield = loan.amount * cfg.yield_bps / 10_000;

            for v in vouches.iter() {
                let voucher_yield = if total_stake > 0 {
                    total_yield * v.stake / total_stake
                } else {
                    0
                };
                token.transfer(
                    &env.current_contract_address(),
                    &v.voucher,
                    &(v.stake + voucher_yield),
                );
            }

            loan.repaid = true;

            // Increment successful repayment count.
            let count: u32 = env
                .storage()
                .persistent()
                .get(&DataKey::RepaymentCount(borrower.clone()))
                .unwrap_or(0);
            env.storage()
                .persistent()
                .set(&DataKey::RepaymentCount(borrower.clone()), &(count + 1));

            // Mint one reputation point if a reputation NFT contract is configured.
            if let Some(nft_addr) = env
                .storage()
                .instance()
                .get::<DataKey, Address>(&DataKey::ReputationNft)
            {
                ReputationNftContractClient::new(&env, &nft_addr).mint(&borrower);
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::Loan(borrower.clone()), &loan);

        Ok(())
    }

    /// Admin marks a loan defaulted; slash_bps% of each voucher's stake is burned.
    pub fn slash(env: Env, admin_signers: Vec<Address>, borrower: Address) {
        Self::require_admin_approval(&env, &admin_signers);
        Self::require_not_paused(&env).expect("contract is paused");

        let mut loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(borrower.clone()))
            .expect("no active loan");

        assert!(!loan.repaid, "loan already repaid");
        assert!(!loan.defaulted, "already defaulted");

        let token = Self::token_client(&env);
        let cfg = Self::config(&env);
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        for v in vouches.iter() {
            let slash_amount = v.stake * cfg.slash_bps / 10_000;
            let returned = v.stake - slash_amount;
            if returned > 0 {
                token.transfer(&env.current_contract_address(), &v.voucher, &returned);
            }
            let treasury: i128 = env
                .storage()
                .instance()
                .get(&DataKey::SlashTreasury)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::SlashTreasury, &(treasury + slash_amount));
        }

        loan.defaulted = true;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(borrower.clone()), &loan);

        if let Some(nft_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ReputationNft)
        {
            ReputationNftContractClient::new(&env, &nft_addr).burn(&borrower);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower));
    }

    /// Callable by anyone after the loan deadline has passed.
    /// Applies the standard slash penalty.
    pub fn auto_slash(env: Env, borrower: Address) {
        let mut loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(borrower.clone()))
            .expect("no active loan");

        assert!(!loan.repaid, "loan already repaid");
        assert!(!loan.defaulted, "loan already defaulted");
        assert!(
            env.ledger().timestamp() > loan.deadline,
            "loan deadline has not passed"
        );

        let token = Self::token_client(&env);
        let cfg = Self::config(&env);
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        for v in vouches.iter() {
            let slash_amount = v.stake * cfg.slash_bps / 10_000;
            let returned = v.stake - slash_amount;
            if returned > 0 {
                token.transfer(&env.current_contract_address(), &v.voucher, &returned);
            }
            let treasury: i128 = env
                .storage()
                .instance()
                .get(&DataKey::SlashTreasury)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::SlashTreasury, &(treasury + slash_amount));
        }

        loan.defaulted = true;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(borrower.clone()), &loan);

        if let Some(nft_addr) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::ReputationNft)
        {
            ReputationNftContractClient::new(&env, &nft_addr).burn(&borrower);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower));
    }

    /// Allows vouchers to claim back their stake if loan has expired without repayment or slash.
    pub fn claim_expired_loan(env: Env, borrower: Address) {
        let loan: LoanRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Loan(borrower.clone()))
            .expect("no active loan");

        assert!(!loan.repaid, "loan already repaid");
        assert!(!loan.defaulted, "loan already defaulted");

        let now = env.ledger().timestamp();
        assert!(now >= loan.deadline, "loan has not expired yet");

        let token = Self::token_client(&env);
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .unwrap_or(Vec::new(&env));

        for v in vouches.iter() {
            token.transfer(&env.current_contract_address(), &v.voucher, &v.stake);
        }

        let mut loan = loan;
        loan.defaulted = true;
        env.storage()
            .persistent()
            .set(&DataKey::Loan(borrower.clone()), &loan);

        env.storage()
            .persistent()
            .remove(&DataKey::Vouches(borrower));
    }

    /// Admin withdraws accumulated slashed funds to a recipient address.
    pub fn slash_treasury(env: Env, admin_signers: Vec<Address>, recipient: Address) {
        Self::require_admin_approval(&env, &admin_signers);

        let amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::SlashTreasury)
            .unwrap_or(0);
        assert!(amount > 0, "no slashed funds to withdraw");

        env.storage()
            .instance()
            .set(&DataKey::SlashTreasury, &0i128);
        Self::token_client(&env).transfer(&env.current_contract_address(), &recipient, &amount);
    }

    /// Withdraw a vouch before any loan is active, returning the exact stake to the voucher.
    pub fn withdraw_vouch(env: Env, voucher: Address, borrower: Address) {
        voucher.require_auth();

        if let Some(loan) = env
            .storage()
            .persistent()
            .get::<DataKey, LoanRecord>(&DataKey::Loan(borrower.clone()))
        {
            assert!(
                loan.repaid || loan.defaulted,
                "loan already active"
            );
        }

        let mut vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower.clone()))
            .expect("vouch not found");

        let idx = vouches
            .iter()
            .position(|v| v.voucher == voucher)
            .expect("vouch not found") as u32;

        let stake = vouches.get(idx).unwrap().stake;
        vouches.remove(idx);

        if vouches.is_empty() {
            env.storage()
                .persistent()
                .remove(&DataKey::Vouches(borrower));
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::Vouches(borrower), &vouches);
        }

        Self::token_client(&env).transfer(&env.current_contract_address(), &voucher, &stake);
    }

    // ── Admin: Config ─────────────────────────────────────────────────────────

    /// Admin updates configurable protocol parameters.
    pub fn set_config(env: Env, config: Config) {
        Self::require_single_admin(&env);
        assert!(config.yield_bps >= 0, "yield_bps must be non-negative");
        assert!(
            config.slash_bps > 0 && config.slash_bps <= 10_000,
            "slash_bps must be 1-10000"
        );
        assert!(config.max_vouchers > 0, "max_vouchers must be greater than zero");
        assert!(config.min_loan_amount > 0, "min_loan_amount must be greater than zero");
        assert!(config.loan_duration > 0, "loan_duration must be greater than zero");
        assert!(
            config.max_loan_to_stake_ratio > 0,
            "max_loan_to_stake_ratio must be greater than zero"
        );
        assert!(config.min_stake >= 0, "min_stake cannot be negative");
        assert!(config.max_loan_amount >= 0, "max_loan_amount cannot be negative");
        Self::validate_admin_config(&config.admins, config.admin_threshold);
        env.storage().instance().set(&DataKey::Config, &config);
    }

    /// Returns the current protocol config.
    pub fn get_config(env: Env) -> Config {
        Self::config(&env)
    }

    // ── Admin: Protocol Fee ───────────────────────────────────────────────────

    /// Admin sets the protocol fee applied to interactions (in basis points).
    pub fn set_protocol_fee(env: Env, admin_signers: Vec<Address>, fee_bps: u32) {
        Self::require_admin_approval(&env, &admin_signers);
        assert!(fee_bps <= 10_000, "fee_bps must not exceed 10000");
        env.storage()
            .instance()
            .set(&DataKey::ProtocolFeeBps, &fee_bps);
    }

    /// Returns the current protocol fee (0 if not set).
    pub fn get_protocol_fee(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::ProtocolFeeBps)
            .unwrap_or(0)
    }

    // ── Admin: Pause / Unpause ────────────────────────────────────────────────

    pub fn pause(env: Env, admin_signers: Vec<Address>) {
        Self::require_admin_approval(&env, &admin_signers);
        env.storage().instance().set(&DataKey::Paused, &true);
    }

    pub fn unpause(env: Env, admin_signers: Vec<Address>) {
        Self::require_admin_approval(&env, &admin_signers);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    // ── Two-Step Admin Transfer ───────────────────────────────────────────────

    /// Step 1: Current admin proposes a new admin address.
    pub fn propose_admin(env: Env, new_admin: Address) {
        Self::require_single_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        let old_admin = Self::config(&env).admins.get(0).unwrap();
        env.events().publish(("AdminProposed",), (old_admin, new_admin));
    }

    /// Step 2: Pending admin accepts the transfer, becoming the new admin.
    pub fn accept_admin(env: Env) {
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .expect("no pending admin");
        pending.require_auth();

        let mut cfg = Self::config(&env);
        let old_admin = cfg.admins.get(0).unwrap();

        // Replace the first admin with the pending admin.
        let mut new_admins = Vec::new(&env);
        new_admins.push_back(pending.clone());
        for i in 1..cfg.admins.len() {
            new_admins.push_back(cfg.admins.get(i).unwrap());
        }
        cfg.admins = new_admins;
        env.storage().instance().set(&DataKey::Config, &cfg);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.events().publish(("AdminUpdated",), (old_admin, pending));
    }

    // ── Admin: Reputation NFT ─────────────────────────────────────────────────

    pub fn set_reputation_nft(env: Env, nft_contract: Address) {
        Self::require_single_admin(&env);
        env.storage()
            .instance()
            .set(&DataKey::ReputationNft, &nft_contract);
    }

    // ── Loan Pool ─────────────────────────────────────────────────────────────

    /// Admin function: atomically disburse a batch of small loans to multiple borrowers.
    pub fn create_loan_pool(
        env: Env,
        borrowers: Vec<Address>,
        amounts: Vec<i128>,
    ) -> Result<u64, ContractError> {
        Self::require_single_admin(&env);

        if borrowers.len() != amounts.len() {
            return Err(ContractError::PoolLengthMismatch);
        }
        if borrowers.is_empty() {
            return Err(ContractError::PoolEmpty);
        }

        let cfg = Self::config(&env);
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

            if let Some(existing) = env
                .storage()
                .persistent()
                .get::<DataKey, LoanRecord>(&DataKey::Loan(borrower.clone()))
            {
                if !existing.repaid && !existing.defaulted {
                    return Err(ContractError::PoolBorrowerActiveLoan);
                }
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

        let token = Self::token_client(&env);
        let contract_balance = token.balance(&env.current_contract_address());
        if contract_balance < total_amount {
            return Err(ContractError::PoolInsufficientFunds);
        }

        let pool_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::LoanPoolCounter)
            .unwrap_or(0u64)
            + 1;
        env.storage()
            .instance()
            .set(&DataKey::LoanPoolCounter, &pool_id);

        for i in 0..borrowers.len() {
            let borrower = borrowers.get(i).unwrap();
            let amount = amounts.get(i).unwrap();

            env.storage().persistent().set(
                &DataKey::Loan(borrower.clone()),
                &LoanRecord {
                    borrower: borrower.clone(),
                    amount,
                    amount_repaid: 0,
                    repaid: false,
                    defaulted: false,
                    created_at: now,
                    disbursement_timestamp: now,
                    deadline,
                },
            );

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
        env.storage()
            .persistent()
            .get(&DataKey::LoanPool(pool_id))
    }

    pub fn get_loan_pool_count(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::LoanPoolCounter)
            .unwrap_or(0)
    }

    // ── Views ─────────────────────────────────────────────────────────────────

    pub fn is_initialized(env: Env) -> bool {
        env.storage().instance().has(&DataKey::Config)
    }

    pub fn get_token(env: Env) -> Address {
        Self::config(&env).token
    }

    pub fn get_admins(env: Env) -> Vec<Address> {
        Self::config(&env).admins
    }

    pub fn get_admin_threshold(env: Env) -> u32 {
        Self::config(&env).admin_threshold
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::PendingAdmin)
    }

    pub fn get_slash_treasury(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::SlashTreasury)
            .unwrap_or(0)
    }

    /// Returns the total accumulated slashed funds held in the treasury.
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

    pub fn loan_status(env: Env, borrower: Address) -> LoanStatus {
        match env
            .storage()
            .persistent()
            .get::<DataKey, LoanRecord>(&DataKey::Loan(borrower))
        {
            None => LoanStatus::None,
            Some(loan) if loan.repaid => LoanStatus::Repaid,
            Some(loan) if loan.defaulted => LoanStatus::Defaulted,
            _ => LoanStatus::Active,
        }
    }

    pub fn vouch_exists(env: Env, voucher: Address, borrower: Address) -> bool {
        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower))
            .unwrap_or(Vec::new(&env));
        vouches.iter().any(|v| v.voucher == voucher)
    }

    pub fn get_loan(env: Env, borrower: Address) -> Option<LoanRecord> {
        env.storage().persistent().get(&DataKey::Loan(borrower))
    }

    pub fn get_vouches(env: Env, borrower: Address) -> Option<Vec<VouchRecord>> {
        env.storage().persistent().get(&DataKey::Vouches(borrower))
    }

    /// Read-only eligibility check for frontends — no transaction required.
    pub fn is_eligible(env: Env, borrower: Address, threshold: i128) -> bool {
        if threshold <= 0 {
            return false;
        }

        if let Some(loan) = env
            .storage()
            .persistent()
            .get::<DataKey, LoanRecord>(&DataKey::Loan(borrower.clone()))
        {
            if !loan.repaid && !loan.defaulted {
                return false;
            }
        }

        let vouches: Vec<VouchRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Vouches(borrower))
            .unwrap_or(Vec::new(&env));

        let total_stake: i128 = vouches.iter().map(|v| v.stake).sum();
        total_stake >= threshold
    }

    pub fn get_contract_balance(env: Env) -> i128 {
        Self::token_client(&env).balance(&env.current_contract_address())
    }

    pub fn voucher_history(env: Env, voucher: Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::VoucherHistory(voucher))
            .unwrap_or(Vec::new(&env))
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
        ReputationNftContractClient::new(&env, &nft_addr).balance(&borrower)
    }

    pub fn total_vouched(env: Env, borrower: Address) -> i128 {
        env.storage()
            .persistent()
            .get::<DataKey, Vec<VouchRecord>>(&DataKey::Vouches(borrower))
            .unwrap_or(Vec::new(&env))
            .iter()
            .map(|v| v.stake)
            .sum()
    }

    pub fn repayment_count(env: Env, borrower: Address) -> u32 {
        env.storage()
            .persistent()
            .get(&DataKey::RepaymentCount(borrower))
            .unwrap_or(0)
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn require_not_paused(env: &Env) -> Result<(), ContractError> {
        let paused: bool = env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false);
        if paused {
            Err(ContractError::ContractPaused)
        } else {
            Ok(())
        }
    }

    fn config(env: &Env) -> Config {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .expect("not initialized")
    }

    fn token_client(env: &Env) -> token::Client<'_> {
        let addr = Self::config(env).token;
        token::Client::new(env, &addr)
    }

    /// Require that the first admin in the config has signed (single-admin operations).
    fn require_single_admin(env: &Env) {
        let cfg = Self::config(env);
        let admin = cfg.admins.get(0).expect("no admin configured");
        admin.require_auth();
    }

    fn require_admin_approval(env: &Env, admin_signers: &Vec<Address>) {
        let config = Self::config(env);
        assert!(
            admin_signers.len() >= config.admin_threshold,
            "insufficient admin approvals"
        );

        let signer_count = admin_signers.len();
        for i in 0..signer_count {
            let signer = admin_signers.get(i).unwrap();

            for j in 0..i {
                let prior_signer = admin_signers.get(j).unwrap();
                assert!(signer != prior_signer, "duplicate admin signer");
            }

            let mut is_admin = false;
            for admin in config.admins.iter() {
                if admin == signer {
                    is_admin = true;
                    break;
                }
            }

            assert!(is_admin, "unauthorized admin signer");
            signer.require_auth();
        }
    }

    fn validate_admin_config(admins: &Vec<Address>, admin_threshold: u32) {
        assert!(!admins.is_empty(), "at least one admin is required");
        assert!(admin_threshold > 0, "admin threshold must be greater than zero");
        assert!(
            admin_threshold <= admins.len(),
            "admin threshold cannot exceed admin count"
        );

        let admin_count = admins.len();
        for i in 0..admin_count {
            let admin = admins.get(i).unwrap();
            for j in 0..i {
                let prior_admin = admins.get(j).unwrap();
                assert!(admin != prior_admin, "duplicate admin");
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn setup() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let token_admin = Address::generate(&env);

        // Deploy a mock Stellar asset token.
        let token_id = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_addr = token_id.address();

        // Deploy the QuorumCredit contract.
        let contract_id = env.register(QuorumCreditContract, ());

        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let mut admins = Vec::new(&env);
        admins.push_back(admin.clone());
        client.initialize(&admin, &admins, &1u32, &token_addr);

        (env, contract_id, admin, token_addr, token_admin)
    }

    /// Mint `amount` tokens to `recipient` using the asset admin.
    fn mint(env: &Env, token_admin: &Address, token_addr: &Address, recipient: &Address, amount: i128) {
        StellarAssetClient::new(env, token_addr).mint(recipient, &amount);
    }

    // ── Sybil Resistance: min_stake ───────────────────────────────────────────

    #[test]
    fn test_vouch_below_min_stake_rejected() {
        let (env, contract_id, admin, token_addr, token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Set min_stake to 1_000_000 stroops.
        let mut cfg = client.get_config();
        cfg.min_stake = 1_000_000;
        client.set_config(&cfg);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        mint(&env, &token_admin, &token_addr, &voucher, 500_000);

        // Stake of 500_000 is below min_stake of 1_000_000 — must be rejected.
        let result = client.try_vouch(&voucher, &borrower, &500_000i128);
        assert_eq!(result, Err(Ok(ContractError::MinStakeNotMet)));
    }

    #[test]
    fn test_vouch_at_min_stake_accepted() {
        let (env, contract_id, admin, token_addr, token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let mut cfg = client.get_config();
        cfg.min_stake = 1_000_000;
        client.set_config(&cfg);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        mint(&env, &token_admin, &token_addr, &voucher, 1_000_000);

        // Exactly at min_stake — must succeed.
        client.vouch(&voucher, &borrower, &1_000_000i128);
        assert!(client.vouch_exists(&voucher, &borrower));
    }

    // ── Sybil Resistance: min_vouchers ────────────────────────────────────────

    #[test]
    fn test_loan_rejected_when_below_min_vouchers() {
        let (env, contract_id, admin, token_addr, token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let mut cfg = client.get_config();
        cfg.min_vouchers = 3;
        client.set_config(&cfg);

        let borrower = Address::generate(&env);

        // Only 2 vouchers — below the required 3.
        for _ in 0..2 {
            let voucher = Address::generate(&env);
            mint(&env, &token_admin, &token_addr, &voucher, 500_000);
            client.vouch(&voucher, &borrower, &500_000i128);
        }

        // Pre-fund contract so balance isn't the limiting factor.
        mint(&env, &token_admin, &token_addr, &contract_id, 10_000_000);

        let result = client.try_request_loan(&borrower, &100_000i128, &500_000i128);
        assert_eq!(result, Err(Ok(ContractError::InsufficientVouchers)));
    }

    #[test]
    fn test_loan_approved_with_sufficient_vouchers() {
        let (env, contract_id, admin, token_addr, token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let mut cfg = client.get_config();
        cfg.min_vouchers = 2;
        client.set_config(&cfg);

        let borrower = Address::generate(&env);
        let mut total_stake: i128 = 0;

        for _ in 0..2 {
            let voucher = Address::generate(&env);
            mint(&env, &token_admin, &token_addr, &voucher, 500_000);
            client.vouch(&voucher, &borrower, &500_000i128);
            total_stake += 500_000;
        }

        mint(&env, &token_admin, &token_addr, &contract_id, 10_000_000);

        // 2 vouchers meets min_vouchers = 2 — loan must be disbursed.
        client.request_loan(&borrower, &100_000i128, &total_stake);
        let loan = client.get_loan(&borrower).unwrap();
        assert!(!loan.repaid && !loan.defaulted);
    }

    // ── Sybil Resistance: self-vouch prevention ───────────────────────────────

    #[test]
    #[should_panic(expected = "voucher cannot vouch for self")]
    fn test_self_vouch_rejected() {
        let (env, contract_id, _admin, token_addr, token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let actor = Address::generate(&env);
        mint(&env, &token_admin, &token_addr, &actor, 1_000_000);

        // Same address as both voucher and borrower — must panic.
        client.vouch(&actor, &actor, &1_000_000i128);
    }

    // ── Sybil Resistance: duplicate vouch prevention ──────────────────────────

    #[test]
    fn test_duplicate_vouch_rejected() {
        let (env, contract_id, _admin, token_addr, token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        mint(&env, &token_admin, &token_addr, &voucher, 2_000_000);

        client.vouch(&voucher, &borrower, &1_000_000i128);

        // Second vouch from the same address must be rejected.
        let result = client.try_vouch(&voucher, &borrower, &1_000_000i128);
        assert_eq!(result, Err(Ok(ContractError::DuplicateVouch)));
    }

    // ── set_config validates admin config ─────────────────────────────────────

    #[test]
    #[should_panic(expected = "admin threshold cannot exceed admin count")]
    fn test_set_config_invalid_admin_threshold_rejected() {
        let (env, contract_id, _admin, _token_addr, _token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let mut cfg = client.get_config();
        // threshold (5) > admins count (1) — must panic.
        cfg.admin_threshold = 5;
        client.set_config(&cfg);
    }

    // ── withdraw_vouch blocked during active loan ─────────────────────────────

    #[test]
    #[should_panic(expected = "loan already active")]
    fn test_withdraw_vouch_blocked_during_active_loan() {
        let (env, contract_id, _admin, token_addr, token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        mint(&env, &token_admin, &token_addr, &voucher, 1_000_000);
        mint(&env, &token_admin, &token_addr, &contract_id, 10_000_000);

        client.vouch(&voucher, &borrower, &1_000_000i128);
        client.request_loan(&borrower, &100_000i128, &1_000_000i128);

        // Loan is active — withdrawal must be blocked.
        client.withdraw_vouch(&voucher, &borrower);
    }

    // ── get_slash_treasury_balance ────────────────────────────────────────────

    #[test]
    fn test_get_slash_treasury_balance_accumulates_on_slash() {
        let (env, contract_id, admin, token_addr, token_admin) = setup();
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Starts at zero before any slash.
        assert_eq!(client.get_slash_treasury_balance(), 0);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);
        mint(&env, &token_admin, &token_addr, &voucher, 1_000_000);
        mint(&env, &token_admin, &token_addr, &contract_id, 10_000_000);

        client.vouch(&voucher, &borrower, &1_000_000i128);
        client.request_loan(&borrower, &100_000i128, &1_000_000i128);

        let mut admin_signers = Vec::new(&env);
        admin_signers.push_back(admin.clone());
        client.slash(&admin_signers, &borrower);

        // slash_bps = 5000 (50%) of 1_000_000 stake = 500_000 slashed.
        assert_eq!(client.get_slash_treasury_balance(), 500_000);
    }
}
