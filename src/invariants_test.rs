//! # Contract Invariant Enforcement
//!
//! This module implements `verify_invariants`, the single function called after
//! every state-changing operation in the test suite.  It checks all 8 documented
//! invariants (I1–I8) plus several additional ones discovered during this work.
//!
//! ## Invariant catalogue
//!
//! | ID  | Name                              | Checked here |
//! |-----|-----------------------------------|--------------|
//! | I1  | Solvency: balance ≥ locked stake  | ✓            |
//! | I2  | Loan ≤ total vouched stake        | ✓            |
//! | I3  | No active loan without vouches    | ✓            |
//! | I4  | amount_repaid ≤ amount + yield    | ✓            |
//! | I5  | Loan status transitions monotonic | ✓            |
//! | I6  | Slash treasury ≥ 0                | ✓            |
//! | I7  | yield_bps in [0, 10_000]          | ✓            |
//! | I8  | 1 ≤ admin_threshold ≤ admins.len  | ✓            |
//! | I9  | slash_bps in [0, 10_000]          | ✓            |
//! | I10 | All stake values ≥ 0              | ✓            |
//! | I11 | Active-loan pointer consistency   | ✓            |
//! | I12 | min_loan_amount > 0               | ✓            |

#![cfg(test)]
#![allow(dead_code)]

use crate::types::{DataKey, LoanStatus};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{self, StellarAssetClient},
    Address, Env, Vec,
};

// ── InvariantViolation ────────────────────────────────────────────────────────

/// Returned when any invariant is found to be violated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvariantViolation {
    /// Short invariant identifier, e.g. "I1", "I4".
    pub id: &'static str,
    /// Human-readable description of the violation.
    pub message: soroban_sdk::String,
}

impl InvariantViolation {
    fn new(env: &Env, id: &'static str, msg: &str) -> Self {
        InvariantViolation {
            id,
            message: soroban_sdk::String::from_str(env, msg),
        }
    }
}

// ── verify_invariants ─────────────────────────────────────────────────────────

/// Assert every known invariant against live contract state.
///
/// # Parameters
/// - `env`         – the test `Env`
/// - `contract_id` – the deployed `QuorumCreditContract` address
/// - `token`       – the primary token address (used for balance checks)
/// - `borrowers`   – slice of all borrower addresses active in this test scenario
///
/// # Returns
/// `Ok(())` when all invariants hold; `Err(InvariantViolation)` for the first
/// violation detected (fail-fast).
///
/// # Usage
/// ```ignore
/// verify_invariants(&env, &contract_id, &token, &[&borrower]).unwrap();
/// ```
pub fn verify_invariants(
    env: &Env,
    contract_id: &Address,
    token: &Address,
    borrowers: &[&Address],
) -> Result<(), InvariantViolation> {
    check_i6_slash_treasury_non_negative(env, contract_id)?;
    check_i7_yield_bps_range(env, contract_id)?;
    check_i8_admin_threshold(env, contract_id)?;
    check_i9_slash_bps_range(env, contract_id)?;
    check_i12_min_loan_amount_positive(env, contract_id)?;

    // Per-borrower checks
    let mut total_locked_stake: i128 = 0;
    for borrower in borrowers {
        let borrower_stake = check_per_borrower(env, contract_id, token, borrower)?;
        total_locked_stake += borrower_stake;
    }

    // I1: contract token balance ≥ sum of all active voucher stakes
    check_i1_solvency(env, contract_id, token, total_locked_stake)?;

    Ok(())
}

// ── Per-borrower check bundle ─────────────────────────────────────────────────

/// Run all per-borrower invariant checks and return the total locked stake for
/// this borrower so the caller can accumulate it for I1.
fn check_per_borrower(
    env: &Env,
    contract_id: &Address,
    token: &Address,
    borrower: &Address,
) -> Result<i128, InvariantViolation> {
    let vouches: soroban_sdk::Vec<crate::types::VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches((*borrower).clone()))
        .unwrap_or_else(|| soroban_sdk::Vec::new(env));

    let loan_opt: Option<crate::types::LoanRecord> = {
        let loan_id_opt: Option<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::ActiveLoan((*borrower).clone()));
        loan_id_opt.and_then(|id| {
            env.storage()
                .persistent()
                .get::<_, crate::types::LoanRecord>(&DataKey::Loan(id))
        })
    };

    // Accumulate primary-token stake that is currently locked in this contract.
    let mut locked_stake: i128 = 0;
    for v in vouches.iter() {
        check_i10_stake_non_negative(env, &v, borrower)?;
        if &v.token == token {
            locked_stake += v.stake;
        }
    }

    if let Some(ref loan) = loan_opt {
        check_i2_loan_le_stake(env, loan, locked_stake, borrower)?;
        check_i3_no_active_loan_without_vouches(env, loan, &vouches, borrower)?;
        check_i4_repaid_le_principal_plus_yield(env, loan, borrower)?;
        check_i5_status_monotonic(env, loan, borrower)?;
        check_i11_active_loan_pointer_consistent(env, loan, borrower)?;
    }

    Ok(locked_stake)
}

// ── Individual invariant checks ───────────────────────────────────────────────

/// I1 – Contract token balance ≥ total locked voucher stake.
fn check_i1_solvency(
    env: &Env,
    contract_id: &Address,
    token: &Address,
    total_locked_stake: i128,
) -> Result<(), InvariantViolation> {
    let balance = token::Client::new(env, token).balance(contract_id);
    if balance < total_locked_stake {
        return Err(InvariantViolation::new(
            env,
            "I1",
            "contract balance is less than total locked voucher stake (solvency violated)",
        ));
    }
    Ok(())
}

/// I2 – Active loan amount ≤ total vouched stake for this borrower.
fn check_i2_loan_le_stake(
    env: &Env,
    loan: &crate::types::LoanRecord,
    locked_stake: i128,
    borrower: &Address,
) -> Result<(), InvariantViolation> {
    if loan.status == LoanStatus::Active && locked_stake > 0 && loan.amount > locked_stake {
        return Err(InvariantViolation::new(
            env,
            "I2",
            "loan amount exceeds total vouched stake at disbursement",
        ));
    }
    Ok(())
}

/// I3 – A borrower cannot have an active loan with zero vouches.
fn check_i3_no_active_loan_without_vouches(
    env: &Env,
    loan: &crate::types::LoanRecord,
    vouches: &soroban_sdk::Vec<crate::types::VouchRecord>,
    _borrower: &Address,
) -> Result<(), InvariantViolation> {
    if loan.status == LoanStatus::Active && vouches.is_empty() {
        return Err(InvariantViolation::new(
            env,
            "I3",
            "borrower has an active loan but zero vouches on record",
        ));
    }
    Ok(())
}

/// I4 – Cumulative amount_repaid ≤ amount + total_yield.
fn check_i4_repaid_le_principal_plus_yield(
    env: &Env,
    loan: &crate::types::LoanRecord,
    _borrower: &Address,
) -> Result<(), InvariantViolation> {
    let max_repayment = loan.amount.saturating_add(loan.total_yield);
    if loan.amount_repaid > max_repayment {
        return Err(InvariantViolation::new(
            env,
            "I4",
            "amount_repaid exceeds principal + yield (double-repayment or over-payment)",
        ));
    }
    Ok(())
}

/// I5 – Loan status is one of the valid terminal or active states (no regressions).
/// Valid forward transitions: None → Active → {Repaid, PartialDefault, Defaulted, ForgivenDefault}.
/// We detect regressions by checking that a Repaid/Defaulted loan still has a
/// consistent `repaid`/`defaulted` flag.
fn check_i5_status_monotonic(
    env: &Env,
    loan: &crate::types::LoanRecord,
    _borrower: &Address,
) -> Result<(), InvariantViolation> {
    // A loan that is Repaid must have repaid=true
    if loan.status == LoanStatus::Repaid && !loan.repaid {
        return Err(InvariantViolation::new(
            env,
            "I5",
            "loan status is Repaid but the repaid flag is false (status regression)",
        ));
    }
    // A loan that is Defaulted must have defaulted=true
    if loan.status == LoanStatus::Defaulted && !loan.defaulted {
        return Err(InvariantViolation::new(
            env,
            "I5",
            "loan status is Defaulted but the defaulted flag is false (status regression)",
        ));
    }
    // A None-status loan should never appear in the active-loan storage
    if loan.status == LoanStatus::None {
        return Err(InvariantViolation::new(
            env,
            "I5",
            "loan record has status None but is stored as an active loan (invalid state)",
        ));
    }
    Ok(())
}

/// I6 – Slash treasury balance is non-negative.
fn check_i6_slash_treasury_non_negative(
    env: &Env,
    _contract_id: &Address,
) -> Result<(), InvariantViolation> {
    let treasury: i128 = env
        .storage()
        .instance()
        .get(&DataKey::SlashTreasury)
        .unwrap_or(0);
    if treasury < 0 {
        return Err(InvariantViolation::new(
            env,
            "I6",
            "slash treasury balance is negative (arithmetic underflow in slash accounting)",
        ));
    }
    Ok(())
}

/// I7 – yield_bps is in [0, 10_000].
fn check_i7_yield_bps_range(
    env: &Env,
    _contract_id: &Address,
) -> Result<(), InvariantViolation> {
    if let Some(cfg) = env
        .storage()
        .instance()
        .get::<_, crate::types::Config>(&DataKey::Config)
    {
        if cfg.yield_bps < 0 || cfg.yield_bps > 10_000 {
            return Err(InvariantViolation::new(
                env,
                "I7",
                "yield_bps is outside the valid range [0, 10_000]",
            ));
        }
    }
    Ok(())
}

/// I8 – 1 ≤ admin_threshold ≤ admins.len().
fn check_i8_admin_threshold(
    env: &Env,
    _contract_id: &Address,
) -> Result<(), InvariantViolation> {
    if let Some(cfg) = env
        .storage()
        .instance()
        .get::<_, crate::types::Config>(&DataKey::Config)
    {
        let n = cfg.admins.len() as u32;
        if cfg.admin_threshold == 0 || cfg.admin_threshold > n {
            return Err(InvariantViolation::new(
                env,
                "I8",
                "admin_threshold is outside the valid range [1, admins.len()]",
            ));
        }
    }
    Ok(())
}

/// I9 – slash_bps is in [0, 10_000].
fn check_i9_slash_bps_range(
    env: &Env,
    _contract_id: &Address,
) -> Result<(), InvariantViolation> {
    if let Some(cfg) = env
        .storage()
        .instance()
        .get::<_, crate::types::Config>(&DataKey::Config)
    {
        if cfg.slash_bps < 0 || cfg.slash_bps > 10_000 {
            return Err(InvariantViolation::new(
                env,
                "I9",
                "slash_bps is outside the valid range [0, 10_000]",
            ));
        }
    }
    Ok(())
}

/// I10 – Every VouchRecord.stake must be ≥ 0.
fn check_i10_stake_non_negative(
    env: &Env,
    vouch: &crate::types::VouchRecord,
    _borrower: &Address,
) -> Result<(), InvariantViolation> {
    if vouch.stake < 0 {
        return Err(InvariantViolation::new(
            env,
            "I10",
            "VouchRecord.stake is negative (stake underflow)",
        ));
    }
    Ok(())
}

/// I11 – If DataKey::ActiveLoan(borrower) exists, the pointed-to LoanRecord
///        must have status == Active.
fn check_i11_active_loan_pointer_consistent(
    env: &Env,
    loan: &crate::types::LoanRecord,
    _borrower: &Address,
) -> Result<(), InvariantViolation> {
    if loan.status != LoanStatus::Active {
        // The ActiveLoan pointer should have been removed for terminal states.
        return Err(InvariantViolation::new(
            env,
            "I11",
            "ActiveLoan pointer exists for a loan that is not in Active status (stale pointer)",
        ));
    }
    Ok(())
}

/// I12 – min_loan_amount must be > 0.
fn check_i12_min_loan_amount_positive(
    env: &Env,
    _contract_id: &Address,
) -> Result<(), InvariantViolation> {
    if let Some(cfg) = env
        .storage()
        .instance()
        .get::<_, crate::types::Config>(&DataKey::Config)
    {
        if cfg.min_loan_amount <= 0 {
            return Err(InvariantViolation::new(
                env,
                "I12",
                "min_loan_amount is zero or negative (invalid protocol configuration)",
            ));
        }
    }
    Ok(())
}

// ── Test-environment setup helper ─────────────────────────────────────────────

/// Shared setup used by every invariant test:
/// - Registers the contract
/// - Mints tokens so the contract can disburse loans + yield
/// - Initializes the contract
///
/// Returns `(contract_id, token_addr, admin, deployer)`.
pub fn setup_env(env: &Env) -> (Address, Address, Address, Address) {
    env.mock_all_auths();
    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = soroban_sdk::Vec::from_array(env, [admin.clone()]);
    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let token_addr = token_id.address();
    let contract_id = env.register_contract(None, QuorumCreditContract);
    // Pre-fund contract with enough tokens for loans + yield + stake
    StellarAssetClient::new(env, &token_addr).mint(&contract_id, &100_000_000_000);
    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1u32, &token_addr);
    (contract_id, token_addr, admin, deployer)
}

/// Mint tokens to an address so it can act as a voucher.
pub fn fund_address(env: &Env, admin: &Address, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

// ── Basic post-initialization invariant test ──────────────────────────────────

#[test]
fn test_invariants_hold_after_initialization() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup_env(&env);
    // No borrowers yet — invariants should hold on a freshly initialised contract.
    verify_invariants(&env, &contract_id, &token_addr, &[]).unwrap();
}

#[test]
fn test_invariants_hold_after_vouch() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token_addr, admin, _deployer) = setup_env(&env);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token_addr, &voucher, 10_000_000);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.vouch(&voucher, &borrower, &1_000_000i128, &token_addr, &None);

    verify_invariants(&env, &contract_id, &token_addr, &[&borrower]).unwrap();
}

#[test]
fn test_invariants_hold_after_loan_and_repay() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token_addr, admin, _deployer) = setup_env(&env);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token_addr, &voucher, 50_000_000);
    fund_address(&env, &admin, &token_addr, &borrower, 50_000_000);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    let loan_amount: i128 = 1_000_000;
    let stake: i128 = 10_000_000;

    // Bypass vouch-age restriction by advancing the ledger
    client.vouch(&voucher, &borrower, &stake, &token_addr, &None);
    env.ledger().with_mut(|l| l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10);

    verify_invariants(&env, &contract_id, &token_addr, &[&borrower]).unwrap();

    client.request_loan(
        &borrower,
        &loan_amount,
        &stake,
        &soroban_sdk::String::from_str(&env, "test loan"),
        &token_addr,
    );

    verify_invariants(&env, &contract_id, &token_addr, &[&borrower]).unwrap();

    // Borrower repays principal + yield
    let yield_amount = loan_amount * 200 / 10_000;
    client.repay(&borrower, &(loan_amount + yield_amount));

    // After repayment vouches are cleared — pass empty borrower slice
    verify_invariants(&env, &contract_id, &token_addr, &[]).unwrap();
}
