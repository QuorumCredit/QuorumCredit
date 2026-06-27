/// Issue #939: Storage Compaction — archive inactive loans
#[cfg(test)]
mod storage_compaction_tests {
    use crate::{LoanStatus, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
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
        // Mint enough to cover loans + yield reserve
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &500_000_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 200);
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    /// Helper: vouch for borrower, advance time past min vouch age, request and
    /// immediately repay a loan so it ends up in Repaid status.
    fn create_repaid_loan(s: &Setup) -> (Address, u64) {
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake: i128 = 10_000_000_000; // 1000 XLM
        let amount: i128 = 1_000_000_000; // 100 XLM

        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake);
        s.client.vouch(&voucher, &borrower, &stake, &s.token, &None);
        s.env.ledger().with_mut(|l| l.timestamp += 90_000); // advance past min vouch age

        s.client.request_loan(
            &borrower,
            &amount,
            &stake,
            &String::from_str(&s.env, "archive test"),
            &s.token,
        );

        // Get the loan id before repaying
        let loan_id = s.client.get_loan(&borrower).unwrap().id;

        // Mint repayment amount to borrower (principal + yield)
        let repayment = amount + amount * 200 / 10_000;
        StellarAssetClient::new(&s.env, &s.token).mint(&borrower, &repayment);
        s.client.repay(&borrower, &repayment);

        (borrower, loan_id)
    }

    #[test]
    fn test_archive_repaid_loan_removes_full_record() {
        let s = setup();
        let (_borrower, loan_id) = create_repaid_loan(&s);

        // Archive it
        s.client.archive_loan(&admins(&s), &loan_id);

        // The archived compact record should exist.
        let archived = s.client.get_archived_loan(&loan_id);
        assert!(archived.is_some());
        let rec = archived.unwrap();
        assert_eq!(rec.loan_id, loan_id);
        assert_eq!(rec.status, LoanStatus::Repaid);
        assert!(rec.repayment_timestamp.is_some());
    }

    #[test]
    fn test_archive_active_loan_fails() {
        let s = setup();
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake: i128 = 5_000_000_000;
        let amount: i128 = 500_000_000;

        StellarAssetClient::new(&s.env, &s.token).mint(&voucher, &stake);
        s.client.vouch(&voucher, &borrower, &stake, &s.token, &None);
        s.env.ledger().with_mut(|l| l.timestamp += 90_000);

        s.client.request_loan(
            &borrower,
            &amount,
            &stake,
            &String::from_str(&s.env, "active"),
            &s.token,
        );

        let loan_id = s.client.get_loan(&borrower).unwrap().id;

        // Attempt to archive an active loan — must fail.
        let result = s.client.try_archive_loan(&admins(&s), &loan_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_get_archived_loan_none_for_unknown_id() {
        let s = setup();
        let archived = s.client.get_archived_loan(&9999);
        assert!(archived.is_none());
    }
}
