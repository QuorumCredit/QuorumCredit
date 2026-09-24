//! Tests for issue #1391: request_loan must not discard the per-vouch
//! weighted yield computation in favor of a flat recalculation.
#[cfg(test)]
mod request_loan_yield_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        contract_id: Address,
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

        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            contract_id,
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

    fn yield_distribution_sum(s: &Setup, loan_id: u64) -> i128 {
        s.env.as_contract(&s.contract_id, || {
            let entries: soroban_sdk::Vec<crate::types::YieldDistributionEntry> = s
                .env
                .storage()
                .persistent()
                .get(&crate::types::DataKey::YieldDistribution(loan_id))
                .unwrap();
            entries.iter().map(|e| e.yield_amount).sum()
        })
    }

    #[test]
    fn total_yield_equals_sum_of_yield_distribution() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);

        s.client
            .request_loan(&borrower, &200_000, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        let distributed = yield_distribution_sum(&s, loan.id);

        assert_eq!(
            loan.total_yield, distributed,
            "loan.total_yield must match what repay() will actually pay out per voucher"
        );
    }

    /// Regression test with vouchers of varying age and stake, to catch any
    /// future drift between the weighted computation and loan.total_yield.
    #[test]
    fn total_yield_matches_distribution_with_multiple_vouchers_of_varying_age() {
        let s = setup();
        let borrower = Address::generate(&s.env);

        // First voucher, aged by advancing the ledger before the second vouches in.
        do_vouch(&s, &borrower, 2_000_000);
        s.env.ledger().with_mut(|l| l.timestamp += 30 * 24 * 60 * 60); // +30 days
        do_vouch(&s, &borrower, 3_000_000);
        s.env.ledger().with_mut(|l| l.timestamp += 10 * 24 * 60 * 60); // +10 more days
        do_vouch(&s, &borrower, 1_500_000);

        s.client
            .request_loan(&borrower, &500_000, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        let distributed = yield_distribution_sum(&s, loan.id);

        assert_eq!(loan.total_yield, distributed);
    }

    /// #1391 regression: the specific bug was a flat `amount * cfg.yield_bps /
    /// 10_000` silently overwriting the weighted total_yield after it was
    /// computed. An aged vouch earns an age bonus on top of the base rate, so
    /// the weighted total must come out *above* the flat calculation here —
    /// if the flat overwrite were reintroduced, this assertion would catch it
    /// even though the earlier tests' exact-match assertions technically
    /// wouldn't (they'd just start comparing the flat value to itself).
    #[test]
    fn weighted_yield_exceeds_flat_calculation_when_a_vouch_is_aged() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 2_000_000);
        s.env.ledger().with_mut(|l| l.timestamp += 60 * 24 * 60 * 60); // +60 days ages this vouch
        do_vouch(&s, &borrower, 2_000_000);

        let amount = 500_000i128;
        s.client
            .request_loan(&borrower, &amount, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        let flat = amount * crate::types::DEFAULT_YIELD_BPS / 10_000;
        assert!(
            loan.total_yield > flat,
            "aged vouch should earn an age bonus pushing total_yield ({}) above the flat calculation ({})",
            loan.total_yield, flat
        );

        let distributed = yield_distribution_sum(&s, loan.id);
        assert_eq!(loan.total_yield, distributed);
    }
}
