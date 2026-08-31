//! Tests for issue #1406: guarantor stake locking and coverage-claim payout.
#[cfg(test)]
mod guarantor_tests {
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

    fn fund(s: &Setup, addr: &Address, amount: i128) {
        StellarAssetClient::new(&s.env, &s.token).mint(addr, &amount);
    }

    fn token_balance(s: &Setup, addr: &Address) -> i128 {
        soroban_sdk::token::Client::new(&s.env, &s.token).balance(addr)
    }

    /// Marks the borrower's loan Defaulted directly in storage — this test
    /// suite is about the guarantor payout mechanics, not the default-detection
    /// path exercised elsewhere.
    fn mark_loan_defaulted(s: &Setup, borrower: &Address) {
        s.env.as_contract(&s.contract_id, || {
            let mut loan = crate::helpers::get_active_loan_record(&s.env, borrower).unwrap();
            loan.status = crate::types::LoanStatus::Defaulted;
            s.env
                .storage()
                .persistent()
                .set(&crate::types::DataKey::Loan(loan.id), &loan);
        });
    }

    #[test]
    fn request_guarantor_locks_stake_from_guarantor() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);
        s.client
            .request_loan(&borrower, &200_000, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        let guarantor = Address::generate(&s.env);
        fund(&s, &guarantor, 500_000);

        s.client
            .request_guarantor_for_loan(&loan.id, &guarantor, &150_000, &s.token);

        assert_eq!(token_balance(&s, &guarantor), 350_000, "150_000 should have moved out of the guarantor");
        assert_eq!(token_balance(&s, &s.contract_id), 150_000, "contract should now hold the locked collateral");
    }

    #[test]
    fn claim_guarantor_coverage_pays_out_to_vouchers_and_marks_claimed() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);
        s.client
            .request_loan(&borrower, &200_000, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        let guarantor = Address::generate(&s.env);
        fund(&s, &guarantor, 500_000);
        s.client
            .request_guarantor_for_loan(&loan.id, &guarantor, &150_000, &s.token);

        mark_loan_defaulted(&s, &borrower);

        let voucher_balance_before = s
            .env
            .as_contract(&s.contract_id, || {
                let vouches: Vec<crate::types::VouchRecord> = s
                    .env
                    .storage()
                    .persistent()
                    .get(&crate::types::DataKey::Vouches(borrower.clone()))
                    .unwrap();
                vouches.get(0).unwrap().voucher
            });
        let before = token_balance(&s, &voucher_balance_before);

        let paid = s.client.claim_guarantor_coverage(&loan.id);
        assert_eq!(paid, 150_000);

        let after = token_balance(&s, &voucher_balance_before);
        assert_eq!(after, before + 150_000, "the sole voucher should receive the entire coverage pro-rata");

        let record = s.client.get_guarantor_record(&loan.id);
        assert_eq!(record.status, crate::types::GuaranteeStatus::Claimed);
    }

    #[test]
    fn claim_guarantor_coverage_twice_is_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);
        s.client
            .request_loan(&borrower, &200_000, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        let guarantor = Address::generate(&s.env);
        fund(&s, &guarantor, 500_000);
        s.client
            .request_guarantor_for_loan(&loan.id, &guarantor, &150_000, &s.token);
        mark_loan_defaulted(&s, &borrower);

        s.client.claim_guarantor_coverage(&loan.id);
        let result = s.client.try_claim_guarantor_coverage(&loan.id);

        assert_eq!(
            result,
            Err(Ok(crate::errors::ContractError::GuarantorAlreadyClaimed))
        );
    }

    #[test]
    fn claim_guarantor_coverage_on_non_defaulted_loan_is_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);
        s.client
            .request_loan(&borrower, &200_000, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        let guarantor = Address::generate(&s.env);
        fund(&s, &guarantor, 500_000);
        s.client
            .request_guarantor_for_loan(&loan.id, &guarantor, &150_000, &s.token);

        // Loan is still Active — never marked Defaulted.
        let result = s.client.try_claim_guarantor_coverage(&loan.id);

        assert_eq!(
            result,
            Err(Ok(crate::errors::ContractError::InvalidStateTransition))
        );
    }

    #[test]
    fn release_guarantor_returns_locked_stake() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);
        s.client
            .request_loan(&borrower, &200_000, &0, &purpose(&s.env), &s.token);
        let loan = s.client.get_loan(&borrower).unwrap();

        let guarantor = Address::generate(&s.env);
        fund(&s, &guarantor, 500_000);
        s.client
            .request_guarantor_for_loan(&loan.id, &guarantor, &150_000, &s.token);
        assert_eq!(token_balance(&s, &guarantor), 350_000);

        s.client.release_guarantor(&loan.id);

        assert_eq!(token_balance(&s, &guarantor), 500_000, "released collateral should return to the guarantor");
    }
}
