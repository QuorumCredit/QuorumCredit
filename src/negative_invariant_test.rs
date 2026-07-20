//! # Negative-Control Invariant Tests
//!
//! These tests **deliberately corrupt contract state** and then assert that
//! `verify_invariants` returns an error.  They prove the harness is not vacuous
//! — every invariant listed in `contract-invariants.md` is actively exercised
//! both in the "holds" direction (normal tests) and the "catches" direction here.
//!
//! ## Structure
//!
//! Each test:
//! 1. Sets up a normal, valid contract state.
//! 2. Directly mutates the **underlying storage** to introduce a violation.
//! 3. Calls `verify_invariants` and asserts it returns the expected `Err`.
//!
//! Soroban's `env.storage()` is accessible inside `#[cfg(test)]` code, so we
//! can reach behind the contract's public API to produce states that would be
//! impossible to create through normal operation.

#![cfg(test)]
#![allow(dead_code, unused_imports)]

use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::types::{Config, DataKey, LoanRecord, LoanStatus, VouchRecord};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::{self, StellarAssetClient},
    Address, Env, Vec,
};

// ── Helper: build a minimal LoanRecord ───────────────────────────────────────

fn make_active_loan(env: &Env, borrower: &Address, token: &Address, amount: i128) -> LoanRecord {
    let now = env.ledger().timestamp();
    LoanRecord {
        id: 1,
        borrower: borrower.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: crate::types::EscrowStatus::None,
        co_borrowers: soroban_sdk::Vec::new(env),
        amount,
        amount_repaid: 0,
        total_yield: amount * 200 / 10_000,
        status: LoanStatus::Active,
        repaid: false,
        defaulted: false,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: None,
        deadline: now + crate::types::DEFAULT_LOAN_DURATION,
        loan_purpose: soroban_sdk::String::from_str(env, "neg-ctrl-test"),
        token_address: token.clone(),
        amortization_schedule: soroban_sdk::Vec::new(env),
        reminder_sent: false,
        risk_score: 0,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: crate::types::RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: false,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    }
}

// ── NC-1: Solvency (I1) ───────────────────────────────────────────────────────

/// Inject a VouchRecord whose stake exceeds the contract's token balance.
/// `verify_invariants` must detect the I1 violation.
#[test]
fn test_nc_i1_solvency_violation_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);

    // Fund voucher with only 1_000_000 stroops so the contract holds that amount.
    fund_address(&env, &admin, &token, &voucher, 1_000_000);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.vouch(&voucher, &borrower, &1_000_000i128, &token, &None);

    // Directly inflate the stake in storage to 999_999_999_999 — far beyond
    // what the contract actually holds.
    let mut vouches: soroban_sdk::Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .expect("vouches must exist after vouch()");

    // Replace the first vouch record with an inflated stake.
    let mut inflated = vouches.get(0).unwrap();
    inflated.stake = 999_999_999_999;
    let mut new_vouches = soroban_sdk::Vec::new(&env);
    new_vouches.push_back(inflated);
    for i in 1..vouches.len() {
        new_vouches.push_back(vouches.get(i).unwrap());
    }

    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &new_vouches);

    // Harness must return I1 violation.
    let result = verify_invariants(&env, &contract_id, &token, &[&borrower]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I1 solvency violation"
    );
    assert_eq!(
        result.unwrap_err().id,
        "I1",
        "violation id must be I1 (solvency)"
    );
}

// ── NC-2: Over-repayment (I4) ─────────────────────────────────────────────────

/// Inject an `amount_repaid` that exceeds `amount + total_yield`.
/// `verify_invariants` must detect the I4 violation.
#[test]
fn test_nc_i4_over_repayment_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 10_000_000);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.vouch(&voucher, &borrower, &5_000_000i128, &token, &None);
    env.ledger().with_mut(|l| {
        l.timestamp = crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 10;
    });
    client.request_loan(
        &borrower,
        &1_000_000i128,
        &5_000_000i128,
        &soroban_sdk::String::from_str(&env, "nc-test"),
        &token,
    );

    // Read the active loan record.
    let loan_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveLoan(borrower.clone()))
        .expect("active loan must exist");
    let mut loan: LoanRecord = env
        .storage()
        .persistent()
        .get(&DataKey::Loan(loan_id))
        .expect("loan record must exist");

    // Corrupt: set amount_repaid well above amount + total_yield.
    loan.amount_repaid = loan.amount + loan.total_yield + 1;
    env.storage().persistent().set(&DataKey::Loan(loan_id), &loan);

    let result = verify_invariants(&env, &contract_id, &token, &[&borrower]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I4 over-repayment violation"
    );
    assert_eq!(
        result.unwrap_err().id,
        "I4",
        "violation id must be I4 (over-repayment)"
    );
}

// ── NC-3: Invalid yield_bps (I7) ─────────────────────────────────────────────

/// Set `yield_bps` to a value above 10_000 in storage directly.
/// `verify_invariants` must detect the I7 violation.
#[test]
fn test_nc_i7_out_of_range_yield_bps_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    // Read current config and inflate yield_bps.
    let mut cfg: Config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .expect("config must exist after initialize");
    cfg.yield_bps = 10_001; // > 10_000 — invalid
    env.storage().instance().set(&DataKey::Config, &cfg);

    let result = verify_invariants(&env, &contract_id, &token, &[]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I7 yield_bps range violation"
    );
    assert_eq!(
        result.unwrap_err().id,
        "I7",
        "violation id must be I7 (yield_bps out of range)"
    );
}

// ── NC-4: Admin threshold out of range (I8) ───────────────────────────────────

/// Set `admin_threshold` to 0 (below the minimum of 1) in storage directly.
/// `verify_invariants` must detect the I8 violation.
#[test]
fn test_nc_i8_admin_threshold_zero_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    let mut cfg: Config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .expect("config must exist");
    cfg.admin_threshold = 0; // violates I8: must be >= 1
    env.storage().instance().set(&DataKey::Config, &cfg);

    let result = verify_invariants(&env, &contract_id, &token, &[]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I8 admin_threshold == 0 violation"
    );
    assert_eq!(
        result.unwrap_err().id,
        "I8",
        "violation id must be I8 (admin threshold)"
    );
}

/// Set `admin_threshold` to admins.len() + 1 (above the maximum).
#[test]
fn test_nc_i8_admin_threshold_exceeds_admins_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    let mut cfg: Config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .expect("config must exist");
    // There is 1 admin; set threshold to 2 — impossible to satisfy.
    cfg.admin_threshold = cfg.admins.len() as u32 + 1;
    env.storage().instance().set(&DataKey::Config, &cfg);

    let result = verify_invariants(&env, &contract_id, &token, &[]);
    assert!(
        result.is_err(),
        "verify_invariants must detect threshold > admins.len()"
    );
    assert_eq!(result.unwrap_err().id, "I8");
}

// ── NC-5: Negative slash treasury (I6) ───────────────────────────────────────

/// Directly write a negative value into the slash treasury.
/// `verify_invariants` must detect the I6 violation.
#[test]
fn test_nc_i6_negative_slash_treasury_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    env.storage()
        .instance()
        .set(&DataKey::SlashTreasury, &(-1i128));

    let result = verify_invariants(&env, &contract_id, &token, &[]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I6 negative treasury violation"
    );
    assert_eq!(result.unwrap_err().id, "I6");
}

// ── NC-6: Active loan without vouches (I3) ────────────────────────────────────

/// Insert an active loan record with no corresponding vouches.
/// `verify_invariants` must detect the I3 violation.
#[test]
fn test_nc_i3_active_loan_without_vouches_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    let borrower = Address::generate(&env);
    let loan = make_active_loan(&env, &borrower, &token, 500_000);

    // Write an active loan record but deliberately omit the vouches entry.
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);
    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(borrower.clone()), &loan.id);
    // No DataKey::Vouches entry for this borrower.

    let result = verify_invariants(&env, &contract_id, &token, &[&borrower]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I3 active-loan-without-vouches violation"
    );
    assert_eq!(result.unwrap_err().id, "I3");
}

// ── NC-7: Negative stake (I10) ────────────────────────────────────────────────

/// Directly write a VouchRecord with a negative stake.
/// `verify_invariants` must detect the I10 violation.
#[test]
fn test_nc_i10_negative_stake_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);
    fund_address(&env, &admin, &token, &voucher, 5_000_000);

    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.vouch(&voucher, &borrower, &5_000_000i128, &token, &None);

    // Corrupt the stake to a negative value.
    let mut vouches: soroban_sdk::Vec<VouchRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::Vouches(borrower.clone()))
        .expect("vouches must exist");

    let mut bad_vouch = vouches.get(0).unwrap();
    bad_vouch.stake = -1;
    let mut new_vouches = soroban_sdk::Vec::new(&env);
    new_vouches.push_back(bad_vouch);

    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &new_vouches);

    let result = verify_invariants(&env, &contract_id, &token, &[&borrower]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I10 negative-stake violation"
    );
    assert_eq!(result.unwrap_err().id, "I10");
}

// ── NC-8: Stale ActiveLoan pointer (I11) ─────────────────────────────────────

/// Write an ActiveLoan pointer whose referenced loan has a Repaid status.
/// The contract should have removed the pointer at repayment time.
/// `verify_invariants` must detect the I11 stale-pointer violation.
#[test]
fn test_nc_i11_stale_active_loan_pointer_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    let borrower = Address::generate(&env);
    let mut loan = make_active_loan(&env, &borrower, &token, 500_000);

    // Set status to Repaid to simulate a stale pointer scenario.
    loan.status = LoanStatus::Repaid;
    loan.repaid = true;

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);
    // Deliberately leave ActiveLoan pointer even though loan is Repaid.
    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(borrower.clone()), &loan.id);

    // Also inject a vouch so I3 does not fire first.
    let voucher = Address::generate(&env);
    let vouch = VouchRecord {
        voucher: voucher.clone(),
        stake: 1_000_000,
        vouch_timestamp: 0,
        token: token.clone(),
        expiry_timestamp: None,
        delegate: None,
        chain_id: None,
    };
    let mut vouches = soroban_sdk::Vec::new(&env);
    vouches.push_back(vouch);
    env.storage()
        .persistent()
        .set(&DataKey::Vouches(borrower.clone()), &vouches);

    let result = verify_invariants(&env, &contract_id, &token, &[&borrower]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I11 stale ActiveLoan pointer"
    );
    assert_eq!(result.unwrap_err().id, "I11");
}

// ── NC-9: Zero min_loan_amount (I12) ─────────────────────────────────────────

/// Set min_loan_amount to 0 in storage directly.
/// `verify_invariants` must detect the I12 violation.
#[test]
fn test_nc_i12_zero_min_loan_amount_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    let mut cfg: Config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .expect("config must exist");
    cfg.min_loan_amount = 0;
    env.storage().instance().set(&DataKey::Config, &cfg);

    let result = verify_invariants(&env, &contract_id, &token, &[]);
    assert!(
        result.is_err(),
        "verify_invariants must detect the I12 zero min_loan_amount violation"
    );
    assert_eq!(result.unwrap_err().id, "I12");
}

// ── NC-10: Out-of-range slash_bps (I9) ───────────────────────────────────────

/// Set slash_bps to a negative value — outside the valid [0, 10_000] range.
#[test]
fn test_nc_i9_negative_slash_bps_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    let mut cfg: Config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .expect("config must exist");
    cfg.slash_bps = -1;
    env.storage().instance().set(&DataKey::Config, &cfg);

    let result = verify_invariants(&env, &contract_id, &token, &[]);
    assert!(
        result.is_err(),
        "verify_invariants must detect negative slash_bps"
    );
    assert_eq!(result.unwrap_err().id, "I9");
}

// ── NC-11: Loan amount exceeds total stake (I2) ───────────────────────────────

/// Construct a loan whose amount exceeds the total vouched stake for the borrower.
/// This simulates a bypass of the stake-threshold check in request_loan.
#[test]
fn test_nc_i2_loan_exceeds_stake_detected() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, admin, _deployer) = setup_env(&env);

    let voucher = Address::generate(&env);
    let borrower = Address::generate(&env);

    // Vouch with only 1_000_000 stroops.
    fund_address(&env, &admin, &token, &voucher, 1_000_000);
    let client = QuorumCreditContractClient::new(&env, &contract_id);
    client.vouch(&voucher, &borrower, &1_000_000i128, &token, &None);

    // Inject a loan whose amount is 10× the actual stake — impossible via normal API.
    let loan = make_active_loan(&env, &borrower, &token, 10_000_000);
    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan.id), &loan);
    env.storage()
        .persistent()
        .set(&DataKey::ActiveLoan(borrower.clone()), &loan.id);

    let result = verify_invariants(&env, &contract_id, &token, &[&borrower]);
    assert!(
        result.is_err(),
        "verify_invariants must detect loan amount > total stake"
    );
    assert_eq!(result.unwrap_err().id, "I2");
}

// ── Positive control: harness passes on valid state ───────────────────────────

/// Sanity check — a clean contract state always passes every invariant.
#[test]
fn test_nc_baseline_clean_state_passes() {
    let env = Env::default();
    env.mock_all_auths();
    let (contract_id, token, _admin, _deployer) = setup_env(&env);

    verify_invariants(&env, &contract_id, &token, &[])
        .expect("invariants must hold on a clean contract");
}
