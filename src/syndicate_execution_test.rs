//! Tests for issue #1409: executing an Approved syndicate proposal, and the
//! symmetric for/against vote threshold.
#[cfg(test)]
mod syndicate_execution_tests {
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

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            token: token_id.address(),
        }
    }

    fn token_balance(s: &Setup, addr: &Address) -> i128 {
        soroban_sdk::token::Client::new(&s.env, &s.token).balance(addr)
    }

    /// Two equal members (50/50 share_bps), so a single member's for-vote
    /// lands the proposal exactly on the 5,000 bps threshold.
    fn setup_two_member_pool(s: &Setup, pool_id: u64) -> (Address, Address) {
        let member_a = Address::generate(&s.env);
        let member_b = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token).mint(&member_a, &500_000);
        StellarAssetClient::new(&s.env, &s.token).mint(&member_b, &500_000);

        let contributions = Vec::from_array(
            &s.env,
            [
                crate::types::SyndicateContribution { member: member_a.clone(), amount: 500_000 },
                crate::types::SyndicateContribution { member: member_b.clone(), amount: 500_000 },
            ],
        );
        s.client.create_vouch_syndicate(&member_a, &pool_id, &s.token, &contributions);
        (member_a, member_b)
    }

    #[test]
    fn exact_tie_at_5000_bps_resolves_to_approved() {
        let s = setup();
        let (member_a, _member_b) = setup_two_member_pool(&s, 1);

        let proposal_id = s.client.propose_syndicate_action(
            &1,
            &member_a,
            &String::from_str(&s.env, "dissolve"),
        );
        // member_a alone carries exactly 5,000 bps (50%) of the pool.
        s.client.vote_syndicate_proposal(&1, &proposal_id, &member_a, &true);

        let proposal = s.client.get_syndicate_proposal(&1, &proposal_id).unwrap();
        assert_eq!(proposal.votes_for_bps, 5_000);
        assert_eq!(proposal.status, crate::types::SyndicateProposalStatus::Approved);
    }

    #[test]
    fn execute_approved_proposal_dissolves_pool_and_returns_principal() {
        let s = setup();
        let (member_a, member_b) = setup_two_member_pool(&s, 2);

        assert_eq!(token_balance(&s, &member_a), 0);
        assert_eq!(token_balance(&s, &member_b), 0);

        let proposal_id = s.client.propose_syndicate_action(
            &2,
            &member_a,
            &String::from_str(&s.env, "dissolve"),
        );
        s.client.vote_syndicate_proposal(&2, &proposal_id, &member_a, &true);

        s.client.execute_syndicate_proposal(&2, &proposal_id);

        assert_eq!(token_balance(&s, &member_a), 500_000, "member_a's principal should be returned");
        assert_eq!(token_balance(&s, &member_b), 500_000, "member_b's principal should be returned");

        let pool = s.client.get_syndicate_pool(&2).unwrap();
        assert!(!pool.active, "pool should be dissolved");

        let proposal = s.client.get_syndicate_proposal(&2, &proposal_id).unwrap();
        assert_eq!(proposal.status, crate::types::SyndicateProposalStatus::Executed);
    }

    #[test]
    fn execute_proposal_twice_is_rejected() {
        let s = setup();
        let (member_a, _member_b) = setup_two_member_pool(&s, 3);

        let proposal_id = s.client.propose_syndicate_action(
            &3,
            &member_a,
            &String::from_str(&s.env, "dissolve"),
        );
        s.client.vote_syndicate_proposal(&3, &proposal_id, &member_a, &true);

        s.client.execute_syndicate_proposal(&3, &proposal_id);
        let result = s.client.try_execute_syndicate_proposal(&3, &proposal_id);

        assert_eq!(
            result,
            Err(Ok(crate::errors::ContractError::InvalidStateTransition))
        );
    }

    #[test]
    fn execute_pending_proposal_is_rejected() {
        let s = setup();
        let (member_a, _member_b) = setup_two_member_pool(&s, 4);

        let proposal_id = s.client.propose_syndicate_action(
            &4,
            &member_a,
            &String::from_str(&s.env, "dissolve"),
        );
        // No votes cast — proposal remains Pending.

        let result = s.client.try_execute_syndicate_proposal(&4, &proposal_id);

        assert_eq!(
            result,
            Err(Ok(crate::errors::ContractError::InvalidStateTransition))
        );
    }
}
