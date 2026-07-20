#![cfg(test)]

use crate::loan::calculate_dynamic_rate;
use crate::types::{DataKey, DynamicRateConfig, OraclePriceRecord};
use crate::QuorumCreditContract;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Symbol,
};

fn set_rate_config(env: &Env, oracle_symbol: Symbol) {
    let cfg = DynamicRateConfig {
        enabled: true,
        base_rate_bps: 200,
        risk_adjustment_bps: 10,
        rate_cap_bps: 5000,
        rate_floor_bps: 50,
        oracle_price_symbol: Some(oracle_symbol),
        oracle_risk_threshold: 100,
        oracle_risk_premium_bps: 300,
        oracle_stale_premium_bps: 500,
    };
    env.storage().instance().set(&DataKey::DynamicRateConfig, &cfg);
}

/// Issue #64/#665: the oracle price feed must actually inform loan pricing, not
/// sit unread. This proves the effective rate genuinely changes as the fresh
/// price crosses the configured risk threshold, and that missing/stale data is
/// priced conservatively rather than silently ignored.
#[test]
fn oracle_price_crossing_threshold_changes_dynamic_rate() {
    let env = Env::default();
    let contract_id = env.register(QuorumCreditContract, ());
    let borrower = Address::generate(&env);
    let symbol = Symbol::new(&env, "xlmusd");

    env.as_contract(&contract_id, || {
        set_rate_config(&env, symbol.clone());

        // No oracle price recorded at all -> treated the same as stale: conservative premium.
        let rate_missing = calculate_dynamic_rate(&env, &borrower, 0);
        assert_eq!(
            rate_missing,
            200 + 500,
            "missing oracle data should apply the stale premium"
        );

        // Fresh price ABOVE the risk threshold -> no premium.
        env.storage().persistent().set(
            &DataKey::OraclePrice(symbol.clone()),
            &OraclePriceRecord {
                price: 150,
                recorded_at: env.ledger().timestamp(),
            },
        );
        let rate_healthy = calculate_dynamic_rate(&env, &borrower, 0);
        assert_eq!(
            rate_healthy, 200,
            "a healthy fresh price should apply no oracle premium"
        );

        // Fresh price BELOW the risk threshold -> risk premium applies.
        env.storage().persistent().set(
            &DataKey::OraclePrice(symbol.clone()),
            &OraclePriceRecord {
                price: 50,
                recorded_at: env.ledger().timestamp(),
            },
        );
        let rate_stressed = calculate_dynamic_rate(&env, &borrower, 0);
        assert_eq!(
            rate_stressed,
            200 + 300,
            "a price below threshold should apply the risk premium"
        );

        assert!(
            rate_stressed > rate_healthy,
            "the loan rate must genuinely change when the oracle price crosses the threshold"
        );

        // A stale price (past the freshness window) must be treated as conservatively as
        // missing data, not trusted just because a record exists.
        env.storage().persistent().set(
            &DataKey::OraclePrice(symbol.clone()),
            &OraclePriceRecord {
                price: 150, // would be "healthy" if fresh
                recorded_at: 0,
            },
        );
        env.ledger().with_mut(|l| {
            l.timestamp = crate::types::ORACLE_PRICE_MAX_AGE_SECS + 1;
        });
        let rate_stale = calculate_dynamic_rate(&env, &borrower, 0);
        assert_eq!(
            rate_stale,
            200 + 500,
            "a stale price must not be trusted even though a healthy value is recorded"
        );
    });
}
