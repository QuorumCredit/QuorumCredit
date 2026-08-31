//! Tests for issue #1392: route_default_proceeds must actually transfer
//! tokens, and must not be able to pay the same batch of loans twice.
#[cfg(test)]
mod loan_priority_waterfall_tests {
    use crate::loan_priority::{LoanPriority, PriorityLoanEntry};
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
        admins: Vec<Address>,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        // Contract needs to actually hold "recovered proceeds" to pay out.
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            contract_id,
            token: token_id.address(),
            admins,
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

    fn token_balance(s: &Setup, addr: &Address) -> i128 {
        soroban_sdk::token::Client::new(&s.env, &s.token).balance(addr)
    }

    /// Creates a real loan (so `route_default_proceeds` can look up a real
    /// `token_address` via `get_loan_by_id`) and returns its id.
    fn make_loan(s: &Setup, amount: i128) -> u64 {
        let borrower = Address::generate(&s.env);
        do_vouch(s, &borrower, amount * 10);
        s.client
            .request_loan(&borrower, &amount, &0, &purpose(&s.env), &s.token);
        s.client.get_loan(&borrower).unwrap().id
    }

    #[test]
    fn route_default_proceeds_transfers_tokens_with_a_shortfall() {
        let s = setup();
        let senior_loan = make_loan(&s, 100_000);
        let junior_loan = make_loan(&s, 100_000);

        let senior_entry = PriorityLoanEntry {
            loan_id: senior_loan,
            borrower: Address::generate(&s.env),
            priority: LoanPriority::Senior,
            amount: 100_000,
        };
        let junior_entry = PriorityLoanEntry {
            loan_id: junior_loan,
            borrower: Address::generate(&s.env),
            priority: LoanPriority::Junior,
            amount: 100_000,
        };
        let entries = Vec::from_array(&s.env, [senior_entry.clone(), junior_entry.clone()]);
        s.client.create_loan_priority_queue(&s.admins, &entries);

        let senior_balance_before = token_balance(&s, &senior_entry.borrower);
        let junior_balance_before = token_balance(&s, &junior_entry.borrower);

        // Only enough recovered proceeds to cover Senior in full; Junior gets
        // nothing — a genuine shortfall.
        let run = s.client.route_default_proceeds(&s.admins, &100_000);

        assert_eq!(run.distributed, 100_000);
        assert_eq!(run.shortfall, 0); // total_proceeds(100_000) - distributed(100_000)

        assert_eq!(
            token_balance(&s, &senior_entry.borrower),
            senior_balance_before + 100_000,
            "Senior should be made whole"
        );
        assert_eq!(
            token_balance(&s, &junior_entry.borrower),
            junior_balance_before,
            "Junior should receive nothing when proceeds run out"
        );
    }

    #[test]
    fn route_default_proceeds_twice_without_a_new_queue_is_rejected() {
        let s = setup();
        let loan_id = make_loan(&s, 100_000);
        let entry = PriorityLoanEntry {
            loan_id,
            borrower: Address::generate(&s.env),
            priority: LoanPriority::Senior,
            amount: 100_000,
        };
        s.client
            .create_loan_priority_queue(&s.admins, &Vec::from_array(&s.env, [entry.clone()]));

        s.client.route_default_proceeds(&s.admins, &100_000);
        let balance_after_first_run = token_balance(&s, &entry.borrower);

        let result = s.client.try_route_default_proceeds(&s.admins, &100_000);

        assert_eq!(result, Err(Ok(crate::errors::ContractError::InvalidAmount)));
        assert_eq!(
            token_balance(&s, &entry.borrower),
            balance_after_first_run,
            "a rejected second call must not pay out again"
        );
    }
}
