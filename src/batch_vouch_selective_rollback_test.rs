/// Issue #1055: Tests for batch_vouch selective rollback semantics.
/// Successful entries are committed; failed entries are skipped per-entry.
#[cfg(test)]
mod batch_vouch_selective_rollback_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::Address as _, token::StellarAssetClient, Address, Env, Vec,
    };

    struct Ctx {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token: Address,
    }

    fn setup() -> Ctx {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token).mint(&contract_id, &100_000_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1u32, &token);
        Ctx { env, client, token }
    }

    fn mint(ctx: &Ctx, to: &Address, amount: i128) {
        StellarAssetClient::new(&ctx.env, &ctx.token).mint(to, &amount);
    }

    // ── all-success ───────────────────────────────────────────────────────────

    #[test]
    fn test_all_valid_entries_succeed() {
        let ctx = setup();
        let voucher = Address::generate(&ctx.env);
        let b1 = Address::generate(&ctx.env);
        let b2 = Address::generate(&ctx.env);
        mint(&ctx, &voucher, 2_000_000);

        let borrowers = Vec::from_array(&ctx.env, [b1.clone(), b2.clone()]);
        let stakes = Vec::from_array(&ctx.env, [1_000_000i128, 1_000_000i128]);

        let results = ctx.client.batch_vouch(&voucher, &borrowers, &stakes, &ctx.token, &None);

        assert_eq!(results.len(), 2);
        assert!(results.get(0).unwrap().success);
        assert_eq!(results.get(0).unwrap().error_code, None);
        assert!(results.get(1).unwrap().success);
        assert_eq!(results.get(1).unwrap().error_code, None);
        // Both vouches committed to storage
        assert!(ctx.client.vouch_exists(&voucher, &b1));
        assert!(ctx.client.vouch_exists(&voucher, &b2));
    }

    // ── mixed success/failure ─────────────────────────────────────────────────

    #[test]
    fn test_zero_stake_entry_skipped_valid_entry_committed() {
        let ctx = setup();
        let voucher = Address::generate(&ctx.env);
        let b_valid = Address::generate(&ctx.env);
        let b_invalid = Address::generate(&ctx.env);
        mint(&ctx, &voucher, 1_000_000);

        let borrowers = Vec::from_array(&ctx.env, [b_valid.clone(), b_invalid.clone()]);
        let stakes = Vec::from_array(&ctx.env, [1_000_000i128, 0i128]);

        let results = ctx.client.batch_vouch(&voucher, &borrowers, &stakes, &ctx.token, &None);

        assert_eq!(results.len(), 2);
        // First entry committed
        assert!(results.get(0).unwrap().success);
        assert_eq!(results.get(0).unwrap().error_code, None);
        // Second entry skipped with an error code
        assert!(!results.get(1).unwrap().success);
        assert!(results.get(1).unwrap().error_code.is_some());

        assert!(ctx.client.vouch_exists(&voucher, &b_valid));
        assert!(!ctx.client.vouch_exists(&voucher, &b_invalid));
    }

    #[test]
    fn test_self_vouch_entry_skipped_others_committed() {
        let ctx = setup();
        let voucher = Address::generate(&ctx.env);
        let b1 = Address::generate(&ctx.env);
        let b2 = Address::generate(&ctx.env);
        mint(&ctx, &voucher, 2_000_000);

        // Second entry is a self-vouch (voucher == borrower) — should be skipped
        let borrowers = Vec::from_array(&ctx.env, [b1.clone(), voucher.clone(), b2.clone()]);
        let stakes = Vec::from_array(&ctx.env, [1_000_000i128, 500_000i128, 1_000_000i128]);

        let results = ctx.client.batch_vouch(&voucher, &borrowers, &stakes, &ctx.token, &None);

        assert_eq!(results.len(), 3);
        assert!(results.get(0).unwrap().success);
        assert!(!results.get(1).unwrap().success);
        assert!(results.get(1).unwrap().error_code.is_some());
        assert!(results.get(2).unwrap().success);

        assert!(ctx.client.vouch_exists(&voucher, &b1));
        assert!(!ctx.client.vouch_exists(&voucher, &voucher));
        assert!(ctx.client.vouch_exists(&voucher, &b2));
    }

    #[test]
    fn test_duplicate_vouch_entry_skipped() {
        let ctx = setup();
        let voucher = Address::generate(&ctx.env);
        let b1 = Address::generate(&ctx.env);
        let b2 = Address::generate(&ctx.env);
        mint(&ctx, &voucher, 3_000_000);

        // First vouch b1 individually so the duplicate in batch is detected
        ctx.client.vouch(&voucher, &b1, &1_000_000, &ctx.token, &None);
        // advance ledger past cooldown
        ctx.env.ledger().with_mut(|l| l.timestamp += 86_401);

        let borrowers = Vec::from_array(&ctx.env, [b1.clone(), b2.clone()]);
        let stakes = Vec::from_array(&ctx.env, [500_000i128, 1_000_000i128]);

        let results = ctx.client.batch_vouch(&voucher, &borrowers, &stakes, &ctx.token, &None);

        assert_eq!(results.len(), 2);
        // b1 duplicate — skipped
        assert!(!results.get(0).unwrap().success);
        assert!(results.get(0).unwrap().error_code.is_some());
        // b2 valid — committed
        assert!(results.get(1).unwrap().success);
        assert!(ctx.client.vouch_exists(&voucher, &b2));
    }

    // ── all-failure ───────────────────────────────────────────────────────────

    #[test]
    fn test_all_invalid_entries_return_all_failed() {
        let ctx = setup();
        let voucher = Address::generate(&ctx.env);
        let b1 = Address::generate(&ctx.env);
        let b2 = Address::generate(&ctx.env);
        // No mint — insufficient balance for both

        let borrowers = Vec::from_array(&ctx.env, [b1.clone(), b2.clone()]);
        let stakes = Vec::from_array(&ctx.env, [1_000_000i128, 1_000_000i128]);

        let results = ctx.client.batch_vouch(&voucher, &borrowers, &stakes, &ctx.token, &None);

        assert_eq!(results.len(), 2);
        assert!(!results.get(0).unwrap().success);
        assert!(!results.get(1).unwrap().success);
        assert!(!ctx.client.vouch_exists(&voucher, &b1));
        assert!(!ctx.client.vouch_exists(&voucher, &b2));
    }

    // ── result metadata ───────────────────────────────────────────────────────

    #[test]
    fn test_result_contains_correct_borrower_and_stake() {
        let ctx = setup();
        let voucher = Address::generate(&ctx.env);
        let b1 = Address::generate(&ctx.env);
        mint(&ctx, &voucher, 500_000);

        let borrowers = Vec::from_array(&ctx.env, [b1.clone()]);
        let stakes = Vec::from_array(&ctx.env, [500_000i128]);

        let results = ctx.client.batch_vouch(&voucher, &borrowers, &stakes, &ctx.token, &None);

        assert_eq!(results.len(), 1);
        let r = results.get(0).unwrap();
        assert_eq!(r.borrower, b1);
        assert_eq!(r.stake, 500_000);
        assert!(r.success);
    }

    // ── empty / length-mismatch batch still fails the whole call ─────────────

    #[test]
    fn test_empty_batch_returns_error() {
        let ctx = setup();
        let voucher = Address::generate(&ctx.env);
        let borrowers: Vec<Address> = Vec::new(&ctx.env);
        let stakes: Vec<i128> = Vec::new(&ctx.env);

        let result = ctx.client.try_batch_vouch(&voucher, &borrowers, &stakes, &ctx.token, &None);
        assert!(result.is_err());
    }

    #[test]
    fn test_length_mismatch_returns_error() {
        let ctx = setup();
        let voucher = Address::generate(&ctx.env);
        let b1 = Address::generate(&ctx.env);
        let borrowers = Vec::from_array(&ctx.env, [b1]);
        let stakes: Vec<i128> = Vec::new(&ctx.env);

        let result = ctx.client.try_batch_vouch(&voucher, &borrowers, &stakes, &ctx.token, &None);
        assert!(result.is_err());
    }
}
