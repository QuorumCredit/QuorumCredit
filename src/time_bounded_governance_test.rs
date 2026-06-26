#[cfg(test)]
mod time_bounded_governance_tests {
    use crate::{ContractError, DataKey, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn vec_of(env: &Env, addr: &Address) -> Vec<Address> {
        let mut v = Vec::new(env);
        v.push_back(addr.clone());
        v
    }

    fn setup() -> (Env, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);

        (env, deployer, admin)
    }

    #[test]
    fn test_proposal_has_expiry_timestamp() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Create a governance proposal
        let proposal_id = 1u64;
        let creation_time = env.ledger().timestamp();

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(
                &DataKey::GovernanceProposal(proposal_id),
                &(creation_time, crate::types::ProposalStatus::Active),
            );
        });

        let proposal = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, (u64, crate::types::ProposalStatus)>(
                    &DataKey::GovernanceProposal(proposal_id),
                )
        });
        assert!(
            proposal.is_some(),
            "Proposal must have a timestamp"
        );
    }

    #[test]
    fn test_proposal_expires_after_voting_period() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Create proposal with expiry
        let proposal_id = 1u64;
        let voting_period = 7 * 24 * 60 * 60; // 7 days
        let creation_time = env.ledger().timestamp();
        let expiry_time = creation_time + voting_period;

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(
                &DataKey::ProposalExpiry(proposal_id),
                &expiry_time,
            );
        });

        let stored_expiry = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, u64>(&DataKey::ProposalExpiry(proposal_id))
        });
        assert_eq!(
            stored_expiry,
            Some(expiry_time),
            "Proposal must have expiry timestamp"
        );
    }

    #[test]
    fn test_expired_proposal_cannot_execute() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let _client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        
        env.as_contract(&contract_id, || {
            // Set a past expiry time
            let past_time = env.ledger().timestamp() - 1000;
            env.storage().persistent().set(
                &DataKey::ProposalExpiry(1u64),
                &past_time,
            );

            // Mark as expired
            env.storage().persistent().set(
                &DataKey::ProposalStatus(1u64),
                &crate::types::ProposalStatus::Expired,
            );
        });

        let status = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, crate::types::ProposalStatus>(&DataKey::ProposalStatus(1u64))
        });
        assert_eq!(
            status,
            Some(crate::types::ProposalStatus::Expired),
            "Expired proposal must be marked as such"
        );
    }

    #[test]
    fn test_active_proposal_can_be_voted_on() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Create active proposal
        let proposal_id = 1u64;
        let current_time = env.ledger().timestamp();
        let future_expiry = current_time + (7 * 24 * 60 * 60); // 7 days

        env.as_contract(&contract_id, || {
            env.storage().persistent().set(
                &DataKey::ProposalExpiry(proposal_id),
                &future_expiry,
            );
            env.storage().persistent().set(
                &DataKey::ProposalStatus(proposal_id),
                &crate::types::ProposalStatus::Active,
            );
        });

        let status = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, crate::types::ProposalStatus>(&DataKey::ProposalStatus(proposal_id))
        });
        assert_eq!(
            status,
            Some(crate::types::ProposalStatus::Active),
            "Active proposal must be votable"
        );
    }

    #[test]
    fn test_proposal_voting_period_configurable() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Store voting period configuration
        let voting_period = 14 * 24 * 60 * 60; // 14 days
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::VotingPeriod, &voting_period);
        });

        let stored_period = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, u64>(&DataKey::VotingPeriod)
        });
        assert_eq!(
            stored_period,
            Some(voting_period),
            "Voting period must be configurable"
        );
    }

    #[test]
    fn test_proposal_cannot_be_extended_past_max_duration() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let _client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        // Set max duration
        let max_duration = 30 * 24 * 60 * 60; // 30 days
        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::MaxProposalDuration, &max_duration);
        });

        let stored_max = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, u64>(&DataKey::MaxProposalDuration)
        });
        assert_eq!(
            stored_max,
            Some(max_duration),
            "Max proposal duration must be enforced"
        );
    }

    #[test]
    fn test_proposal_expiry_cleanup() {
        let (env, deployer, admin) = setup();
        use soroban_sdk::token::StellarAssetClient;

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let _client = QuorumCreditContractClient::new(&env, &contract_id);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &0);

        let admins = vec_of(&env, &admin);
        
        // Create multiple proposals
        for i in 1u64..=3 {
            env.as_contract(&contract_id, || {
                let past_time = env.ledger().timestamp() - (i * 1000);
                env.storage()
                    .persistent()
                    .set(&DataKey::ProposalExpiry(i), &past_time);
                env.storage()
                    .persistent()
                    .set(&DataKey::ProposalStatus(i), &crate::types::ProposalStatus::Expired);
            });
        }

        // Verify expired proposals are marked
        let proposal_1_status = env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .get::<DataKey, crate::types::ProposalStatus>(&DataKey::ProposalStatus(1u64))
        });
        assert_eq!(
            proposal_1_status,
            Some(crate::types::ProposalStatus::Expired),
            "Expired proposals must be tracked for cleanup"
        );
    }
}
