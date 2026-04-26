#[cfg(test)]
mod tests {
    use crate::loan::{request_loan, repay};
    use crate::types::LoanStatus;
    use crate::vouch::vouch;
    use soroban_sdk::{Address, Env, String};

    #[test]
    fn test_prepayment_penalty_calculation() {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::random(&env);
        let admin = Address::random(&env);
        let borrower = Address::random(&env);
        let voucher = Address::random(&env);
        let token = Address::random(&env);

        // Initialize contract
        crate::lib::QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            soroban_sdk::vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        // Setup token mock
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        token_client.set_balance(&voucher, &1_000_000);
        token_client.set_balance(&borrower, &1_000_000);
        token_client.set_balance(&env.current_contract_address(), &10_000_000);

        // Vouch for borrower
        vouch(
            env.clone(),
            voucher.clone(),
            borrower.clone(),
            100_000,
            token.clone(),
        )
        .unwrap();

        // Request loan
        request_loan(
            env.clone(),
            borrower.clone(),
            50_000,
            100_000,
            String::from_slice(&env, "test loan"),
            token.clone(),
        )
        .unwrap();

        // Repay loan early (prepayment)
        let loan = crate::loan::get_loan(env.clone(), borrower.clone()).unwrap();
        let total_owed = loan.amount + loan.total_yield;

        // Repay full amount
        repay(env.clone(), borrower.clone(), total_owed).unwrap();

        // Verify loan is repaid
        let updated_loan = crate::loan::get_loan(env.clone(), borrower.clone()).unwrap();
        assert_eq!(updated_loan.status, LoanStatus::Repaid);
    }
}
