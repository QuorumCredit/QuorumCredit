#[cfg(test)]
mod amortization_schedule_tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

    fn setup(env: &Env) -> (Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        QuorumCreditContractClient::new(env, &contract_id).initialize(
            &deployer,
            &Vec::from_array(env, [admin.clone()]),
            &1,
            &token_id,
        );
        (contract_id, token_id, admin)
    }

    fn fund_vouch_and_loan(
        env: &Env,
        client: &QuorumCreditContractClient,
        token: &Address,
        amount: i128,
    ) -> (Address, Address) {
        let voucher = Address::generate(env);
        let borrower = Address::generate(env);
        StellarAssetClient::new(env, token).mint(&voucher, &(amount * 2));
        StellarAssetClient::new(env, token).mint(&client.address, &(amount * 10));
        client.vouch(&voucher, &borrower, &(amount * 2), token);
        client.request_loan(
            &borrower,
            &amount,
            &(amount * 2),
            &soroban_sdk::String::from_str(env, "test"),
            token,
        );
        (voucher, borrower)
    }

    /// #641: loan record has a non-empty amortization_schedule after request_loan.
    #[test]
    fn test_amortization_schedule_created_on_loan() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (_voucher, borrower) = fund_vouch_and_loan(&env, &client, &token_id, 1_000_000);

        let loan = client.get_loan(&borrower).expect("loan should exist");
        assert!(!loan.amortization_schedule.is_empty(), "schedule should not be empty");
    }

    /// #641: installment numbers are sequential starting from 1.
    #[test]
    fn test_amortization_installment_numbers_sequential() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (_voucher, borrower) = fund_vouch_and_loan(&env, &client, &token_id, 1_000_000);

        let loan = client.get_loan(&borrower).expect("loan should exist");
        for (i, entry) in loan.amortization_schedule.iter().enumerate() {
            assert_eq!(entry.installment_number, (i + 1) as u32);
        }
    }

    /// #641: sum of all installment amounts equals principal + yield.
    #[test]
    fn test_amortization_schedule_sums_to_total_owed() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (_voucher, borrower) = fund_vouch_and_loan(&env, &client, &token_id, 1_000_000);

        let loan = client.get_loan(&borrower).expect("loan should exist");
        let total_owed = loan.amount + loan.total_yield;
        let schedule_sum: i128 = loan.amortization_schedule.iter().map(|e| e.amount_due).sum();
        assert_eq!(schedule_sum, total_owed);
    }

    /// #641: all installments start as unpaid.
    #[test]
    fn test_amortization_installments_start_unpaid() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (_voucher, borrower) = fund_vouch_and_loan(&env, &client, &token_id, 1_000_000);

        let loan = client.get_loan(&borrower).expect("loan should exist");
        for entry in loan.amortization_schedule.iter() {
            assert!(!entry.paid, "installment {} should start unpaid", entry.installment_number);
        }
    }

    /// #641: repaying marks installments as paid in order.
    #[test]
    fn test_repay_marks_installments_paid() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id, _admin) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let (_, borrower) = fund_vouch_and_loan(&env, &client, &token_id, 1_000_000);

        let loan = client.get_loan(&borrower).expect("loan should exist");
        let first_installment = loan.amortization_schedule.get(0).unwrap();

        // Pay exactly the first installment
        StellarAssetClient::new(&env, &token_id).mint(&borrower, &first_installment.amount_due);
        client.repay(&borrower, &first_installment.amount_due);

        let updated = client.get_loan(&borrower).expect("loan should exist");
        assert!(updated.amortization_schedule.get(0).unwrap().paid, "first installment should be paid");
        if updated.amortization_schedule.len() > 1 {
            assert!(!updated.amortization_schedule.get(1).unwrap().paid, "second installment should still be unpaid");
        }
    }
}
