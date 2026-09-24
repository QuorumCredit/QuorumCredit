//! Tests for issue #1395: removing the duplicate total_owed/outstanding
//! check in `loan::repay` and switching its remaining validation from a bare
//! `assert!` panic to a typed `ContractError::InvalidAmount` return.
#[cfg(test)]
mod repay_validation_tests {
    use crate::errors::ContractError;
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

    fn fund_borrower(s: &Setup, borrower: &Address, amount: i128) {
        StellarAssetClient::new(&s.env, &s.token).mint(borrower, &amount);
    }

    /// A payment that exactly equals the outstanding balance should succeed
    /// and fully clear the loan — the boundary case for the `payment >
    /// outstanding` check that replaced the old `assert!`.
    #[test]
    fn repay_exact_outstanding_amount_succeeds() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);
        fund_borrower(&s, &borrower, 1_000_000);

        s.client
            .request_loan(&borrower, &100_000, &0, &purpose(&s.env), &s.token);

        let loan = s.client.get_loan(&borrower).unwrap();
        let outstanding = loan.amount + loan.total_yield + loan.accrued_interest - loan.amount_repaid;

        // Same ledger second as the request — no interest has accrued yet, so
        // `outstanding` above already matches what `repay` will compute.
        s.client.repay(&borrower, &outstanding);

        let after = s.client.get_loan(&borrower);
        // Fully repaid: either the loan record is cleared or amount_repaid
        // matches total_owed, depending on how repay finalizes a paid-off loan.
        if let Some(after) = after {
            assert!(after.amount_repaid >= after.amount + after.total_yield + after.accrued_interest);
        }
    }

    /// A payment one unit over the outstanding balance must be rejected with
    /// the typed `ContractError::InvalidAmount`, not a bare panic — this is
    /// the behavior the removed `assert!` used to enforce via an untyped
    /// panic string.
    #[test]
    fn repay_over_outstanding_returns_invalid_amount() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);
        fund_borrower(&s, &borrower, 1_000_000);

        s.client
            .request_loan(&borrower, &100_000, &0, &purpose(&s.env), &s.token);

        let loan = s.client.get_loan(&borrower).unwrap();
        let outstanding = loan.amount + loan.total_yield + loan.accrued_interest - loan.amount_repaid;

        let result = s.client.try_repay(&borrower, &(outstanding + 1));
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
    }

    /// Removing the redundant first total_owed/outstanding computation
    /// should only ever reduce `repay`'s CPU cost, never regress it. Rather
    /// than compare against a stored "before" number this pins the call to
    /// the budget already documented for `repay` (typical, 1 voucher) in
    /// docs/gas-budgets.md, so a future change re-introducing redundant work
    /// (here or elsewhere in the function) trips this ceiling.
    #[test]
    fn repay_stays_within_documented_gas_budget() {
        const REPAY_TYPICAL_CPU_BUDGET: u64 = 5_000_000;

        let s = setup();
        let borrower = Address::generate(&s.env);
        do_vouch(&s, &borrower, 1_000_000);
        fund_borrower(&s, &borrower, 1_000_000);

        s.client
            .request_loan(&borrower, &100_000, &0, &purpose(&s.env), &s.token);

        s.env.cost_estimate().budget().reset_default();
        s.client.repay(&borrower, &1_000);
        let cpu = s.env.cost_estimate().budget().cpu_instruction_cost();

        assert!(
            cpu < REPAY_TYPICAL_CPU_BUDGET,
            "repay used {cpu} CPU instructions, over the documented {REPAY_TYPICAL_CPU_BUDGET}-instruction budget"
        );
    }
}
