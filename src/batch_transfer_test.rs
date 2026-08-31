#[cfg(test)]
mod batch_transfer_tests {
    use crate::batch_transfer;
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, token::Client as TokenClient, Address,
        Env,
    };

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(admin);
        let token = token_id.address();

        let contract_id = env.register(crate::QuorumCreditContract, ());
        StellarAssetClient::new(&env, &token).mint(&contract_id, &10_000_000);

        (env, contract_id, token)
    }

    /// Issue #1472: multiple queued transfers to the same (token, recipient) pair are
    /// summed and executed as exactly one transfer on flush.
    #[test]
    fn test_multiple_queued_transfers_same_recipient_are_summed_and_flushed_once() {
        let (env, contract_id, token) = setup();
        let recipient = Address::generate(&env);
        let token_client = TokenClient::new(&env, &token);

        env.as_contract(&contract_id, || {
            batch_transfer::queue_transfer(&env, recipient.clone(), 100, token.clone());
            batch_transfer::queue_transfer(&env, recipient.clone(), 250, token.clone());
            batch_transfer::queue_transfer(&env, recipient.clone(), 50, token.clone());
        });

        assert_eq!(token_client.balance(&recipient), 0);

        env.as_contract(&contract_id, || {
            batch_transfer::flush_transfers(&env).unwrap();
        });

        assert_eq!(token_client.balance(&recipient), 400);

        // Flushing again must be a no-op: the queue was cleared, so the recipient's
        // balance doesn't move a second time.
        env.as_contract(&contract_id, || {
            batch_transfer::flush_transfers(&env).unwrap();
        });
        assert_eq!(token_client.balance(&recipient), 400);
    }

    /// Issue #1472: distinct recipients (and distinct tokens) keep separate aggregated
    /// totals — no cross-contamination.
    #[test]
    fn test_queued_transfers_are_kept_separate_per_recipient_and_token() {
        let (env, contract_id, token_a) = setup();
        let admin_b = Address::generate(&env);
        let token_b_id = env.register_stellar_asset_contract_v2(admin_b);
        let token_b = token_b_id.address();
        StellarAssetClient::new(&env, &token_b).mint(&contract_id, &10_000_000);

        let recipient_1 = Address::generate(&env);
        let recipient_2 = Address::generate(&env);

        env.as_contract(&contract_id, || {
            batch_transfer::queue_transfer(&env, recipient_1.clone(), 100, token_a.clone());
            batch_transfer::queue_transfer(&env, recipient_2.clone(), 300, token_a.clone());
            batch_transfer::queue_transfer(&env, recipient_1.clone(), 700, token_b.clone());

            batch_transfer::flush_transfers(&env).unwrap();
        });

        let token_a_client = TokenClient::new(&env, &token_a);
        let token_b_client = TokenClient::new(&env, &token_b);
        assert_eq!(token_a_client.balance(&recipient_1), 100);
        assert_eq!(token_a_client.balance(&recipient_2), 300);
        assert_eq!(token_b_client.balance(&recipient_1), 700);
    }

    /// A zero-amount queue call is ignored rather than creating a dangling entry.
    #[test]
    fn test_zero_amount_queue_transfer_is_ignored() {
        let (env, contract_id, token) = setup();
        let recipient = Address::generate(&env);
        let token_client = TokenClient::new(&env, &token);

        env.as_contract(&contract_id, || {
            batch_transfer::queue_transfer(&env, recipient.clone(), 0, token.clone());
            batch_transfer::flush_transfers(&env).unwrap();
        });

        assert_eq!(token_client.balance(&recipient), 0);
    }
}
