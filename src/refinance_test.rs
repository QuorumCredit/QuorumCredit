#[cfg(test)]
mod tests {
    use crate::helpers::config;
    use crate::loan::{refinance_loan, request_loan, repay};
    use crate::types::{Config, DataKey, LoanStatus};
    use crate::vouch::vouch;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::{Address, Env, String};

    #[test]
    fn test_refinance_loan() {
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

        // Request initial loan
        request_loan(
            env.clone(),
            borrower.clone(),
            50_000,
            100_000,
            String::from_slice(&env, "initial loan"),
            token.clone(),
        )
        .unwrap();

        // Refinance with new amount
        refinance_loan(
            env.clone(),
            borrower.clone(),
            75_000,
            100_000,
            token.clone(),
        )
        .unwrap();

        // Verify new loan is active
        let loan = crate::loan::get_loan(env.clone(), borrower.clone()).unwrap();
        assert_eq!(loan.amount, 75_000);
        assert_eq!(loan.status, LoanStatus::Active);
        assert_eq!(loan.is_refinance, true);
    }
}
