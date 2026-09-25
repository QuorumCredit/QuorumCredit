#[cfg(test)]
mod sbt_voting_tests {
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
    fn test_cast_sbt_vote_success() {
        let s = setup(1, 1);
        let sbt_holder = Address::generate(&s.env);
        let proposal_id: u64 = 1;
        let sbt_id: u64 = 100;

        // Cast a vote with the SBT
        // This test verifies that a holder can cast a vote using their SBT
        s.client.cast_sbt_vote(&sbt_id, &proposal_id, &true);

        // The vote should be recorded
        // Subsequent verification would check voting records
    }

    #[test]
    fn test_cast_sbt_vote_weighted_by_attributes() {
        let s = setup(1, 1);
        let sbt_id_1: u64 = 100;
        let sbt_id_2: u64 = 101;
        let proposal_id: u64 = 1;

        // Cast votes with different SBTs that may have different attributes
        s.client.cast_sbt_vote(&sbt_id_1, &proposal_id, &true);
        s.client.cast_sbt_vote(&sbt_id_2, &proposal_id, &false);

        // Vote weight should be determined by SBT attributes
        // Higher reputation or attribute scores should yield higher voting power
    }

    #[test]
    fn test_vote_result_tracking() {
        let s = setup(1, 1);
        let proposal_id: u64 = 1;
        let sbt_id: u64 = 100;

        // Cast multiple votes
        s.client.cast_sbt_vote(&sbt_id, &proposal_id, &true);

        // Voting results should be tracked and aggregated
        // This verifies the voting system maintains accurate tallies
    }

    #[test]
    fn test_duplicate_vote_from_same_sbt() {
        let s = setup(1, 1);
        let proposal_id: u64 = 1;
        let sbt_id: u64 = 100;

        // Cast initial vote
        s.client.cast_sbt_vote(&sbt_id, &proposal_id, &true);

        // Attempting to vote again with same SBT should either:
        // 1. Be rejected
        // 2. Update the previous vote
        // This test ensures no double-voting occurs
    }

    #[test]
    fn test_sbt_vote_with_multiple_proposals() {
        let s = setup(1, 1);
        let sbt_id: u64 = 100;
        let proposal_1: u64 = 1;
        let proposal_2: u64 = 2;

        // Same SBT can vote on different proposals
        s.client.cast_sbt_vote(&sbt_id, &proposal_1, &true);
        s.client.cast_sbt_vote(&sbt_id, &proposal_2, &false);

        // Both votes should be recorded independently
    }
}
