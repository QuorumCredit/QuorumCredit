#[cfg(test)]
mod vote_delegation_tests {
    use super::*;
    use crate::types::DataKey;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, Address, Env, Vec};

    fn setup_contract(env: &Env) -> (Address, Address) {
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admins = Vec::from_array(env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        (contract_id, token_id.address())
    }

    fn create_vouch(
        env: &Env,
        contract_id: &Address,
        token_id: &Address,
        voucher: &Address,
        borrower: &Address,
        stake: i128,
    ) {
        let client = QuorumCreditContractClient::new(env, contract_id);
        StellarAssetClient::new(env, token_id).mint(&voucher, &stake);
        client.vouch(&voucher, &borrower, &stake, &token_id, &None);
    }

    #[test]
    fn test_delegate_vote() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id) = setup_contract(&env);

        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);
        let borrower = Address::generate(&env);

        create_vouch(&env, &contract_id, &token_id, &voucher1, &borrower, 1_000_000);
        create_vouch(&env, &contract_id, &token_id, &voucher2, &borrower, 500_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Delegate vote
        client.delegate_vote(&voucher1, &voucher2);

        // Check delegation was stored
        let delegate = client.get_vote_delegate(&voucher1);
        assert_eq!(delegate, Some(voucher2.clone()));

        // Voucher2 should not have a delegate
        let delegate2 = client.get_vote_delegate(&voucher2);
        assert_eq!(delegate2, None);
    }

    #[test]
    fn test_revoke_vote_delegation() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id) = setup_contract(&env);

        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);
        let borrower = Address::generate(&env);

        create_vouch(&env, &contract_id, &token_id, &voucher1, &borrower, 1_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Delegate vote
        client.delegate_vote(&voucher1, &voucher2);
        assert!(client.get_vote_delegate(&voucher1).is_some());

        // Revoke delegation
        client.revoke_vote_delegation(&voucher1);
        assert!(client.get_vote_delegate(&voucher1).is_none());
    }

    #[test]
    fn test_circular_delegation_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id) = setup_contract(&env);

        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);
        let voucher3 = Address::generate(&env);
        let borrower = Address::generate(&env);

        create_vouch(&env, &contract_id, &token_id, &voucher1, &borrower, 1_000_000);
        create_vouch(&env, &contract_id, &token_id, &voucher2, &borrower, 500_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Set up: voucher1 -> voucher2
        client.delegate_vote(&voucher1, &voucher2);

        // Try to create circular: voucher2 -> voucher1 (should fail)
        let result = client.try_delegate_vote(&voucher2, &voucher1);
        assert!(result.is_err());
    }

    #[test]
    fn test_self_delegation_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id) = setup_contract(&env);

        let voucher1 = Address::generate(&env);
        let borrower = Address::generate(&env);

        create_vouch(&env, &contract_id, &token_id, &voucher1, &borrower, 1_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Try to delegate to self (should fail)
        let result = client.try_delegate_vote(&voucher1, &voucher1);
        assert!(result.is_err());
    }

    #[test]
    fn test_delegated_vote_counts_correctly() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id) = setup_contract(&env);

        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);
        let borrower = Address::generate(&env);

        // voucher1 has 1000, voucher2 has 500
        create_vouch(&env, &contract_id, &token_id, &voucher1, &borrower, 1_000_000);
        create_vouch(&env, &contract_id, &token_id, &voucher2, &borrower, 500_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // voucher1 delegates to voucher2
        client.delegate_vote(&voucher1, &voucher2);

        // Now voucher2 should be able to vote with combined stake (1000 + 500 = 1500)
        // voucher1's vote should return Ok (delegate will vote)
        let result = client.vote_slash(&voucher1, &borrower, &true);
        assert!(result.is_ok());

        // voucher2 votes with their own stake + delegated stake
        let result = client.vote_slash(&voucher2, &borrower, &true);
        assert!(result.is_ok());

        // Check vote record
        let vote = client.get_slash_vote(&borrower);
        assert!(vote.is_some());
        let vote = vote.unwrap();
        
        // Should have 1500 total approve stake (voucher2's 500 + voucher1's 1000)
        assert_eq!(vote.approve_stake, 1_500_000);
    }

    #[test]
    fn test_revoke_delegation_not_found() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id) = setup_contract(&env);

        let voucher1 = Address::generate(&env);
        let borrower = Address::generate(&env);

        create_vouch(&env, &contract_id, &token_id, &voucher1, &borrower, 1_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Try to revoke non-existent delegation
        let result = client.try_revoke_vote_delegation(&voucher1);
        assert!(result.is_err());
    }

    #[test]
    fn test_delegation_chain_resolution() {
        let env = Env::default();
        env.mock_all_auths();
        let (contract_id, token_id) = setup_contract(&env);

        let voucher1 = Address::generate(&env);
        let voucher2 = Address::generate(&env);
        let voucher3 = Address::generate(&env);
        let borrower = Address::generate(&env);

        create_vouch(&env, &contract_id, &token_id, &voucher1, &borrower, 1_000_000);
        create_vouch(&env, &contract_id, &token_id, &voucher2, &borrower, 500_000);
        create_vouch(&env, &contract_id, &token_id, &voucher3, &borrower, 300_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);

        // Set up chain: voucher1 -> voucher2 -> voucher3
        client.delegate_vote(&voucher1, &voucher2);
        client.delegate_vote(&voucher2, &voucher3);

        // voucher3 should be the final delegate for both
        // When voucher3 votes, they should have: 300 (own) + 500 (voucher2) + 1000 (voucher1) = 1800
        let result = client.vote_slash(&voucher3, &borrower, &true);
        assert!(result.is_ok());

        let vote = client.get_slash_vote(&borrower);
        assert!(vote.is_some());
        let vote = vote.unwrap();
        assert_eq!(vote.approve_stake, 1_800_000);
    }
}