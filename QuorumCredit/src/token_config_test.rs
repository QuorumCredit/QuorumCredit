#[cfg(test)]
mod token_config_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient, TokenConfig};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        xlm: Address,
        usdc: Address,
        admin: Address,
        admins: Vec<Address>,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);

        let xlm_id = env.register_stellar_asset_contract_v2(admin.clone());
        let usdc_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        // Fund contract with enough for loans + yield
        StellarAssetClient::new(&env, &xlm_id.address()).mint(&contract_id, &10_000_000);
        StellarAssetClient::new(&env, &usdc_id.address()).mint(&contract_id, &10_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &xlm_id.address());
        client.add_allowed_token(&admins, &usdc_id.address());

        // Advance past MIN_VOUCH_AGE
        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            xlm: xlm_id.address(),
            usdc: usdc_id.address(),
            admin,
            admins,
        }
    }

    fn purpose(env: &Env) -> String {
        String::from_str(env, "test")
    }

    // ── set_token_config / get_token_config ───────────────────────────────────

    #[test]
    fn test_set_and_get_token_config() {
        let s = setup();
        let cfg = TokenConfig {
            yield_bps: 500,
            slash_bps: 3000,
        };
        s.client.set_token_config(&s.admins, &s.usdc, &cfg);

        let stored = s.client.get_token_config(&s.usdc).unwrap();
        assert_eq!(stored.yield_bps, 500);
        assert_eq!(stored.slash_bps, 3000);
    }

    #[test]
    fn test_get_token_config_returns_none_when_unset() {
        let s = setup();
        assert!(s.client.get_token_config(&s.usdc).is_none());
    }

    // ── Token-specific yield BPS ──────────────────────────────────────────────

    #[test]
    fn test_token_specific_yield_applied_on_repay() {
        let s = setup();

        // Set USDC yield to 500 bps (5%) — different from global 200 bps (2%)
        s.client.set_token_config(
            &s.admins,
            &s.usdc,
            &TokenConfig {
                yield_bps: 500,
                slash_bps: 5000,
            },
        );

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake: i128 = 1_000_000;
        let loan_amount: i128 = 500_000;

        StellarAssetClient::new(&s.env, &s.usdc).mint(&voucher, &stake);
        s.client.vouch(&voucher, &borrower, &stake, &s.usdc);
        s.env.ledger().with_mut(|l| l.timestamp += 61);
        s.client
            .request_loan(&borrower, &loan_amount, &stake, &purpose(&s.env), &s.usdc);

        let loan = s.client.get_loan(&borrower).unwrap();
        // Expected yield = 500_000 * 500 / 10_000 = 25_000
        assert_eq!(loan.total_yield, 25_000);

        // Repay in full
        let total_owed = loan_amount + loan.total_yield;
        StellarAssetClient::new(&s.env, &s.usdc).mint(&borrower, &total_owed);
        s.client.repay(&borrower, &total_owed);

        // Voucher should receive stake + yield
        let usdc_client =
            soroban_sdk::token::TokenClient::new(&s.env, &s.usdc);
        let voucher_balance = usdc_client.balance(&voucher);
        assert_eq!(voucher_balance, stake + 25_000);
    }

    #[test]
    fn test_global_yield_used_when_no_token_config() {
        let s = setup();

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake: i128 = 1_000_000;
        let loan_amount: i128 = 500_000;

        StellarAssetClient::new(&s.env, &s.usdc).mint(&voucher, &stake);
        s.client.vouch(&voucher, &borrower, &stake, &s.usdc);
        s.env.ledger().with_mut(|l| l.timestamp += 61);
        s.client
            .request_loan(&borrower, &loan_amount, &stake, &purpose(&s.env), &s.usdc);

        let loan = s.client.get_loan(&borrower).unwrap();
        // Global yield_bps = 200 → 500_000 * 200 / 10_000 = 10_000
        assert_eq!(loan.total_yield, 10_000);
    }

    // ── Token-specific slash BPS ──────────────────────────────────────────────

    #[test]
    fn test_token_specific_slash_applied() {
        let s = setup();

        // Set USDC slash to 2000 bps (20%) — different from global 5000 bps (50%)
        s.client.set_token_config(
            &s.admins,
            &s.usdc,
            &TokenConfig {
                yield_bps: 200,
                slash_bps: 2000,
            },
        );

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake: i128 = 1_000_000;

        StellarAssetClient::new(&s.env, &s.usdc).mint(&voucher, &stake);
        s.client.vouch(&voucher, &borrower, &stake, &s.usdc);
        s.env.ledger().with_mut(|l| l.timestamp += 61);
        s.client
            .request_loan(&borrower, &500_000, &stake, &purpose(&s.env), &s.usdc);

        // Trigger slash via governance vote (voucher approves)
        s.client.vote_slash(&voucher, &borrower, &true);

        // Voucher should receive 80% of stake back (20% slashed)
        let usdc_client = soroban_sdk::token::TokenClient::new(&s.env, &s.usdc);
        let voucher_balance = usdc_client.balance(&voucher);
        assert_eq!(voucher_balance, 800_000); // 1_000_000 * (1 - 0.20)
    }

    #[test]
    fn test_global_slash_used_when_no_token_config() {
        let s = setup();

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake: i128 = 1_000_000;

        StellarAssetClient::new(&s.env, &s.usdc).mint(&voucher, &stake);
        s.client.vouch(&voucher, &borrower, &stake, &s.usdc);
        s.env.ledger().with_mut(|l| l.timestamp += 61);
        s.client
            .request_loan(&borrower, &500_000, &stake, &purpose(&s.env), &s.usdc);

        // Trigger slash — global slash_bps = 5000 (50%)
        s.client.vote_slash(&voucher, &borrower, &true);

        let usdc_client = soroban_sdk::token::TokenClient::new(&s.env, &s.usdc);
        let voucher_balance = usdc_client.balance(&voucher);
        assert_eq!(voucher_balance, 500_000); // 1_000_000 * (1 - 0.50)
    }
}
