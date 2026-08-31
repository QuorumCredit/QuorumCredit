//! Tests for issue #1408: per-vouch reputation weight records must not all
//! collapse onto the same storage slot.
#[cfg(test)]
mod vouch_reputation_tests {
    use crate::types::VouchRecord;
    use crate::vouch_reputation::{
        derive_vouch_id, get_vouch_reputation_weight, update_weighted_vouch_distribution,
    };
    use crate::QuorumCreditContract;
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn each_vouch_gets_a_distinct_retrievable_weight_record() {
        let env = Env::default();
        let contract_id = env.register(QuorumCreditContract, ());
        let borrower = Address::generate(&env);
        let token = Address::generate(&env);

        let voucher_a = Address::generate(&env);
        let voucher_b = Address::generate(&env);
        let voucher_c = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let vouches = soroban_sdk::Vec::from_array(
                &env,
                [
                    VouchRecord {
                        voucher: voucher_a.clone(),
                        stake: 1_000_000,
                        vouch_timestamp: 0,
                        token: token.clone(),
                        expiry_timestamp: None,
                        delegate: None,
                        chain_id: None,
                    },
                    VouchRecord {
                        voucher: voucher_b.clone(),
                        stake: 2_000_000,
                        vouch_timestamp: 0,
                        token: token.clone(),
                        expiry_timestamp: None,
                        delegate: None,
                        chain_id: None,
                    },
                    VouchRecord {
                        voucher: voucher_c.clone(),
                        stake: 3_000_000,
                        vouch_timestamp: 0,
                        token: token.clone(),
                        expiry_timestamp: None,
                        delegate: None,
                        chain_id: None,
                    },
                ],
            );

            update_weighted_vouch_distribution(&env, &borrower, &token, &vouches).unwrap();

            let id_a = derive_vouch_id(&env, &borrower, &voucher_a, &token);
            let id_b = derive_vouch_id(&env, &borrower, &voucher_b, &token);
            let id_c = derive_vouch_id(&env, &borrower, &voucher_c, &token);

            // Distinct IDs — the actual bug: every vouch previously hashed to
            // the same hardcoded id (0).
            assert_ne!(id_a, id_b);
            assert_ne!(id_b, id_c);
            assert_ne!(id_a, id_c);

            let weight_a = get_vouch_reputation_weight(&env, id_a)
                .expect("voucher_a's weight record should be retrievable");
            let weight_b = get_vouch_reputation_weight(&env, id_b)
                .expect("voucher_b's weight record should be retrievable");
            let weight_c = get_vouch_reputation_weight(&env, id_c)
                .expect("voucher_c's weight record should be retrievable");

            // Each record reflects its own vouch's stake, not whichever vouch
            // happened to be processed last.
            assert_eq!(weight_a.base_strength, 1_000_000);
            assert_eq!(weight_b.base_strength, 2_000_000);
            assert_eq!(weight_c.base_strength, 3_000_000);
        });
    }

    #[test]
    fn derive_vouch_id_differs_by_borrower_even_for_the_same_voucher_and_token() {
        let env = Env::default();
        let borrower_1 = Address::generate(&env);
        let borrower_2 = Address::generate(&env);
        let voucher = Address::generate(&env);
        let token = Address::generate(&env);

        let id_1 = derive_vouch_id(&env, &borrower_1, &voucher, &token);
        let id_2 = derive_vouch_id(&env, &borrower_2, &voucher, &token);

        assert_ne!(id_1, id_2);
    }

    #[test]
    fn derive_vouch_id_is_deterministic() {
        let env = Env::default();
        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);
        let token = Address::generate(&env);

        let first = derive_vouch_id(&env, &borrower, &voucher, &token);
        let second = derive_vouch_id(&env, &borrower, &voucher, &token);

        assert_eq!(first, second);
    }
}
