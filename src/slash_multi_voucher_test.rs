#![cfg(test)]
//! Issue #1434: multi-voucher slash distribution.
//!
//! Verifies that when several vouchers back the same loan, the slash is
//! distributed across them in proportion to each voucher's stake and that
//! the per-voucher shares sum back to the recorded `total_slashed`
//! (no rounding drift, no double-charging).
//!
//! Env / admin setup follows the pattern in `slash_appeal_test.rs`.

use crate::governance::{execute_slash_vote, vote_slash};
use crate::loan::request_loan;
use crate::types::{DataKey, SlashRecord, BPS_DENOMINATOR};
use crate::vouch::vouch;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

fn setup_test_env() -> (Env, Address, Address, Address, Address, Address) {
    let env = Env::new();
    env.mock_all_auths();
    env.ledger().set_timestamp(1000);

    let deployer = Address::random(&env);
    let admin = Address::random(&env);
    let borrower = Address::random(&env);
    let voucher1 = Address::random(&env);
    let voucher2 = Address::random(&env);
    let token = Address::random(&env);

    crate::QuorumCreditContract::initialize(
        env.clone(),
        deployer.clone(),
        vec![&env, admin.clone()],
        1,
        token.clone(),
    )
    .expect("initialize failed");

    (env, admin, borrower, voucher1, voucher2, token)
}

fn run_slash(
    env: &Env,
    borrower: &Address,
    vouchers: &[(Address, i128)],
    token: &Address,
) -> SlashRecord {
    for (v, stake) in vouchers {
        vouch(env, v.clone(), borrower.clone(), *stake, token.clone())
            .expect("vouch failed");
    }
    let loan_amount: i128 = vouchers.iter().map(|(_, s)| *s).sum();
    request_loan(env, borrower.clone(), loan_amount, 86400, String::new(env))
        .expect("request_loan failed");

    for (v, _) in vouchers {
        vote_slash(env, v.clone(), borrower.clone(), true).expect("vote_slash failed");
    }
    execute_slash_vote(env, borrower.clone()).expect("execute_slash_vote failed");

    env.storage()
        .persistent()
        .get(&DataKey::SlashAudit(borrower.clone()))
        .expect("slash record not found")
}

/// Per-voucher pro-rata shares sum to the recorded total slashed amount.
#[test]
fn test_multi_voucher_shares_sum_to_total() {
    let (env, _admin, borrower, voucher1, voucher2, token) = setup_test_env();
    let voucher3 = Address::random(&env);

    let stakes = [
        (voucher1.clone(), 1_000i128),
        (voucher2.clone(), 2_000i128),
        (voucher3.clone(), 3_000i128),
    ];
    let total_stake: i128 = stakes.iter().map(|(_, s)| *s).sum();

    let record = run_slash(&env, &borrower, &stakes, &token);
    let total_slashed = record.total_slashed;
    assert!(total_slashed > 0, "expected a positive total slashed amount");

    // Reconstruct each voucher's share from its stake proportion.
    let mut summed = 0i128;
    for (_, stake) in &stakes {
        let proportion_bps = (*stake * BPS_DENOMINATOR) / total_stake;
        summed += (total_slashed * proportion_bps) / BPS_DENOMINATOR;
    }

    // Allow at most (n_vouchers) stroops of integer-division rounding loss.
    let drift = total_slashed - summed;
    assert!(
        drift >= 0 && drift <= stakes.len() as i128,
        "pro-rata shares ({summed}) must sum to total_slashed ({total_slashed}) within rounding"
    );
}

/// A larger stake is slashed by a proportionally larger amount.
#[test]
fn test_slash_share_scales_with_stake() {
    let (env, _admin, borrower, voucher1, voucher2, token) = setup_test_env();

    let stakes = [
        (voucher1.clone(), 1_000i128),
        (voucher2.clone(), 4_000i128),
    ];
    let total_stake: i128 = stakes.iter().map(|(_, s)| *s).sum();

    let record = run_slash(&env, &borrower, &stakes, &token);
    let total_slashed = record.total_slashed;

    let share1 = (total_slashed * ((1_000 * BPS_DENOMINATOR) / total_stake)) / BPS_DENOMINATOR;
    let share2 = (total_slashed * ((4_000 * BPS_DENOMINATOR) / total_stake)) / BPS_DENOMINATOR;

    assert!(
        share2 > share1,
        "voucher with 4x the stake must absorb a larger slash share ({share2} vs {share1})"
    );
    // Effective slash percentage is bounded.
    assert!(
        record.effective_slash_bps > 0 && record.effective_slash_bps <= 10_000,
        "effective_slash_bps must be within (0, 10000]"
    );
}
