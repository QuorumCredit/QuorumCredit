#![cfg(test)]

//! Tests for Arbitrage Prevention Module (Issue #967)
//!
//! Covers:
//! - Overflow-safe percentage-change arithmetic (Issue #1432)
//! - Two-step `propose` → wait → `finalize` exchange-rate updates (Issue #1431)
//! - Stale `RateHistory` decay vs. within-window accumulation (Issue #1433)

mod tests {
    use crate::arbitrage_prevention::*;
    use crate::types::DataKey;
    use crate::ContractError;
    use crate::QuorumCreditContract;
    use proptest::prelude::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env,
    };

    // ── Issue #1432: overflow-safe calculate_percentage_change ───────────────

    #[test]
    fn test_percentage_change_basic() {
        // old = 1000, new = 1100 -> +10% -> 1000 bps
        assert_eq!(calculate_percentage_change(1_000, 1_100).unwrap(), 1_000);
        // symmetric drop
        assert_eq!(calculate_percentage_change(1_000, 900).unwrap(), -1_000);
        // zero old rate is rejected
        assert_eq!(
            calculate_percentage_change(0, 100),
            Err(ContractError::InvalidAmount)
        );
    }

    #[test]
    fn test_percentage_change_overflow_is_reported_not_wrapped() {
        // `change * 10_000` overflows i128 for rates this large; the checked
        // path must surface ArithmeticError instead of silently wrapping.
        let huge = i128::MAX / 5_000; // change * 10_000 > i128::MAX
        assert_eq!(
            calculate_percentage_change(1, huge),
            Err(ContractError::ArithmeticError)
        );
        assert_eq!(
            calculate_percentage_change(huge, -huge),
            Err(ContractError::ArithmeticError)
        );
    }

    proptest! {
        #![proptest_config(proptest::test_runner::Config {
            cases: 512,
            max_shrink_iters: 64,
            ..Default::default()
        })]

        /// For any rates, the function either returns a mathematically correct
        /// bps figure or a clean ArithmeticError — it never panics and never
        /// returns a silently wrapped value.
        #[test]
        fn prop_percentage_change_never_wraps(
            old_rate in 1i128..=i128::MAX,
            new_rate in i128::MIN..=i128::MAX,
        ) {
            match calculate_percentage_change(old_rate, new_rate) {
                Ok(bps) => {
                    let change = new_rate.saturating_sub(old_rate);
                    // Ok is only allowed when the multiplication fit in i128.
                    let scaled = change.checked_mul(10_000);
                    prop_assert!(scaled.is_some());
                    prop_assert_eq!(bps, scaled.unwrap() / old_rate);
                }
                Err(e) => prop_assert_eq!(e, ContractError::ArithmeticError),
            }
        }

        /// Very large rates (well past the i128::MAX / 10_000 boundary) must be
        /// rejected, never wrapped into a spurious in-bounds percentage.
        #[test]
        fn prop_large_rates_reject_instead_of_wrapping(
            old_rate in 1i128..=1_000i128,
            new_rate in (i128::MAX / 2)..=i128::MAX,
        ) {
            prop_assert_eq!(
                calculate_percentage_change(old_rate, new_rate),
                Err(ContractError::ArithmeticError)
            );
        }
    }

    // ── Shared harness for storage-backed tests ────────────────────────────

    struct Setup {
        env: Env,
        contract_id: Address,
        token_a: Address,
        token_b: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        let contract_id = env.register(QuorumCreditContract, ());
        let token_a = Address::generate(&env);
        let token_b = Address::generate(&env);
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        Setup {
            env,
            contract_id,
            token_a,
            token_b,
        }
    }

    /// Seed a registered pair directly (bypassing the admin-gated
    /// `register_token_pair` wrapper — auth is covered by governance tests).
    fn seed_pair(s: &Setup, rate: i128, max_slippage_bps: u32) {
        s.env.storage().persistent().set(
            &DataKey::ExchangeRate(s.token_a.clone(), s.token_b.clone()),
            &ExchangeRate {
                token_a: s.token_a.clone(),
                token_b: s.token_b.clone(),
                rate,
                updated_at: s.env.ledger().timestamp(),
                max_slippage_bps,
            },
        );
    }

    fn current_rate(s: &Setup) -> i128 {
        get_exchange_rate(s.env.clone(), s.token_a.clone(), s.token_b.clone()).unwrap()
    }

    fn history(s: &Setup) -> RateHistory {
        s.env
            .storage()
            .persistent()
            .get(&DataKey::RateHistory(s.token_a.clone(), s.token_b.clone()))
            .unwrap()
    }

    // ── Issue #1431: two-step exchange-rate update ─────────────────────────

    #[test]
    fn test_propose_wait_finalize_applies_rate() {
        let s = setup();
        s.env.as_contract(&s.contract_id, || {
            seed_pair(&s, 1_000_000, 1_000); // 10% max slippage

            // Propose a +5% move (within slippage).
            propose_rate_update_inner(&s.env, s.token_a.clone(), s.token_b.clone(), 1_050_000)
                .unwrap();

            // Rate is unchanged before finalization.
            assert_eq!(current_rate(&s), 1_000_000);

            // Too early: finalize is rejected before the timelock elapses.
            s.env
                .ledger()
                .with_mut(|l| l.timestamp += MIN_RATE_UPDATE_DELAY_SECS - 1);
            assert_eq!(
                finalize_rate_update_inner(&s.env, s.token_a.clone(), s.token_b.clone()),
                Err(ContractError::DelayNotElapsed)
            );
            assert_eq!(current_rate(&s), 1_000_000);

            // Advance past the timelock and finalize.
            s.env.ledger().with_mut(|l| l.timestamp += 1);
            finalize_rate_update_inner(&s.env, s.token_a.clone(), s.token_b.clone()).unwrap();
            assert_eq!(current_rate(&s), 1_050_000);

            // Pending entry was cleared: a second finalize finds nothing.
            assert_eq!(
                finalize_rate_update_inner(&s.env, s.token_a.clone(), s.token_b.clone()),
                Err(ContractError::NotFound)
            );
        });
    }

    #[test]
    fn test_finalize_too_early_is_rejected() {
        let s = setup();
        s.env.as_contract(&s.contract_id, || {
            seed_pair(&s, 1_000_000, 1_000);
            propose_rate_update_inner(&s.env, s.token_a.clone(), s.token_b.clone(), 1_050_000)
                .unwrap();

            // One second short of the required delay.
            s.env
                .ledger()
                .with_mut(|l| l.timestamp += MIN_RATE_UPDATE_DELAY_SECS - 1);
            assert_eq!(
                finalize_rate_update_inner(&s.env, s.token_a.clone(), s.token_b.clone()),
                Err(ContractError::DelayNotElapsed)
            );
            assert_eq!(current_rate(&s), 1_000_000);
        });
    }

    #[test]
    fn test_propose_rejects_out_of_slippage_move() {
        let s = setup();
        s.env.as_contract(&s.contract_id, || {
            seed_pair(&s, 1_000_000, 1_000); // 10%
            // +20% proposal exceeds the 10% bound.
            assert_eq!(
                propose_rate_update_inner(&s.env, s.token_a.clone(), s.token_b.clone(), 1_200_000),
                Err(ContractError::InvalidAmount)
            );
            assert!(s
                .env
                .storage()
                .persistent()
                .get::<_, PendingRateUpdate>(&DataKey::PendingRateUpdate(
                    s.token_a.clone(),
                    s.token_b.clone()
                ))
                .is_none());
        });
    }

    #[test]
    fn test_finalize_without_proposal_is_not_found() {
        let s = setup();
        s.env.as_contract(&s.contract_id, || {
            seed_pair(&s, 1_000_000, 1_000);
            assert_eq!(
                finalize_rate_update_inner(&s.env, s.token_a.clone(), s.token_b.clone()),
                Err(ContractError::NotFound)
            );
        });
    }

    // ── Issue #1433: rate-history decay ───────────────────────────────────

    #[test]
    fn test_rate_history_accumulates_within_window() {
        let s = setup();
        s.env.as_contract(&s.contract_id, || {
            update_rate_history(&s.env, &s.token_a, &s.token_b, 1_000_000).unwrap();

            // A day later — well inside the default 30d window.
            s.env.ledger().with_mut(|l| l.timestamp += 86_400);
            update_rate_history(&s.env, &s.token_a, &s.token_b, 1_500_000).unwrap();
            s.env.ledger().with_mut(|l| l.timestamp += 86_400);
            update_rate_history(&s.env, &s.token_a, &s.token_b, 800_000).unwrap();

            let h = history(&s);
            // Band spans every observed rate — nothing was reset.
            assert_eq!(h.min_rate, 800_000);
            assert_eq!(h.max_rate, 1_500_000);
        });
    }

    #[test]
    fn test_rate_history_resets_after_window() {
        let s = setup();
        s.env.as_contract(&s.contract_id, || {
            update_rate_history(&s.env, &s.token_a, &s.token_b, 1_000_000).unwrap();
            s.env.ledger().with_mut(|l| l.timestamp += 100);
            update_rate_history(&s.env, &s.token_a, &s.token_b, 2_000_000).unwrap();
            assert_eq!(history(&s).max_rate, 2_000_000);

            // Shrink the window, then jump well past it before the next update.
            set_rate_history_window_inner(&s.env, 3_600).unwrap();
            s.env.ledger().with_mut(|l| l.timestamp += 10_000);
            update_rate_history(&s.env, &s.token_a, &s.token_b, 2_100_000).unwrap();

            let h = history(&s);
            // Stale band was discarded and re-seeded from the current rate.
            assert_eq!(h.min_rate, 2_100_000);
            assert_eq!(h.max_rate, 2_100_000);
            assert_eq!(h.avg_rate, 2_100_000);
            assert_eq!(h.last_updated, s.env.ledger().timestamp());
        });
    }

    #[test]
    fn test_rate_history_reset_boundary_is_strict() {
        let s = setup();
        s.env.as_contract(&s.contract_id, || {
            set_rate_history_window_inner(&s.env, 3_600).unwrap();
            update_rate_history(&s.env, &s.token_a, &s.token_b, 1_000_000).unwrap();

            // Exactly at the window edge (== window, not >) still accumulates.
            s.env.ledger().with_mut(|l| l.timestamp += 3_600);
            update_rate_history(&s.env, &s.token_a, &s.token_b, 5_000_000).unwrap();
            assert_eq!(history(&s).max_rate, 5_000_000);
            assert_eq!(history(&s).min_rate, 1_000_000);

            // One second past the edge resets.
            s.env.ledger().with_mut(|l| l.timestamp += 3_601);
            update_rate_history(&s.env, &s.token_a, &s.token_b, 2_000_000).unwrap();
            assert_eq!(history(&s).min_rate, 2_000_000);
            assert_eq!(history(&s).max_rate, 2_000_000);
        });
    }

    #[test]
    fn test_set_rate_history_window_rejects_zero() {
        let s = setup();
        s.env.as_contract(&s.contract_id, || {
            assert_eq!(
                set_rate_history_window_inner(&s.env, 0),
                Err(ContractError::InvalidAmount)
            );
        });
    }
}
