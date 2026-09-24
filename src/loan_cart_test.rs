//! Tests for the loan cart batch-submission discount reporting (issue #1397).
#[cfg(test)]
mod loan_cart_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = soroban_sdk::Vec::from_array(&env, [admin.clone()]);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            token: token_id.address(),
        }
    }

    fn purpose(env: &Env) -> String {
        String::from_str(env, "test loan")
    }

    fn do_vouch(s: &Setup, borrower: &Address, stake: i128) {
        let voucher = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake);
        s.client.vouch(&voucher, borrower, &stake, &s.token, &None);
    }

    /// A 3-item cart should report exactly one success (the first item) and
    /// two `ActiveLoanExists` failures — the protocol's single-active-loan
    /// invariant means only one submission per batch can ever disburse.
    /// Discount reporting must reflect that: only the successful item's
    /// `discounted_amount` is actually discounted; the two failed items
    /// report their plain requested amount, never a phantom discount for a
    /// loan that was never funded.
    #[test]
    fn batch_of_three_reports_one_success_and_accurate_discounts() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);

        // Amounts kept well above DEFAULT_MIN_LOAN_AMOUNT (100_000) even after
        // the 1% discount, so the first item succeeds on its own merits and
        // the only reason items 1-2 fail is ActiveLoanExists, not amount.
        s.client.add_to_loan_cart(&borrower, &200_000, &2_592_000);
        s.client.add_to_loan_cart(&borrower, &300_000, &2_592_000);
        s.client.add_to_loan_cart(&borrower, &400_000, &2_592_000);

        let results = s
            .client
            .submit_batch_loan_request(&borrower, &purpose(&s.env), &0, &s.token);

        assert_eq!(results.len(), 3);

        let first = results.get(0).unwrap();
        assert!(first.success);
        assert_eq!(first.error_code, None);
        assert_eq!(first.requested_amount, 200_000);
        // 3-item batch clears the volume discount threshold: 200_000 - 1% = 198_000.
        assert_eq!(first.discounted_amount, 198_000);
        assert!(first.discount_applied);

        for i in 1..3 {
            let item = results.get(i).unwrap();
            assert!(!item.success);
            assert_eq!(item.error_code, Some(2)); // ContractError::ActiveLoanExists
            // No phantom discount reported for a loan that never disbursed.
            assert_eq!(item.discounted_amount, item.requested_amount);
            assert!(!item.discount_applied);
        }
    }

    // ── #1396: per-item removal / edit ────────────────────────────────────

    #[test]
    fn remove_cart_item_drops_only_the_targeted_item() {
        let s = setup();
        let borrower = Address::generate(&s.env);

        s.client.add_to_loan_cart(&borrower, &100_000, &2_592_000);
        s.client.add_to_loan_cart(&borrower, &200_000, &2_592_000);
        s.client.add_to_loan_cart(&borrower, &300_000, &2_592_000);

        let cart = s.client.remove_cart_item(&borrower, &1);

        assert_eq!(cart.items.len(), 2);
        assert_eq!(cart.items.get(0).unwrap().amount, 100_000);
        assert_eq!(cart.items.get(1).unwrap().amount, 300_000);

        let stats = s.client.get_cart_abandonment_stats();
        assert_eq!(stats.items_removed, 1);
        // Removing an item is not abandonment: the cart wasn't dropped.
        assert_eq!(stats.carts_submitted, 0);
    }

    #[test]
    fn update_cart_item_replaces_amount_and_tenure_in_place() {
        let s = setup();
        let borrower = Address::generate(&s.env);

        s.client.add_to_loan_cart(&borrower, &100_000, &2_592_000);
        s.client.add_to_loan_cart(&borrower, &200_000, &2_592_000);

        let cart = s.client.update_cart_item(&borrower, &0, &150_000, &1_296_000);

        assert_eq!(cart.items.len(), 2);
        let updated = cart.items.get(0).unwrap();
        assert_eq!(updated.amount, 150_000);
        assert_eq!(updated.tenure_secs, 1_296_000);
        // The other item is untouched.
        assert_eq!(cart.items.get(1).unwrap().amount, 200_000);

        let stats = s.client.get_cart_abandonment_stats();
        assert_eq!(stats.items_edited, 1);
    }

    #[test]
    fn remove_cart_item_out_of_range_returns_not_found() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        s.client.add_to_loan_cart(&borrower, &100_000, &2_592_000);

        let result = s.client.try_remove_cart_item(&borrower, &5);
        assert_eq!(result, Err(Ok(crate::errors::ContractError::NotFound)));
    }

    #[test]
    fn update_cart_item_out_of_range_returns_not_found() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        s.client.add_to_loan_cart(&borrower, &100_000, &2_592_000);

        let result = s
            .client
            .try_update_cart_item(&borrower, &5, &100_000, &2_592_000);
        assert_eq!(result, Err(Ok(crate::errors::ContractError::NotFound)));
    }

    #[test]
    fn remove_cart_item_with_no_cart_returns_not_found() {
        let s = setup();
        let borrower = Address::generate(&s.env);

        let result = s.client.try_remove_cart_item(&borrower, &0);
        assert_eq!(result, Err(Ok(crate::errors::ContractError::NotFound)));
    }

    /// A cart below the volume-discount threshold never reports a discount,
    /// applied or otherwise.
    #[test]
    fn batch_of_two_never_discounts() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);

        s.client.add_to_loan_cart(&borrower, &100_000, &2_592_000);
        s.client.add_to_loan_cart(&borrower, &200_000, &2_592_000);

        let results = s
            .client
            .submit_batch_loan_request(&borrower, &purpose(&s.env), &0, &s.token);

        let first = results.get(0).unwrap();
        assert!(first.success);
        assert_eq!(first.discounted_amount, first.requested_amount);
        assert!(!first.discount_applied);
    }
}
