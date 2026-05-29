#[cfg(test)]
mod tests {
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env, String, Vec};

    fn setup(env: &Env) -> (Address, Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        QuorumCreditContractClient::new(env, &contract_id).initialize(
            &deployer,
            &Vec::from_array(env, [admin.clone()]),
            &1,
            &token,
        );
        (contract_id, admin, token)
    }

    #[test]
    fn test_propose_governance_change() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);

        // Mint governance tokens to proposer so they can propose
        soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&proposer, &1_000_000);

        client.set_governance_token(&Vec::from_array(&env, [admin.clone()]), &token);

        let description = String::from_str(&env, "Increase yield to 3%");
        let voting_period = 7 * 24 * 60 * 60u64;

        let proposal_id = client.propose_governance_change(&proposer, &description, &voting_period);

        let proposal = client.get_governance_proposal(&proposal_id).unwrap();
        assert_eq!(proposal.id, proposal_id);
        assert_eq!(proposal.proposer, proposer);
        assert!(!proposal.executed);
    }

    #[test]
    fn test_vote_on_governance_change() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&proposer, &1_000_000);
        soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&voter, &1_000_000);

        client.set_governance_token(&Vec::from_array(&env, [admin.clone()]), &token);

        let description = String::from_str(&env, "Increase yield to 3%");
        let voting_period = 7 * 24 * 60 * 60u64;

        let proposal_id = client.propose_governance_change(&proposer, &description, &voting_period);

        client.vote_on_governance_change(&voter, &proposal_id, &true);

        let proposal = client.get_governance_proposal(&proposal_id).unwrap();
        assert!(proposal.approve_votes > 0);
        assert!(proposal.voters.iter().any(|v| v == voter));
    }

    #[test]
    fn test_execute_governance_change() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, admin, token) = setup(&env);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&proposer, &1_000_000);
        soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&voter, &1_000_000);

        client.set_governance_token(&Vec::from_array(&env, [admin.clone()]), &token);

        let description = String::from_str(&env, "Increase yield to 3%");
        let voting_period = 1u64;

        let proposal_id = client.propose_governance_change(&proposer, &description, &voting_period);

        client.vote_on_governance_change(&voter, &proposal_id, &true);

        env.ledger().with_mut(|l| l.timestamp += 2);

        client.execute_governance_change(&proposal_id);

        let proposal = client.get_governance_proposal(&proposal_id).unwrap();
        assert!(proposal.executed);
    }
}
