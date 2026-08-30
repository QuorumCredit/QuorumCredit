#[cfg(test)]
mod cache_tests {
    use crate::cache;
    use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        let contract_id = env.register(crate::QuorumCreditContract, ());
        (env, contract_id)
    }

    /// Issue #1473: a previously-set yield is returned from the cache without recomputation.
    #[test]
    fn test_cache_hit_returns_previously_set_yield() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        env.as_contract(&contract_id, || {
            cache::set_cached_yield(&env, &borrower, &voucher, 850, 500);
            assert_eq!(cache::get_cached_yield(&env, &borrower, &voucher, 500), Some(850));
        });
    }

    /// Issue #1473: an uncached (borrower, voucher) pair is a miss.
    #[test]
    fn test_cache_miss_when_never_set() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        env.as_contract(&contract_id, || {
            assert_eq!(cache::get_cached_yield(&env, &borrower, &voucher, 500), None);
        });
    }

    /// Issue #1473: a rate update (different base_yield_bps) invalidates the cached entry.
    #[test]
    fn test_cache_miss_after_rate_update() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        env.as_contract(&contract_id, || {
            cache::set_cached_yield(&env, &borrower, &voucher, 850, 500);
            // Config yield rate changed from 500 to 600 bps — stale entry must miss.
            assert_eq!(cache::get_cached_yield(&env, &borrower, &voucher, 600), None);
        });
    }

    /// Issue #1473: an explicit invalidation (e.g. after a stake change) evicts the entry.
    #[test]
    fn test_cache_invalidation_after_stake_change() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        env.as_contract(&contract_id, || {
            cache::set_cached_yield(&env, &borrower, &voucher, 850, 500);
            assert_eq!(cache::get_cached_yield(&env, &borrower, &voucher, 500), Some(850));

            cache::invalidate_yield_cache(&env, &borrower, &voucher);

            assert_eq!(cache::get_cached_yield(&env, &borrower, &voucher, 500), None);
        });
    }

    /// Issue #1473: an entry older than the TTL is treated as a miss even though the
    /// base_yield_bps still matches.
    #[test]
    fn test_cache_miss_after_ttl_expires() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        env.ledger().with_mut(|l| l.timestamp = 1_000);
        env.as_contract(&contract_id, || {
            cache::set_cached_yield(&env, &borrower, &voucher, 850, 500);
        });

        // Advance well past the 3600s TTL.
        env.ledger().with_mut(|l| l.timestamp = 1_000 + 3_601);
        env.as_contract(&contract_id, || {
            assert_eq!(cache::get_cached_yield(&env, &borrower, &voucher, 500), None);
        });
    }

    /// Distinct (borrower, voucher) pairs don't collide in the cache.
    #[test]
    fn test_cache_is_keyed_per_borrower_voucher_pair() {
        let (env, contract_id) = setup();
        let borrower_a = Address::generate(&env);
        let borrower_b = Address::generate(&env);
        let voucher = Address::generate(&env);

        env.as_contract(&contract_id, || {
            cache::set_cached_yield(&env, &borrower_a, &voucher, 850, 500);
            cache::set_cached_yield(&env, &borrower_b, &voucher, 700, 500);

            assert_eq!(cache::get_cached_yield(&env, &borrower_a, &voucher, 500), Some(850));
            assert_eq!(cache::get_cached_yield(&env, &borrower_b, &voucher, 500), Some(700));
        });
    }
}
