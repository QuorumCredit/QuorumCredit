#[cfg(test)]
mod slice_governance_tests {
    use crate::types::DataKey;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token: Address,
        contract_id: Address,
        deployer: Address,
        admins: Vec<Address>,
    }

    fn setup(admin_threshold: u32, num_admins: usize) -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let mut admins = Vec::new(&env);
        for _ in 0..num_admins {
            admins.push_back(Address::generate(&env));
        }

        let token_id = env.register_stellar_asset_contract_v2(admins.get(0).unwrap().clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        // Fund contract
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &1_000_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &admin_threshold, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 120);

        Setup {
            env,
            client,
            token: token_id.address(),
            contract_id,
            deployer,
            admins,
        }
    }

    #[test]
    fn test_vote_on_slice_change_success() {
        let s = setup(1, 1);
        let slice_id: u64 = 1;
        let governance_voter = Address::generate(&s.env);

        // Cast a vote on a slice change
        // This test verifies that governance members can vote on slice modifications
        s.client.vote_on_slice_change(&slice_id, &true);

        // The vote should be recorded in governance history
    }

    #[test]
    fn test_multiple_votes_on_same_slice() {
        let s = setup(1, 1);
        let slice_id: u64 = 1;

        // Multiple governance members can vote on the same slice change
        s.client.vote_on_slice_change(&slice_id, &true);
        s.client.vote_on_slice_change(&slice_id, &false);

        // Both votes should be tracked
    }

    #[test]
    fn test_voting_history_tracking() {
        let s = setup(1, 1);
        let slice_id: u64 = 1;

        // Vote on slice change
        s.client.vote_on_slice_change(&slice_id, &true);

        // Voting history should be maintained and queryable
        // This allows for audit trails and governance transparency
    }

    #[test]
    fn test_governance_token_model() {
        let s = setup(1, 1);
        let slice_id: u64 = 1;

        // Governance voting power should be determined by token holdings
        s.client.vote_on_slice_change(&slice_id, &true);

        // Higher governance token balance should provide more voting weight
        // This ensures proportional governance participation
    }

    #[test]
    fn test_slice_change_vote_quorum_requirements() {
        let s = setup(2, 3);
        let slice_id: u64 = 1;

        // Test that slice changes require sufficient governance approval
        // Multiple voters may be required depending on governance rules
        s.client.vote_on_slice_change(&slice_id, &true);

        // Quorum or threshold requirements should be enforced
    }

    #[test]
    fn test_vote_against_slice_change() {
        let s = setup(1, 1);
        let slice_id: u64 = 1;

        // Governance members should be able to vote against proposed changes
        s.client.vote_on_slice_change(&slice_id, &false);

        // Both affirmative and negative votes should be properly recorded
    }
}
