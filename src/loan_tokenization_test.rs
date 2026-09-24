//! Issue #1468: order-matching fairness tests for the loan_tokenization
//! secondary market — price-time priority and cancel-order ownership.

#[cfg(test)]
mod loan_tokenization_tests {
    use crate::errors::ContractError;
    use crate::loan_tokenization;
    use crate::QuorumCreditContract;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env,
    };

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuorumCreditContract, ());
        (env, contract_id)
    }

    /// Issue #1468: two orders at the same price must be matched in the
    /// order they were submitted (FIFO by submission time).
    #[test]
    fn test_orders_at_same_price_are_fifo_by_submission_time() {
        let (env, contract_id) = setup();
        let seller1 = Address::generate(&env);
        let seller2 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.ledger().with_mut(|l| l.timestamp = 100);
            let first = loan_tokenization::create_market_order(&env, seller1.clone(), 1, 50, 10).unwrap();

            env.ledger().with_mut(|l| l.timestamp = 200);
            let second = loan_tokenization::create_market_order(&env, seller2.clone(), 1, 50, 10).unwrap();

            let active = loan_tokenization::get_active_market_orders(&env, 1);
            assert_eq!(active.len(), 2);
            assert_eq!(active.get(0).unwrap().order_id, first.order_id);
            assert_eq!(active.get(1).unwrap().order_id, second.order_id);
        });
    }

    /// Issue #1468: a later order with a better price must be matched ahead
    /// of an earlier, worse-priced order (price takes priority over time).
    #[test]
    fn test_better_price_is_matched_ahead_of_earlier_worse_price() {
        let (env, contract_id) = setup();
        let seller = Address::generate(&env);

        env.as_contract(&contract_id, || {
            env.ledger().with_mut(|l| l.timestamp = 100);
            let worse = loan_tokenization::create_market_order(&env, seller.clone(), 1, 80, 10).unwrap();

            env.ledger().with_mut(|l| l.timestamp = 200);
            let better = loan_tokenization::create_market_order(&env, seller.clone(), 1, 40, 10).unwrap();

            let active = loan_tokenization::get_active_market_orders(&env, 1);
            assert_eq!(active.get(0).unwrap().order_id, better.order_id);
            assert_eq!(active.get(1).unwrap().order_id, worse.order_id);
        });
    }

    /// Issue #1468: `cancel_market_order` must reject a caller who is not
    /// the order's original seller.
    #[test]
    fn test_cancel_market_order_rejects_non_owner() {
        let (env, contract_id) = setup();
        let seller = Address::generate(&env);
        let stranger = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let order = loan_tokenization::create_market_order(&env, seller.clone(), 1, 50, 10).unwrap();

            let err = loan_tokenization::cancel_market_order(&env, &stranger, order.order_id).unwrap_err();
            assert_eq!(err, ContractError::UnauthorizedCaller);

            // The active order book is unaffected by the rejected attempt.
            let active = loan_tokenization::get_active_market_orders(&env, 1);
            assert_eq!(active.len(), 1);

            // The actual owner can still cancel it.
            loan_tokenization::cancel_market_order(&env, &seller, order.order_id).unwrap();
            let active = loan_tokenization::get_active_market_orders(&env, 1);
            assert_eq!(active.len(), 0);
        });
    }
}
