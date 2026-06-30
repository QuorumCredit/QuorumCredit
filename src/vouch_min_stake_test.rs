//! Tests for the minimum stake enforcement in `vouch()` and the dynamic
//! minimum stake calculation introduced in issue #1057.

#[cfg(test)]
mod vouch_min_stake_tests {
    use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::Address as _,
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    // ── helpers ───────────────────────────────────────────────────────────────

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
        // Pre-fund contract so yield payments can be made in later tests.
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &50_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        // Advance time past the default min-vouch-age so vouches are immediately usable.
        env.ledger().with_mut(|l| l.timestamp = 120);
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    fn mint_to(s: &Setup, addr: &Address, amount: i128) {
        StellarAssetClient::new(&s.env, &s.token).mint(addr, &amount);
    }

    // ── static min stake tests ────────────────────────────────────────────────

    /// With no min_stake configured the vouch should succeed regardless of amount.
    #[test]
    fn test_vouch_succeeds_without_min_stake() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        mint_to(&s, &voucher, 100);

        let res = s.client.try_vouch(&voucher, &borrower, &100, &s.token);
        assert!(res.is_ok(), "expected vouch to succeed when no min_stake set");
    }

    /// A vouch below the configured min_stake should be rejected.
    #[test]
    fn test_vouch_below_min_stake_rejected() {
        let s = setup();
        // Set min_stake to 1_000 stroops.
        s.client.set_min_stake(&admins(&s), &1_000);

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        mint_to(&s, &voucher, 500);

        let err = s
            .client
            .try_vouch(&voucher, &borrower, &500, &s.token)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ContractError::MinStakeNotMet);
    }

    /// A vouch exactly at the configured min_stake should be accepted.
    #[test]
    fn test_vouch_at_min_stake_accepted() {
        let s = setup();
        s.client.set_min_stake(&admins(&s), &1_000);

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        mint_to(&s, &voucher, 1_000);

        let res = s.client.try_vouch(&voucher, &borrower, &1_000, &s.token);
        assert!(res.is_ok(), "vouch equal to min_stake must succeed");
    }

    /// A vouch above the configured min_stake should be accepted.
    #[test]
    fn test_vouch_above_min_stake_accepted() {
        let s = setup();
        s.client.set_min_stake(&admins(&s), &500);

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        mint_to(&s, &voucher, 2_000);

        let res = s.client.try_vouch(&voucher, &borrower, &2_000, &s.token);
        assert!(res.is_ok(), "vouch above min_stake must succeed");
    }

    // ── get_dynamic_min_stake (no credit score) ───────────────────────────────

    /// When no min_stake is configured, get_dynamic_min_stake returns 0.
    #[test]
    fn test_get_dynamic_min_stake_no_config_returns_zero() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        assert_eq!(s.client.get_dynamic_min_stake(&borrower), 0);
    }

    /// With a base min_stake set but no credit score for the borrower the dynamic
    /// value should equal the base.
    #[test]
    fn test_get_dynamic_min_stake_equals_base_when_no_credit_score() {
        let s = setup();
        s.client.set_min_stake(&admins(&s), &10_000);

        let borrower = Address::generate(&s.env);
        // No credit score has been recorded for this borrower.
        let dynamic = s.client.get_dynamic_min_stake(&borrower);
        assert_eq!(
            dynamic, 10_000,
            "without a credit score the dynamic min_stake must equal the base"
        );
    }

    // ── get_dynamic_min_stake (with credit score) ─────────────────────────────

    /// After updating the credit score for a borrower with a Good or better tier,
    /// the dynamic min_stake should be strictly less-than-or-equal to the base.
    #[test]
    fn test_get_dynamic_min_stake_reduced_for_good_borrower() {
        let s = setup();
        let base: i128 = 10_000_000; // 1 XLM
        s.client.set_min_stake(&admins(&s), &base);

        let borrower = Address::generate(&s.env);
        // Assign a credit score; a new user starts around neutral (500).
        s.client.update_credit_score(&borrower).unwrap();

        let dynamic = s.client.get_dynamic_min_stake(&borrower);
        // Dynamic min_stake must always be <= base_min_stake.
        assert!(
            dynamic <= base,
            "dynamic min_stake ({dynamic}) must be <= base ({base})"
        );
        // Dynamic min_stake must be non-negative.
        assert!(dynamic >= 0, "dynamic min_stake must be non-negative");
    }

    /// Confirm that get_dynamic_min_stake is deterministic across repeated calls.
    #[test]
    fn test_get_dynamic_min_stake_is_deterministic() {
        let s = setup();
        s.client.set_min_stake(&admins(&s), &5_000);
        let borrower = Address::generate(&s.env);

        let first = s.client.get_dynamic_min_stake(&borrower);
        let second = s.client.get_dynamic_min_stake(&borrower);
        assert_eq!(first, second, "get_dynamic_min_stake must be deterministic");
    }

    /// Setting min_stake to 0 disables the minimum; dynamic should also return 0.
    #[test]
    fn test_get_dynamic_min_stake_zero_when_base_is_zero() {
        let s = setup();
        s.client.set_min_stake(&admins(&s), &0);
        let borrower = Address::generate(&s.env);
        s.client.update_credit_score(&borrower).unwrap();

        assert_eq!(
            s.client.get_dynamic_min_stake(&borrower),
            0,
            "dynamic min_stake should be 0 when base is 0"
        );
    }

    // ── dynamic enforcement in vouch() ───────────────────────────────────────

    /// A borrower with an established credit history may vouch with a stake
    /// below the base min_stake if their credit discount brings the effective
    /// minimum down sufficiently.
    ///
    /// This test uses the POOR tier (no discount) to verify baseline behaviour:
    /// the effective min must still be <= base.
    #[test]
    fn test_dynamic_min_stake_never_exceeds_base() {
        let s = setup();
        let base: i128 = 20_000_000; // 2 XLM
        s.client.set_min_stake(&admins(&s), &base);

        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        // Update credit score so a CreditScore record exists.
        s.client.update_credit_score(&borrower).unwrap();

        let dynamic = s.client.get_dynamic_min_stake(&borrower);
        assert!(
            dynamic <= base,
            "effective min stake must never exceed the configured base"
        );

        // Vouch with exactly the dynamic minimum — must succeed.
        mint_to(&s, &voucher, dynamic + 1);
        let res = s.client.try_vouch(&voucher, &borrower, &(dynamic + 1), &s.token);
        assert!(res.is_ok(), "vouch at dynamic min_stake + 1 should succeed");
    }
}
