//! Issue #1421 — Referral Reward Flow Tests
//!
//! Exercises `generate_referral_code`, `get_referrer_by_code`,
//! `distribute_referral_reward`, and `get_referral_leaderboard` in
//! `src/referral.rs`:
//!   - `generate_referral_code` is idempotent — repeated calls return the same
//!     code for the same referrer.
//!   - `get_referrer_by_code` reverse-lookup returns the correct referrer.
//!   - `distribute_referral_reward` pays out correctly when the yield reserve
//!     has sufficient funds.
//!   - It is a silent no-op when the yield reserve is insufficient.
//!   - It is a silent no-op when the borrower has no registered referrer.
//!   - `get_referral_leaderboard` sorts descending by conversion_count, then
//!     total_rewards_earned, for a mixed input set.

#![cfg(test)]

use crate::referral::distribute_referral_reward;
use crate::types::{DataKey, DEFAULT_REFERRAL_BONUS_BPS};
use crate::{QuorumCreditContract, QuorumCreditContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::StellarAssetClient,
    Address, Env, Vec,
};

// ── helpers ───────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, Address, Address, Address) {
    env.mock_all_auths();

    let deployer = Address::generate(env);
    let admin = Address::generate(env);
    let admins = Vec::from_array(env, [admin.clone()]);

    let token_id = env.register_stellar_asset_contract_v2(admin.clone());
    let contract_id = env.register_contract(None, crate::QuorumCreditContract);

    StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &100_000_000);

    let client = QuorumCreditContractClient::new(env, &contract_id);
    client.initialize(&deployer, &admins, &1, &token_id.address());

    env.ledger().with_mut(|l| l.timestamp = 120);

    (contract_id, token_id.address(), admin, deployer)
}

// ── Test 1: generate_referral_code is idempotent ──────────────────────────────

#[test]
fn test_generate_referral_code_is_idempotent() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let referrer = Address::generate(&env);

    let code1 = client.generate_referral_code(&referrer);
    let code2 = client.generate_referral_code(&referrer);

    assert_eq!(
        code1, code2,
        "generate_referral_code must return the same code on repeated calls"
    );
}

// ── Test 2: get_referrer_by_code reverse lookup ───────────────────────────────

#[test]
fn test_get_referrer_by_code_reverse_lookup() {
    let env = Env::default();
    let (contract_id, _token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let referrer = Address::generate(&env);
    let code = client.generate_referral_code(&referrer);

    let looked_up = client.get_referrer_by_code(&code);
    assert_eq!(
        looked_up,
        Some(referrer.clone()),
        "get_referrer_by_code must return the referrer who owns the code"
    );
}

// ── Test 3: unknown code returns None ─────────────────────────────────────────

#[test]
fn test_get_referrer_by_code_unknown_returns_none() {
    let env = Env::default();
    let (contract_id, _token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    // Use a dummy 32-byte hash that was never registered
    let fake_code = soroban_sdk::BytesN::<32>::from_array(&env, &[0u8; 32]);
    let looked_up = client.get_referrer_by_code(&fake_code);
    assert!(looked_up.is_none(), "Unregistered code must return None");
}

// ── Test 4: distribute_referral_reward pays out when reserve is sufficient ────

#[test]
fn test_distribute_referral_reward_pays_correctly() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);

    let referrer = Address::generate(&env);
    let borrower = Address::generate(&env);

    let first_loan_interest: i128 = 200_000; // 200_000 stroops of interest

    env.as_contract(&contract_id, || {
        // Register the referrer for the borrower
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);

        // Seed the yield reserve with plenty of funds
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &10_000_000i128);

        // Set referral bonus to DEFAULT_REFERRAL_BONUS_BPS (1000 = 10%)
        env.storage()
            .instance()
            .set(&DataKey::ReferralBonusBps, &DEFAULT_REFERRAL_BONUS_BPS);

        let tc = soroban_sdk::token::Client::new(&env, &token_addr);
        let referrer_balance_before = tc.balance(&referrer);

        distribute_referral_reward(&env, &borrower, &token_addr, first_loan_interest);

        // Expected reward: 200_000 * 1000 / 10_000 = 20_000
        let expected_reward: i128 = first_loan_interest * DEFAULT_REFERRAL_BONUS_BPS as i128 / 10_000;
        assert_eq!(
            tc.balance(&referrer),
            referrer_balance_before + expected_reward,
            "Referrer should receive the correct reward"
        );

        // Yield reserve should be reduced by the reward
        let remaining_reserve: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::YieldReserve)
            .unwrap_or(0);
        assert_eq!(
            remaining_reserve,
            10_000_000 - expected_reward,
            "Yield reserve should decrease by the reward amount"
        );
    });
}

// ── Test 5: silent no-op when yield reserve is insufficient ───────────────────

#[test]
fn test_distribute_referral_reward_noop_when_reserve_insufficient() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);

    let referrer = Address::generate(&env);
    let borrower = Address::generate(&env);

    env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::ReferredBy(borrower.clone()), &referrer);

        // Set an empty yield reserve
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &0i128);

        env.storage()
            .instance()
            .set(&DataKey::ReferralBonusBps, &DEFAULT_REFERRAL_BONUS_BPS);

        let tc = soroban_sdk::token::Client::new(&env, &token_addr);
        let referrer_balance_before = tc.balance(&referrer);

        // Must not panic; must be a no-op
        distribute_referral_reward(&env, &borrower, &token_addr, 200_000);

        assert_eq!(
            tc.balance(&referrer),
            referrer_balance_before,
            "Referrer balance must not change when reserve is insufficient"
        );
    });
}

// ── Test 6: silent no-op when borrower has no registered referrer ──────────────

#[test]
fn test_distribute_referral_reward_noop_when_no_referrer() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);

    let borrower = Address::generate(&env);

    env.as_contract(&contract_id, || {
        // No ReferredBy key for this borrower
        env.storage()
            .persistent()
            .set(&DataKey::YieldReserve, &10_000_000i128);

        // Should be a silent no-op — must not panic
        distribute_referral_reward(&env, &borrower, &token_addr, 200_000);

        // Yield reserve must be unchanged (no payout happened)
        let reserve: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::YieldReserve)
            .unwrap_or(0);
        assert_eq!(reserve, 10_000_000, "Reserve must be unchanged when borrower has no referrer");
    });
}

// ── Test 7: get_referral_leaderboard sort order ───────────────────────────────

#[test]
fn test_referral_leaderboard_sort_order() {
    let env = Env::default();
    let (contract_id, _token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let addr_a = Address::generate(&env);
    let addr_b = Address::generate(&env);
    let addr_c = Address::generate(&env);

    // Seed stats directly in storage:
    //   addr_b: 5 conversions, 100_000 rewards  (highest conversions)
    //   addr_a: 3 conversions, 200_000 rewards  (lower conversions, higher rewards)
    //   addr_c: 3 conversions,  50_000 rewards  (tie on conversions, lower rewards)
    // Expected sorted order: b (5), a (3, 200k), c (3, 50k)
    env.as_contract(&contract_id, || {
        use crate::types::ReferralStats;

        env.storage().persistent().set(
            &DataKey::ReferralRewardsEarned(addr_a.clone()),
            &ReferralStats {
                referrer: addr_a.clone(),
                conversion_count: 3,
                total_rewards_earned: 200_000,
                last_conversion_at: 0,
            },
        );
        env.storage().persistent().set(
            &DataKey::ReferralRewardsEarned(addr_b.clone()),
            &ReferralStats {
                referrer: addr_b.clone(),
                conversion_count: 5,
                total_rewards_earned: 100_000,
                last_conversion_at: 0,
            },
        );
        env.storage().persistent().set(
            &DataKey::ReferralRewardsEarned(addr_c.clone()),
            &ReferralStats {
                referrer: addr_c.clone(),
                conversion_count: 3,
                total_rewards_earned: 50_000,
                last_conversion_at: 0,
            },
        );
    });

    let referrers = Vec::from_array(&env, [addr_a.clone(), addr_b.clone(), addr_c.clone()]);
    let leaderboard = client.get_referral_leaderboard(&referrers);

    assert_eq!(leaderboard.len(), 3);
    assert_eq!(
        leaderboard.get(0).unwrap().referrer,
        addr_b,
        "1st place must be addr_b (5 conversions)"
    );
    assert_eq!(
        leaderboard.get(1).unwrap().referrer,
        addr_a,
        "2nd place must be addr_a (3 conversions, 200k rewards)"
    );
    assert_eq!(
        leaderboard.get(2).unwrap().referrer,
        addr_c,
        "3rd place must be addr_c (3 conversions, 50k rewards)"
    );
}

// ════════════════════════════════════════════════════════════════════════════
//  Issue #1421 — backfill_payment_history tests
// ════════════════════════════════════════════════════════════════════════════

use crate::types::{
    AmortizationEntry, EscrowStatus, LoanRecord, LoanStatus, PaymentRecord, RateType,
};

fn make_terminal_loan(env: &Env, contract_id: &Address, token: &Address, status: LoanStatus) -> u64 {
    let loan_id: u64 = {
        let counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::LoanCounter)
            .unwrap_or(0)
            + 1;
        env.storage()
            .persistent()
            .set(&DataKey::LoanCounter, &counter);
        counter
    };

    let borrower = Address::generate(env);
    let now = env.ledger().timestamp();

    let (repaid, defaulted) = match status {
        LoanStatus::Repaid => (true, false),
        LoanStatus::Defaulted => (false, true),
        _ => (false, false),
    };

    let loan = LoanRecord {
        id: loan_id,
        borrower: borrower.clone(),
        guarantor: None,
        buyback_price: 0,
        auto_repay_enabled: false,
        auto_repay_attempts: 0,
        escrow_status: EscrowStatus::None,
        co_borrowers: soroban_sdk::Vec::new(env),
        amount: 1_000_000,
        amount_repaid: 1_000_000,
        total_yield: 20_000,
        status,
        repaid,
        defaulted,
        created_at: now,
        disbursement_timestamp: now,
        repayment_timestamp: Some(now + 100),
        deadline: now + 86_400,
        loan_purpose: soroban_sdk::String::from_str(env, "test"),
        token_address: token.clone(),
        amortization_schedule: soroban_sdk::Vec::new(env),
        reminder_sent: false,
        risk_score: 0,
        deferment_periods: 0,
        maturity_date: None,
        rate_type: RateType::Fixed,
        index_reference: None,
        last_interest_calc: now,
        accrued_interest: 0,
        milestone_bonus_applied: 0,
        retry_count: 0,
        suspension_timestamp: None,
        suspension_amount_repaid: 0,
    };

    env.storage()
        .persistent()
        .set(&DataKey::Loan(loan_id), &loan);
    loan_id
}

// ── Test 8: backfill_payment_history succeeds for a Repaid loan ───────────────

#[test]
fn test_backfill_payment_history_succeeds_for_repaid_loan() {
    let env = Env::default();
    let (contract_id, token_addr, admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let loan_id = env.as_contract(&contract_id, || {
        make_terminal_loan(&env, &contract_id, &token_addr, LoanStatus::Repaid)
    });

    let records = Vec::from_array(
        &env,
        [
            PaymentRecord {
                amount: 400_000,
                timestamp: 100,
                cumulative_repaid: 400_000,
            },
            PaymentRecord {
                amount: 600_000,
                timestamp: 200,
                cumulative_repaid: 1_000_000,
            },
        ],
    );

    let admin_sigs = Vec::from_array(&env, [admin.clone()]);
    let result = client.try_backfill_payment_history(&admin_sigs, &loan_id, &records);
    assert!(
        result.is_ok(),
        "backfill_payment_history must succeed for a Repaid loan, got: {:?}",
        result
    );

    env.as_contract(&contract_id, || {
        let history: soroban_sdk::Vec<PaymentRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::PaymentHistory(loan_id))
            .expect("PaymentHistory should be set after backfill");
        assert_eq!(history.len(), 2, "Two payment records should be stored");
        assert_eq!(
            history.get(1).unwrap().cumulative_repaid,
            1_000_000,
            "Last record cumulative_repaid should be 1_000_000"
        );
    });
}

// ── Test 9: backfill rejected for Active loan ─────────────────────────────────

#[test]
fn test_backfill_payment_history_rejected_for_active_loan() {
    let env = Env::default();
    let (contract_id, token_addr, admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let loan_id = env.as_contract(&contract_id, || {
        make_terminal_loan(&env, &contract_id, &token_addr, LoanStatus::Active)
    });

    let records = Vec::from_array(
        &env,
        [PaymentRecord {
            amount: 500_000,
            timestamp: 100,
            cumulative_repaid: 500_000,
        }],
    );

    let admin_sigs = Vec::from_array(&env, [admin.clone()]);
    let result = client.try_backfill_payment_history(&admin_sigs, &loan_id, &records);
    assert_eq!(
        result,
        Err(Ok(crate::ContractError::InvalidStateTransition)),
        "backfill_payment_history must reject Active loans with InvalidStateTransition"
    );
}

// ── Test 10: backfill rejected for non-existent loan ─────────────────────────

#[test]
fn test_backfill_payment_history_rejected_for_nonexistent_loan() {
    let env = Env::default();
    let (contract_id, _token_addr, admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let records = Vec::from_array(
        &env,
        [PaymentRecord {
            amount: 500_000,
            timestamp: 100,
            cumulative_repaid: 500_000,
        }],
    );

    let admin_sigs = Vec::from_array(&env, [admin.clone()]);
    let result = client.try_backfill_payment_history(&admin_sigs, &9_999_999u64, &records);
    assert_eq!(
        result,
        Err(Ok(crate::ContractError::NoActiveLoan)),
        "backfill_payment_history must reject a non-existent loan_id with NoActiveLoan"
    );
}

// ── Test 11: backfill rejected for non-admin caller ───────────────────────────

#[test]
fn test_backfill_payment_history_rejected_for_non_admin() {
    let env = Env::default();
    let (contract_id, token_addr, _admin, _deployer) = setup(&env);
    let client = QuorumCreditContractClient::new(&env, &contract_id);

    let loan_id = env.as_contract(&contract_id, || {
        make_terminal_loan(&env, &contract_id, &token_addr, LoanStatus::Repaid)
    });

    let records = Vec::from_array(
        &env,
        [PaymentRecord {
            amount: 500_000,
            timestamp: 100,
            cumulative_repaid: 500_000,
        }],
    );

    // Use a random address that is NOT an admin
    let rando = Address::generate(&env);
    let fake_signers = Vec::from_array(&env, [rando.clone()]);

    let result = client.try_backfill_payment_history(&fake_signers, &loan_id, &records);
    // Should fail authorization — exact error depends on require_admin_approval
    assert!(
        result.is_err(),
        "backfill_payment_history must be rejected for a non-admin caller"
    );
}
