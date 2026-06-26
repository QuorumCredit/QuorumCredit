#[cfg(test)]
mod proposal_metadata_tests {
    use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    fn setup() -> (Env, QuorumCreditContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token);

        (env, client, admin)
    }

    #[test]
    fn test_proposal_stores_description() {
        let (_env, client, admin) = setup();
        
        let description = String::from_str(&_env, "Increase max loan amount");
        let proposal_id = client.propose_with_metadata(&admin, &description);
        
        assert!(proposal_id > 0);
    }

    #[test]
    fn test_proposal_creator_recorded() {
        let (_env, client, admin) = setup();
        
        let description = String::from_str(&_env, "Admin proposal");
        let proposal_id = client.propose_with_metadata(&admin, &description);
        let proposal = client.get_proposal(&proposal_id);
        
        assert_eq!(proposal.creator, admin);
    }

    #[test]
    fn test_proposal_timestamp_recorded() {
        let (env, client, admin) = setup();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        
        let description = String::from_str(&env, "Timestamped proposal");
        let proposal_id = client.propose_with_metadata(&admin, &description);
        let proposal = client.get_proposal(&proposal_id);
        
        assert_eq!(proposal.created_at, 100_000);
    }

    #[test]
    fn test_proposal_description_preserved() {
        let (_env, client, admin) = setup();
        
        let desc = String::from_str(&_env, "Rich proposal with details");
        let proposal_id = client.propose_with_metadata(&admin, &desc);
        let proposal = client.get_proposal(&proposal_id);
        
        assert_eq!(proposal.description, desc);
    }

    #[test]
    fn test_multiple_proposals_tracked() {
        let (_env, client, admin) = setup();
        
        let desc1 = String::from_str(&_env, "First");
        let desc2 = String::from_str(&_env, "Second");
        
        let id1 = client.propose_with_metadata(&admin, &desc1);
        let id2 = client.propose_with_metadata(&admin, &desc2);
        
        assert_ne!(id1, id2);
    }
}
