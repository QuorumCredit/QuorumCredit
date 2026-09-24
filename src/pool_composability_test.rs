//! Issue #1469: staleness tests for `pool_composability` — a pool that stops
//! reporting must be excluded from aggregate TVL/APY and flagged as stale.

#[cfg(test)]
mod pool_composability_tests {
    use crate::errors::ContractError;
    use crate::pool_composability;
    use crate::QuorumCreditContract;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env, String,
    };

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuorumCreditContract, ());
        (env, contract_id)
    }

    /// Issue #1469: a pool with no recent activity is excluded from
    /// aggregate TVL/APY once the freshness window elapses, and is
    /// surfaced as `stale` via `get_active_pools`.
    #[test]
    fn test_stale_pool_excluded_from_tvl_and_apy() {
        let (env, contract_id) = setup();
        let pool_contract = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.ledger().with_mut(|l| l.timestamp = 1_000);

            let pool = pool_composability::register_external_pool(
                &env,
                String::from_str(&env, "some-protocol"),
                pool_contract.clone(),
                String::from_str(&env, "farming"),
            )
            .unwrap();

            let deposit =
                pool_composability::deposit_to_external_pool(&env, 1, pool.pool_id, 1_000).unwrap();
            pool_composability::record_yield_earning(&env, deposit.deposit_id, 100, 500).unwrap();

            // Freshly reported: counted in both aggregates.
            assert_eq!(pool_composability::get_total_external_tvl(&env), 1_000);
            assert_eq!(pool_composability::calculate_weighted_avg_apy(&env, 1).unwrap(), 500);
            let active = pool_composability::get_active_pools(&env);
            assert_eq!(active.get(0).unwrap().stale, false);

            // Advance well past the default freshness window (7 days) with
            // no further report from this pool.
            env.ledger().with_mut(|l| l.timestamp = 1_000 + 8 * 24 * 60 * 60);

            assert_eq!(pool_composability::get_total_external_tvl(&env), 0);
            assert_eq!(
                pool_composability::calculate_weighted_avg_apy(&env, 1).unwrap_err(),
                ContractError::NotFound
            );
            let active = pool_composability::get_active_pools(&env);
            assert_eq!(active.get(0).unwrap().stale, true);
        });
    }

    /// Issue #1469: a fresh report (yield earning) on a previously stale
    /// pool resets its staleness clock, bringing it back into the aggregates.
    #[test]
    fn test_pool_becomes_fresh_again_after_reporting() {
        let (env, contract_id) = setup();
        let pool_contract = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.ledger().with_mut(|l| l.timestamp = 1_000);

            let pool = pool_composability::register_external_pool(
                &env,
                String::from_str(&env, "some-protocol"),
                pool_contract.clone(),
                String::from_str(&env, "farming"),
            )
            .unwrap();
            let deposit =
                pool_composability::deposit_to_external_pool(&env, 1, pool.pool_id, 1_000).unwrap();

            env.ledger().with_mut(|l| l.timestamp = 1_000 + 8 * 24 * 60 * 60);
            assert_eq!(pool_composability::get_total_external_tvl(&env), 0);

            // A fresh earning report resets the clock.
            pool_composability::record_yield_earning(&env, deposit.deposit_id, 50, 300).unwrap();
            assert_eq!(pool_composability::get_total_external_tvl(&env), 1_000);
        });
    }
}
