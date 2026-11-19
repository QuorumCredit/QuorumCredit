//! Tests for issue #2068: when accrued interest would overflow `i128` during
//! repayment, the call must fail with `ContractError::ArithmeticError` instead
//! of silently saturating the debt to `i128::MAX`.
#[cfg(test)]
mod loan_accrued_interest_overflow_tests {
    use crate::errors::ContractError;
    use crate::types::{DataKey, LoanRecord, SECS_PER_DAY};
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token: Address,
        contract_id: Address,
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
            token: token_id.address(),
            contract_id,
        }
    }

    #[test]
    fn repay_accrued_interest_overflow_returns_arithmetic_error() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &1_000_000);
        s.client.vouch(&voucher, &borrower, &1_000_000, &s.token, &None);
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &1_000_000);

        s.client.request_loan(
            &borrower,
            &100_000,
            &0,
            &String::from_str(&s.env, "test loan"),
            &s.token,
        );
        let loan = s.client.get_loan(&borrower).unwrap();

        // Drive accrued interest to the boundary, then let a full day pass so
        // the next repayment computes a strictly positive interest amount.
        s.env.as_contract(&s.contract_id, || {
            let mut stored: LoanRecord = s
                .env
                .storage()
                .persistent()
                .get(&DataKey::Loan(loan.id))
                .unwrap();
            stored.accrued_interest = i128::MAX;
            s.env
                .storage()
                .persistent()
                .set(&DataKey::Loan(loan.id), &stored);
        });
        s.env.ledger().with_mut(|l| l.timestamp += 2 * SECS_PER_DAY);

        let result = s.client.try_repay(&borrower, &1_000);
        assert_eq!(result, Err(Ok(ContractError::ArithmeticError)));
    }
}
