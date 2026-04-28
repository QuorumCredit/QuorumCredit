/// Regression Test Suite
///
/// Each test corresponds to a previously fixed bug. The test name and comment
/// reference the issue number so the fix can be traced back to the original report.
/// These tests run on every CI build to prevent regressions.
#[cfg(test)]
mod regression_tests {
    use crate::{LoanStatus, QuorumCreditContract, QuorumCreditContractClient};
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

        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup { env, client, contract_id, token: token_id.address() }
    }

    fn mint(s: &Setup, to: &Address, amount: i128) {
        StellarAssetClient::new(&s.env, &s.token).mint(to, &amount);
    }

    fn purpose(env: &Env) -> String {
        String::from_str(env, "regression test")
    }

    // ── Regression: Issue 108 — Borrower repaying another borrower's loan ────
    #[test]
    fn regression_108_unauthorized_repay_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let attacker = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        mint(&s, &voucher, 10_000);
        mint(&s, &attacker, 10_000);

        s.client.vouch(&voucher, &borrower, &10_000, &s.token);
        s.client
            .request_loan(&borrower, &5_000, &5_000, &purpose(&s.env), &s.token);

        let result = s.client.try_repay(&attacker, &5_100);
        assert!(
            result.is_err(),
            "regression_108: attacker should not be able to repay another borrower's loan"
        );
        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Active);
    }

    // ── Regression: Issue 109 — Slash after repay ────────────────────────────
    #[test]
    fn regression_109_slash_after_repay_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        mint(&s, &voucher, 10_000);
        mint(&s, &borrower, 5_000);

        s.client.vouch(&voucher, &borrower, &10_000, &s.token);
        s.client
            .request_loan(&borrower, &5_000, &5_000, &purpose(&s.env), &s.token);

        let loan = s.client.get_loan(&borrower).unwrap();
        s.client.repay(&borrower, &(loan.amount + loan.total_yield));
        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Repaid);

        let result = s.client.try_vote_slash(&voucher, &borrower, &true);
        assert!(
            result.is_err(),
            "regression_109: slash after repay should be rejected"
        );
    }

    // ── Regression: Issue 112 — Slash treasury accounting ────────────────────
    #[test]
    fn regression_112_slash_treasury_increases_on_slash() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        mint(&s, &voucher, 10_000);

        s.client.vouch(&voucher, &borrower, &10_000, &s.token);
        s.client
            .request_loan(&borrower, &5_000, &5_000, &purpose(&s.env), &s.token);

        let treasury_before = s.client.get_slash_treasury_balance();
        s.client.vote_slash(&voucher, &borrower, &true);
        let treasury_after = s.client.get_slash_treasury_balance();

        assert!(
            treasury_after > treasury_before,
            "regression_112: slash treasury should increase (before={treasury_before}, after={treasury_after})"
        );
    }

    // ── Regression: Issue 114 — Loan disbursement cannot exceed contract balance
    #[test]
    fn regression_114_loan_disbursement_cannot_exceed_contract_balance() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        mint(&s, &voucher, 500);
        s.client.vouch(&voucher, &borrower, &500, &s.token);

        let huge = 10_000_000_000i128;
        let result =
            s.client
                .try_request_loan(&borrower, &huge, &500, &purpose(&s.env), &s.token);
        assert!(
            result.is_err(),
            "regression_114: loan exceeding contract balance should be rejected"
        );
    }

    // ── Regression: Duplicate vouch rejected ─────────────────────────────────
    #[test]
    fn regression_duplicate_vouch_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        mint(&s, &voucher, 20_000);
        s.client.vouch(&voucher, &borrower, &5_000, &s.token);

        let result = s.client.try_vouch(&voucher, &borrower, &5_000, &s.token);
        assert!(result.is_err(), "regression: duplicate vouch should be rejected");
    }

    // ── Regression: Zero-stake vouch rejected ────────────────────────────────
    #[test]
    fn regression_zero_stake_vouch_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        let result = s.client.try_vouch(&voucher, &borrower, &0, &s.token);
        assert!(result.is_err(), "regression: zero-stake vouch should be rejected");
    }

    // ── Regression: Self-vouch rejected ──────────────────────────────────────
    #[test]
    fn regression_self_vouch_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        mint(&s, &borrower, 10_000);

        let result = s.client.try_vouch(&borrower, &borrower, &5_000, &s.token);
        assert!(result.is_err(), "regression: self-vouch should be rejected");
    }

    // ── Regression: Loan below minimum amount rejected ───────────────────────
    #[test]
    fn regression_loan_below_min_amount_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        mint(&s, &voucher, 10_000);
        s.client.vouch(&voucher, &borrower, &10_000, &s.token);

        let result =
            s.client
                .try_request_loan(&borrower, &1, &1, &purpose(&s.env), &s.token);
        assert!(
            result.is_err(),
            "regression: loan below min_loan_amount should be rejected"
        );
    }

    // ── Regression: Vouch during active loan rejected ────────────────────────
    #[test]
    fn regression_vouch_during_active_loan_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher1 = Address::generate(&s.env);
        let voucher2 = Address::generate(&s.env);

        mint(&s, &voucher1, 10_000);
        mint(&s, &voucher2, 10_000);

        s.client.vouch(&voucher1, &borrower, &10_000, &s.token);
        s.client
            .request_loan(&borrower, &5_000, &5_000, &purpose(&s.env), &s.token);

        let result = s.client.try_vouch(&voucher2, &borrower, &5_000, &s.token);
        assert!(
            result.is_err(),
            "regression: vouch during active loan should be rejected"
        );
    }

    // ── Regression: Double initialize rejected ───────────────────────────────
    #[test]
    fn regression_double_initialize_rejected() {
        let s = setup();
        let deployer2 = Address::generate(&s.env);
        let admin2 = Address::generate(&s.env);
        let admins2 = Vec::from_array(&s.env, [admin2]);

        let result = s
            .client
            .try_initialize(&deployer2, &admins2, &1, &s.token);
        assert!(
            result.is_err(),
            "regression: second initialize should be rejected"
        );
    }
}
