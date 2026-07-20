//! # Proptest-based Invariant Fuzzing Harness
//!
//! This module generates randomized sequences of contract operations
//! (vouch / request_loan / repay / slash / update_config) and asserts
//! that all I1–I12 invariants hold after **every single step**.
//!
//! ## Design
//!
//! Each fuzz run:
//! 1. Picks a random list of `Op` variants.
//! 2. Executes each `Op` against the live Soroban test environment.
//! 3. Calls `verify_invariants` — any invariant violation causes an immediate
//!    proptest failure with a full reproduction trace.
//!
//! Operations that are expected to fail gracefully (e.g. vouching with 0 stake)
//! use `try_*` calls so the harness does not panic on contract-level errors.
//! The invariant check ensures that even a rejected operation leaves state
//! consistent.
//!
//! ## Running
//!
//! ```
//! cargo test invariant_fuzz -- --nocapture
//! ```
//!
//! By default proptest runs 256 cases. Set `PROPTEST_CASES=1024` for deeper
//! exploration.

#![cfg(test)]
#![allow(unused_variables, dead_code)]

use crate::invariants_test::{fund_address, setup_env, verify_invariants};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use proptest::prelude::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString};

// ── Operation vocabulary ──────────────────────────────────────────────────────

/// Every operation the fuzzer can apply.
#[derive(Clone, Debug)]
enum Op {
    /// vouch(voucher_idx, borrower_idx, stake)
    Vouch {
        voucher_idx: usize,
        borrower_idx: usize,
        stake: i128,
    },
    /// request_loan(borrower_idx, amount, threshold)
    RequestLoan {
        borrower_idx: usize,
        amount: i128,
        threshold: i128,
    },
    /// repay(borrower_idx, payment)
    Repay {
        borrower_idx: usize,
        payment: i128,
    },
    /// slash(admin, borrower_idx)
    Slash { borrower_idx: usize },
    /// update_config(yield_bps, slash_bps)
    UpdateConfig {
        yield_bps: i128,
        slash_bps: i128,
    },
    /// increase_stake(voucher_idx, borrower_idx, amount)
    IncreaseStake {
        voucher_idx: usize,
        borrower_idx: usize,
        amount: i128,
    },
    /// withdraw_vouch(voucher_idx, borrower_idx)
    WithdrawVouch {
        voucher_idx: usize,
        borrower_idx: usize,
    },
}

// ── Strategy generators ───────────────────────────────────────────────────────

/// Number of distinct actor slots (vouchers + borrowers share the same pool to
/// maximise address reuse and find aliasing bugs).
const NUM_ACTORS: usize = 4;

fn actor_idx() -> impl Strategy<Value = usize> {
    0usize..NUM_ACTORS
}

/// Stake values in the realistic range: 1 stroop up to 50 XLM, plus a sprinkle
/// of edge cases (0, negative, very large).
fn stake_strategy() -> impl Strategy<Value = i128> {
    prop_oneof![
        // Normal range: 100_000 stroops (0.01 XLM) – 50_000_000 (5 XLM)
        100_000i128..=50_000_000i128,
        // Edge cases
        Just(0i128),
        Just(1i128),
        Just(-1i128),
        Just(i128::MAX),
        Just(i128::MIN),
    ]
}

fn loan_amount_strategy() -> impl Strategy<Value = i128> {
    prop_oneof![
        // Normal range
        100_000i128..=5_000_000i128,
        // Edge cases
        Just(0i128),
        Just(-1i128),
        Just(1i128),
        Just(i128::MAX),
    ]
}

fn bps_strategy() -> impl Strategy<Value = i128> {
    prop_oneof![
        // Valid range
        0i128..=10_000i128,
        // Out-of-range (should be rejected by the contract)
        Just(-1i128),
        Just(10_001i128),
        Just(i128::MAX),
    ]
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        // Vouch — most common op
        3 => (actor_idx(), actor_idx(), stake_strategy()).prop_map(|(vi, bi, s)| {
            Op::Vouch { voucher_idx: vi, borrower_idx: bi, stake: s }
        }),
        // Request loan
        2 => (actor_idx(), loan_amount_strategy(), loan_amount_strategy()).prop_map(|(bi, a, t)| {
            Op::RequestLoan { borrower_idx: bi, amount: a, threshold: t }
        }),
        // Repay
        2 => (actor_idx(), loan_amount_strategy()).prop_map(|(bi, p)| {
            Op::Repay { borrower_idx: bi, payment: p }
        }),
        // Slash
        1 => actor_idx().prop_map(|bi| Op::Slash { borrower_idx: bi }),
        // Config update — should always stay in-range if contract validates properly
        1 => (bps_strategy(), bps_strategy()).prop_map(|(y, s)| {
            Op::UpdateConfig { yield_bps: y, slash_bps: s }
        }),
        // Increase stake
        1 => (actor_idx(), actor_idx(), stake_strategy()).prop_map(|(vi, bi, a)| {
            Op::IncreaseStake { voucher_idx: vi, borrower_idx: bi, amount: a }
        }),
        // Withdraw vouch
        1 => (actor_idx(), actor_idx()).prop_map(|(vi, bi)| {
            Op::WithdrawVouch { voucher_idx: vi, borrower_idx: bi }
        }),
    ]
}

fn ops_strategy() -> impl Strategy<Value = std::vec::Vec<Op>> {
    // 1–20 operations per run: short enough to be fast, long enough to find bugs.
    prop::collection::vec(op_strategy(), 1..=20)
}

// ── Harness execution ─────────────────────────────────────────────────────────

/// Execute a single `Op` against the contract. All operations use `try_*` so that
/// contract-level errors (InvalidAmount, etc.) are silently swallowed — we are only
/// interested in invariant violations, not panics from expected error paths.
fn execute_op(
    env: &Env,
    client: &QuorumCreditContractClient,
    actors: &[Address],
    admin: &Address,
    token: &Address,
    op: &Op,
) {
    let admins = soroban_sdk::Vec::from_array(env, [admin.clone()]);

    match op {
        Op::Vouch { voucher_idx, borrower_idx, stake } => {
            let voucher = &actors[voucher_idx % actors.len()];
            let borrower = &actors[borrower_idx % actors.len()];
            // Pre-fund voucher so the transfer can succeed for valid stakes.
            if *stake > 0 && *stake < 100_000_000 {
                fund_address(env, admin, token, voucher, *stake);
            }
            let _ = client.try_vouch(voucher, borrower, stake, token, &None);
        }
        Op::RequestLoan { borrower_idx, amount, threshold } => {
            let borrower = &actors[borrower_idx % actors.len()];
            let purpose = SorobanString::from_str(env, "fuzz-loan");
            // Advance past min vouch age so the request is not rejected on timing.
            env.ledger().with_mut(|l| {
                l.timestamp = l.timestamp.saturating_add(crate::types::DEFAULT_MIN_VOUCH_AGE_SECS + 1);
            });
            let _ = client.try_request_loan(borrower, amount, threshold, &purpose, token);
        }
        Op::Repay { borrower_idx, payment } => {
            let borrower = &actors[borrower_idx % actors.len()];
            if *payment > 0 {
                fund_address(env, admin, token, borrower, *payment);
            }
            let _ = client.try_repay(borrower, payment);
        }
        Op::Slash { borrower_idx } => {
            let borrower = &actors[borrower_idx % actors.len()];
            // Advance past deadline so slash is valid.
            env.ledger().with_mut(|l| {
                l.timestamp = l.timestamp.saturating_add(crate::types::DEFAULT_LOAN_DURATION + 1);
            });
            let _ = client.try_slash(&admins, borrower);
        }
        Op::UpdateConfig { yield_bps, slash_bps } => {
            let _ = client.try_update_config(&admins, &Some(*yield_bps), &Some(*slash_bps));
        }
        Op::IncreaseStake { voucher_idx, borrower_idx, amount } => {
            let voucher = &actors[voucher_idx % actors.len()];
            let borrower = &actors[borrower_idx % actors.len()];
            if *amount > 0 && *amount < 100_000_000 {
                fund_address(env, admin, token, voucher, *amount);
            }
            // increase_stake(voucher, borrower, additional) — no token param in the contract API
            let _ = client.try_increase_stake(voucher, borrower, amount);
        }
        Op::WithdrawVouch { voucher_idx, borrower_idx } => {
            let voucher = &actors[voucher_idx % actors.len()];
            let borrower = &actors[borrower_idx % actors.len()];
            // Advance past the vouch lock period.
            env.ledger().with_mut(|l| {
                l.timestamp = l.timestamp.saturating_add(crate::types::MIN_VOUCH_LOCK_PERIOD + 1);
            });
            // withdraw_vouch(voucher, borrower) — no token param in the contract API
            let _ = client.try_withdraw_vouch(voucher, borrower);
        }
    }
}

// ── Proptest entry point ──────────────────────────────────────────────────────

proptest! {
    /// Main fuzz test: apply a random sequence of operations and assert that all
    /// invariants hold after every step.
    #[test]
    fn invariant_fuzz_sequence(ops in ops_strategy()) {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token, admin, _deployer) = setup_env(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Create a fixed pool of actor addresses.
        let mut actors: std::vec::Vec<Address> = std::vec::Vec::new();
        for _ in 0..NUM_ACTORS {
            let a = Address::generate(&env);
            // Give each actor a base balance so transfer-related checks don't
            // immediately fail on insufficient-funds for small stakes.
            fund_address(&env, &admin, &token, &a, 10_000_000);
            actors.push(a);
        }

        // All borrower addresses (the same pool — any actor can borrow).
        let borrower_refs: std::vec::Vec<&Address> = actors.iter().collect();

        // Initial invariant check on a freshly-initialised contract.
        verify_invariants(&env, &contract_id, &token, &[])
            .expect("invariants must hold on fresh contract");

        for op in &ops {
            execute_op(&env, &client, &actors, &admin, &token, op);

            // Re-build the borrower slice each time (all actors are potential borrowers).
            let borrower_refs: std::vec::Vec<&Address> = actors.iter().collect();

            verify_invariants(&env, &contract_id, &token, borrower_refs.as_slice())
                .unwrap_or_else(|violation| {
                    panic!(
                        "Invariant {} violated after op {:?}: {}",
                        violation.id,
                        op,
                        violation.message,
                    )
                });
        }
    }

    /// Config-only fuzz: verify that any combination of yield_bps / slash_bps
    /// values leaves I7, I8, I9 intact (either the update is accepted and stays
    /// in-range, or it is rejected).
    #[test]
    fn invariant_fuzz_config_only(
        yield_bps in bps_strategy(),
        slash_bps in bps_strategy()
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token, admin, _deployer) = setup_env(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);

        let _ = client.try_update_config(&admins, &Some(yield_bps), &Some(slash_bps));

        verify_invariants(&env, &contract_id, &token, &[])
            .unwrap_or_else(|v| panic!("Config fuzz violated {}: {}", v.id, v.message));
    }

    /// Stake-amount fuzz: a single vouch with an arbitrary stake must leave I1
    /// and I10 intact.
    #[test]
    fn invariant_fuzz_single_vouch(stake in stake_strategy()) {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token, admin, _deployer) = setup_env(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let voucher = Address::generate(&env);
        let borrower = Address::generate(&env);

        // Only fund for positive, reasonable stakes.
        if stake > 0 && stake < 500_000_000 {
            fund_address(&env, &admin, &token, &voucher, stake);
        }

        let _ = client.try_vouch(&voucher, &borrower, &stake, &token, &None);

        // Pass borrower so I2/I3 checks run even if vouch succeeded.
        verify_invariants(&env, &contract_id, &token, &[&borrower])
            .unwrap_or_else(|v| panic!("Single-vouch fuzz violated {}: {}", v.id, v.message));
    }
}
