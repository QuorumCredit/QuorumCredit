/// Issue #71: Unit tests for batch_total_stake (parallel stake calculation).
#[cfg(test)]
mod batch_stake_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    // ── helpers ───────────────────────────────────────────────────────────────

    struct Ctx {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token: Address,
    }

    fn setup() -> Ctx {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();

        client.initialize(&deployer, &admins, &1u32, &token);

        // Mint tokens to the contract so it can pay out loans / returns.
        let asset_admin =
            soroban_sdk::token::StellarAssetClient::new(&env, &token);
        asset_admin.mint(&contract_id, &1_000_000_000);

        Ctx { env, client, token }
    }

    fn mint_and_vouch(ctx: &Ctx, voucher: &Address, borrower: &Address, stake: i128) {
        let asset_admin =
            soroban_sdk::token::StellarAssetClient::new(&ctx.env, &ctx.token);
        asset_admin.mint(voucher, &stake);
        ctx.client
            .vouch(voucher, borrower, &stake, &ctx.token, &None);
    }

    // ── tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_batch_empty_input_returns_empty_vec() {
        let ctx = setup();
        let borrowers: Vec<Address> = Vec::new(&ctx.env);
        let result = ctx.client.batch_total_stake(&borrowers);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_batch_single_borrower_no_vouches() {
        let ctx = setup();
        let borrower = Address::generate(&ctx.env);
        let borrowers = Vec::from_array(&ctx.env, [borrower.clone()]);

        let result = ctx.client.batch_total_stake(&borrowers);
        assert_eq!(result.len(), 1);
        assert_eq!(result.get(0).unwrap().borrower, borrower);
        assert_eq!(result.get(0).unwrap().total_stake, 0);
    }

    #[test]
    fn test_batch_single_borrower_with_one_vouch() {
        let ctx = setup();
        let borrower = Address::generate(&ctx.env);
        let voucher = Address::generate(&ctx.env);

        mint_and_vouch(&ctx, &voucher, &borrower, 1_000_000);

        let borrowers = Vec::from_array(&ctx.env, [borrower.clone()]);
        let result = ctx.client.batch_total_stake(&borrowers);

        assert_eq!(result.len(), 1);
        assert_eq!(result.get(0).unwrap().total_stake, 1_000_000);
    }

    #[test]
    fn test_batch_single_borrower_multiple_vouchers() {
        let ctx = setup();
        let borrower = Address::generate(&ctx.env);
        let v1 = Address::generate(&ctx.env);
        let v2 = Address::generate(&ctx.env);
        let v3 = Address::generate(&ctx.env);

        mint_and_vouch(&ctx, &v1, &borrower, 1_000_000);
        mint_and_vouch(&ctx, &v2, &borrower, 2_000_000);
        mint_and_vouch(&ctx, &v3, &borrower, 3_000_000);

        let borrowers = Vec::from_array(&ctx.env, [borrower.clone()]);
        let result = ctx.client.batch_total_stake(&borrowers);

        assert_eq!(result.len(), 1);
        assert_eq!(result.get(0).unwrap().total_stake, 6_000_000);
    }

    #[test]
    fn test_batch_multiple_borrowers_returns_correct_sums() {
        let ctx = setup();
        let b1 = Address::generate(&ctx.env);
        let b2 = Address::generate(&ctx.env);
        let b3 = Address::generate(&ctx.env);
        let v1 = Address::generate(&ctx.env);
        let v2 = Address::generate(&ctx.env);
        let v3 = Address::generate(&ctx.env);

        mint_and_vouch(&ctx, &v1, &b1, 1_000_000);
        mint_and_vouch(&ctx, &v2, &b2, 2_000_000);
        mint_and_vouch(&ctx, &v3, &b3, 3_000_000);

        let borrowers = Vec::from_array(&ctx.env, [b1.clone(), b2.clone(), b3.clone()]);
        let result = ctx.client.batch_total_stake(&borrowers);

        assert_eq!(result.len(), 3);
        // Order must match input order.
        assert_eq!(result.get(0).unwrap().borrower, b1);
        assert_eq!(result.get(0).unwrap().total_stake, 1_000_000);
        assert_eq!(result.get(1).unwrap().borrower, b2);
        assert_eq!(result.get(1).unwrap().total_stake, 2_000_000);
        assert_eq!(result.get(2).unwrap().borrower, b3);
        assert_eq!(result.get(2).unwrap().total_stake, 3_000_000);
    }

    #[test]
    fn test_batch_result_order_matches_input_order() {
        let ctx = setup();
        // Deliberately put the higher-stake borrower first in the input Vec,
        // then verify the result order is identical.
        let b_high = Address::generate(&ctx.env);
        let b_low = Address::generate(&ctx.env);
        let v1 = Address::generate(&ctx.env);
        let v2 = Address::generate(&ctx.env);

        mint_and_vouch(&ctx, &v1, &b_high, 9_000_000);
        mint_and_vouch(&ctx, &v2, &b_low, 100_000);

        let borrowers = Vec::from_array(&ctx.env, [b_high.clone(), b_low.clone()]);
        let result = ctx.client.batch_total_stake(&borrowers);

        assert_eq!(result.get(0).unwrap().borrower, b_high);
        assert_eq!(result.get(0).unwrap().total_stake, 9_000_000);
        assert_eq!(result.get(1).unwrap().borrower, b_low);
        assert_eq!(result.get(1).unwrap().total_stake, 100_000);
    }

    #[test]
    fn test_batch_mixed_vouched_and_unvouched_borrowers() {
        let ctx = setup();
        let b_vouched = Address::generate(&ctx.env);
        let b_empty = Address::generate(&ctx.env);
        let voucher = Address::generate(&ctx.env);

        mint_and_vouch(&ctx, &voucher, &b_vouched, 5_000_000);

        let borrowers =
            Vec::from_array(&ctx.env, [b_vouched.clone(), b_empty.clone()]);
        let result = ctx.client.batch_total_stake(&borrowers);

        assert_eq!(result.len(), 2);
        assert_eq!(result.get(0).unwrap().total_stake, 5_000_000);
        assert_eq!(result.get(1).unwrap().total_stake, 0);
    }

    #[test]
    fn test_batch_result_consistent_with_individual_total_vouched() {
        let ctx = setup();
        let b1 = Address::generate(&ctx.env);
        let b2 = Address::generate(&ctx.env);
        let v1 = Address::generate(&ctx.env);
        let v2 = Address::generate(&ctx.env);
        let v3 = Address::generate(&ctx.env);

        mint_and_vouch(&ctx, &v1, &b1, 4_000_000);
        mint_and_vouch(&ctx, &v2, &b1, 1_000_000);
        mint_and_vouch(&ctx, &v3, &b2, 7_000_000);

        let borrowers = Vec::from_array(&ctx.env, [b1.clone(), b2.clone()]);
        let batch = ctx.client.batch_total_stake(&borrowers);

        let individual_b1 = ctx.client.total_vouched(&b1);
        let individual_b2 = ctx.client.total_vouched(&b2);

        assert_eq!(batch.get(0).unwrap().total_stake, individual_b1);
        assert_eq!(batch.get(1).unwrap().total_stake, individual_b2);
    }
}
