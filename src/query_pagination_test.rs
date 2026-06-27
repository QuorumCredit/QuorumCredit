/// Issue #941: Query Pagination — cursor-based pagination for get_vouches
#[cfg(test)]
mod query_pagination_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &100_000_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 500);
        Setup { env, client, admin, token: token_id.address() }
    }

    /// Register `n` distinct vouchers for `borrower`, each staking `stake` stroops.
    fn add_vouchers(s: &Setup, borrower: &Address, n: u32, stake: i128) {
        for _ in 0..n {
            let voucher = Address::generate(&s.env);
            StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake);
            s.client.vouch(&voucher, borrower, &stake, &s.token, &None);
            s.env.ledger().with_mut(|l| l.timestamp += 1);
        }
    }

    #[test]
    fn test_first_page_returns_correct_slice() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        add_vouchers(&s, &borrower, 5, 1_000_000);

        let page = s.client.get_vouches_paginated(&borrower, &0, &3);
        assert_eq!(page.records.len(), 3);
        assert_eq!(page.next_cursor, Some(3));
    }

    #[test]
    fn test_second_page_returns_remaining() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        add_vouchers(&s, &borrower, 5, 1_000_000);

        let page = s.client.get_vouches_paginated(&borrower, &3, &3);
        assert_eq!(page.records.len(), 2); // only 2 left
        assert_eq!(page.next_cursor, None); // last page
    }

    #[test]
    fn test_page_beyond_end_returns_empty() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        add_vouchers(&s, &borrower, 3, 1_000_000);

        let page = s.client.get_vouches_paginated(&borrower, &10, &5);
        assert_eq!(page.records.len(), 0);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn test_page_size_capped_at_50() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        // Add 60 vouchers — need to relax max_vouchers_per_borrower first
        let admins = Vec::from_array(&s.env, [s.admin.clone()]);
        s.client.set_max_vouchers_per_borrower(&admins, &100);
        add_vouchers(&s, &borrower, 60, 1_000_000);

        // Requesting 100 records should be capped to 50.
        let page = s.client.get_vouches_paginated(&borrower, &0, &100);
        assert_eq!(page.records.len(), 50);
        assert_eq!(page.next_cursor, Some(50));
    }

    #[test]
    fn test_no_vouches_returns_empty_page() {
        let s = setup();
        let borrower = Address::generate(&s.env);

        let page = s.client.get_vouches_paginated(&borrower, &0, &10);
        assert_eq!(page.records.len(), 0);
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn test_full_traversal_via_cursor() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        add_vouchers(&s, &borrower, 7, 1_000_000);

        let mut cursor: u32 = 0;
        let mut total_seen: u32 = 0;
        loop {
            let page = s.client.get_vouches_paginated(&borrower, &cursor, &3);
            total_seen += page.records.len();
            match page.next_cursor {
                None => break,
                Some(next) => cursor = next,
            }
        }
        assert_eq!(total_seen, 7);
    }
}
