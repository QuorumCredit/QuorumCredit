//! Tests for issue #1393: generate_factor_performance_report must surface
//! sector/region aggregates, which are keyed by the loan's actual
//! sector/region string rather than the literal words "sector"/"region".
#[cfg(test)]
mod factor_report_sector_region_tests {
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

    /// Creates a loan and immediately marks it Repaid, so
    /// analyze_loan_attribution sees a resolved (non-Pending) outcome and
    /// actually feeds the factor aggregates.
    fn make_repaid_loan(s: &Setup, sector: &str, region: &str) -> u64 {
        let borrower = Address::generate(&s.env);
        do_vouch(s, &borrower, 1_000_000);
        s.client
            .request_loan(&borrower, &200_000, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        s.env.as_contract(&s.contract_id, || {
            let mut record = crate::helpers::get_active_loan_record(&s.env, &borrower).unwrap();
            record.status = crate::types::LoanStatus::Repaid;
            s.env
                .storage()
                .persistent()
                .set(&crate::types::DataKey::Loan(record.id), &record);
        });

        s.client.record_loan_performance_factors(
            &loan.id,
            &borrower,
            &700,
            &10_500,
            &String::from_str(&s.env, sector),
            &String::from_str(&s.env, region),
        );
        s.client.analyze_loan_attribution(&loan.id);

        loan.id
    }

    #[test]
    fn report_includes_aggregates_for_two_distinct_sectors() {
        let s = setup();
        make_repaid_loan(&s, "agriculture", "west-africa");
        make_repaid_loan(&s, "retail", "east-africa");

        let report = s.client.generate_factor_report();
        let has = |name: &str| report.factors.iter().any(|f| f.factor_name == String::from_str(&s.env, name));

        assert!(has("agriculture"), "report should include the 'agriculture' sector aggregate");
        assert!(has("retail"), "report should include the 'retail' sector aggregate");
        assert!(has("west-africa"), "report should include the 'west-africa' region aggregate");
        assert!(has("east-africa"), "report should include the 'east-africa' region aggregate");
    }

    /// #1393 regression: the report must never look up sector/region
    /// aggregates under the literal keys "sector"/"region" — no loan's actual
    /// sector/region value is ever named that, so any aggregate found under
    /// those exact keys would mean the orphaned-literal bug reappeared.
    #[test]
    fn report_never_includes_the_literal_sector_or_region_keys() {
        let s = setup();
        make_repaid_loan(&s, "agriculture", "west-africa");

        let report = s.client.generate_factor_report();
        assert!(!report.factors.iter().any(|f| f.factor_name == String::from_str(&s.env, "sector")));
        assert!(!report.factors.iter().any(|f| f.factor_name == String::from_str(&s.env, "region")));
    }

    #[test]
    fn sector_aggregate_reflects_the_correct_outcome_count() {
        let s = setup();
        make_repaid_loan(&s, "agriculture", "west-africa");
        make_repaid_loan(&s, "agriculture", "east-africa");

        let report = s.client.generate_factor_report();
        let agriculture = report
            .factors
            .iter()
            .find(|f| f.factor_name == String::from_str(&s.env, "agriculture"))
            .expect("agriculture aggregate should be present");

        assert_eq!(agriculture.loans_observed, 2);
        assert_eq!(agriculture.successes, 2);
    }
}
