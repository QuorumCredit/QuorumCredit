#[cfg(test)]
mod refinance_tests {
    use crate::{
        LoanStatus, RefinanceRecord, QuorumCreditContract,
        QuorumCreditContractClient,
    };
    use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env, Vec};

    fn setup_with_loan() -> (
        Env,
        QuorumCreditContractClient<'static>,
        Address,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(admin1.clone())
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &2, &token);

        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &token);
        token_client.mint(&voucher, &50_000_000);

        client.vouch(&voucher, &borrower, &20_000_000, &token, &None);

        let purpose = soroban_sdk::String::from_str(&env, "initial loan");
        client
            .request_loan(&borrower, &5_000_000, &1_000_000, &purpose, &token)
            .unwrap();

        (env, client, admin1, token, borrower, voucher)
    }

    #[test]
    fn test_refinance_loan() {
        let (_env, client, _admin1, token, borrower, _voucher) = setup_with_loan();

        let old_loan = client.get_loan(&borrower).unwrap();
        let old_loan_id = old_loan.id;

        let new_amount = old_loan.amount + old_loan.total_yield + 1_000_000;

        client
            .refinance_loan(
                &borrower,
                &new_amount,
                &1_000_000,
                &token,
            )
            .unwrap();

        let new_loan = client.get_loan(&borrower).unwrap();
        assert_ne!(new_loan.id, old_loan_id);
        assert_eq!(new_loan.amount, new_amount);
        assert_eq!(new_loan.status, LoanStatus::Active);
    }

    #[test]
    fn test_refinance_preserves_co_borrowers() {
        let (env, client, _admin1, token, borrower, _voucher) = setup_with_loan();

        let co = Address::generate(&env);
        client.add_co_borrower(&borrower, &co).unwrap();

        let old_loan = client.get_loan(&borrower).unwrap();
        let new_amount = old_loan.amount + old_loan.total_yield + 500_000;

        client
            .refinance_loan(&borrower, &new_amount, &1_000_000, &token)
            .unwrap();

        let co_borrowers = client.get_co_borrowers(&borrower);
        assert_eq!(co_borrowers.len(), 1);
        assert_eq!(co_borrowers.get(0).unwrap(), co);
    }

    #[test]
    fn test_refinance_record_stored() {
        let (_env, client, _admin1, token, borrower, _voucher) = setup_with_loan();

        let old_loan = client.get_loan(&borrower).unwrap();
        let new_amount = old_loan.amount + old_loan.total_yield + 500_000;

        client
            .refinance_loan(&borrower, &new_amount, &1_000_000, &token)
            .unwrap();

        let new_loan = client.get_loan(&borrower).unwrap();
        let record = client.get_refinance_record(&new_loan.id);
        assert!(record.is_some());
        let record: RefinanceRecord = record.unwrap().try_into().unwrap();
        assert_eq!(record.old_loan_id, old_loan.id);
        assert_eq!(record.new_loan_id, new_loan.id);
        assert_eq!(record.new_amount, new_amount);
    }

    #[test]
    fn test_cannot_refinance_when_loan_is_past_due() {
        let (env, client, _admin1, token, borrower, _voucher) = setup_with_loan();

        let old_loan = client.get_loan(&borrower).unwrap();
        env.ledger().set_timestamp(old_loan.deadline + 1);

        let result = client.try_refinance_loan(
            &borrower,
            &(old_loan.amount + old_loan.total_yield + 500_000),
            &1_000_000,
            &token,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_refinance_marks_old_loan_fully_repaid() {
        let (_env, client, _admin1, token, borrower, _voucher) = setup_with_loan();

        let old_loan = client.get_loan(&borrower).unwrap();
        let new_amount = old_loan.amount + old_loan.total_yield + 500_000;

        client
            .refinance_loan(&borrower, &new_amount, &1_000_000, &token)
            .unwrap();

        let new_loan = client.get_loan(&borrower).unwrap();
        assert_eq!(new_loan.amount_repaid, 0);
        assert_eq!(new_loan.status, LoanStatus::Active);
        let record = client.get_refinance_record(&new_loan.id).unwrap();
        let record: RefinanceRecord = record.try_into().unwrap();
        assert_eq!(record.old_amount, old_loan.amount);
        assert_eq!(record.new_amount, new_amount);
    }

    #[test]
    fn test_cannot_refinance_below_outstanding() {
        let (_env, client, _admin1, token, borrower, _voucher) = setup_with_loan();

        let result = client.try_refinance_loan(
            &borrower,
            &100,
            &100,
            &token,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_cannot_refinance_without_active_loan() {
        let (env, client, _admin1, _admin2, token) = {
            let env = Env::default();
            env.mock_all_auths();
            let deployer = Address::generate(&env);
            let admin1 = Address::generate(&env);
            let admin2 = Address::generate(&env);
            let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
            let token = env
                .register_stellar_asset_contract_v2(admin1.clone())
                .address();
            let contract_id = env.register_contract(None, QuorumCreditContract);
            let client = QuorumCreditContractClient::new(&env, &contract_id);
            client.initialize(&deployer, &admins, &2, &token);
            (env, client, admin1, admin2, token)
        };

        let borrower = Address::generate(&env);
        let result = client.try_refinance_loan(
            &borrower,
            &1_000_000,
            &1_000_000,
            &token,
        );
        assert!(result.is_err());
    }
}
