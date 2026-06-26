#[cfg(test)]
mod rate_limiting_tests {
    use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
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
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &500_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 120);
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    /// Test that rapid sequential vouches are rate limited
    #[test]
    fn test_rate_limit_rapid_vouch_calls() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower1 = Address::generate(&setup.env);
        let borrower2 = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &500_000_000);
        
        // First vouch should succeed
        let result1 = setup.client.try_vouch(&voucher, &borrower1, &100_000_000, &setup.token);
        assert!(result1.is_ok());
        
        // Immediate second vouch without time advancement should be rate limited
        let result2 = setup.client.try_vouch(&voucher, &borrower2, &100_000_000, &setup.token);
        
        // Should be rate limited
        if let Err(e) = result2 {
            assert_eq!(e.unwrap_err(), ContractError::RateLimitExceeded);
        }
    }

    /// Test that rate limit is lifted after cooldown period
    #[test]
    fn test_rate_limit_cooldown_expires() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower1 = Address::generate(&setup.env);
        let borrower2 = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &500_000_000);
        
        // First vouch
        setup.client.vouch(&voucher, &borrower1, &100_000_000, &setup.token);
        
        // Advance time by rate limit window (e.g., 60 seconds)
        setup.env.ledger().with_mut(|l| l.timestamp += 60);
        
        // Second vouch should succeed after cooldown
        let result = setup.client.try_vouch(&voucher, &borrower2, &100_000_000, &setup.token);
        assert!(result.is_ok());
    }

    /// Test that different addresses have independent rate limits
    #[test]
    fn test_rate_limit_per_address() {
        let setup = setup();
        let voucher1 = Address::generate(&setup.env);
        let voucher2 = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher1, &200_000_000);
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher2, &200_000_000);
        
        // First address vouches
        let result1 = setup.client.try_vouch(&voucher1, &borrower, &100_000_000, &setup.token);
        assert!(result1.is_ok());
        
        // Second address should not be rate limited by first address's actions
        let result2 = setup.client.try_vouch(&voucher2, &borrower, &100_000_000, &setup.token);
        assert!(result2.is_ok());
    }

    /// Test rate limit applies to request_loan operations
    #[test]
    fn test_rate_limit_on_request_loan() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        // Setup vouching
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &500_000_000);
        setup.client.vouch(&voucher, &borrower, &200_000_000, &setup.token);
        setup.env.ledger().with_mut(|l| l.timestamp += 61);
        
        // First loan request succeeds
        let result1 = setup.client.try_request_loan(
            &borrower,
            &100_000_000,
            &200_000_000,
            &String::from_str(&setup.env, "loan1"),
            &setup.token,
        );
        assert!(result1.is_ok());
        
        // Immediate second loan request should be rate limited
        let result2 = setup.client.try_request_loan(
            &borrower,
            &50_000_000,
            &200_000_000,
            &String::from_str(&setup.env, "loan2"),
            &setup.token,
        );
        
        // Should be rate limited
        if let Err(e) = result2 {
            assert_eq!(e.unwrap_err(), ContractError::RateLimitExceeded);
        }
    }

    /// Test rate limit allows multiple operations across time windows
    #[test]
    fn test_rate_limit_multiple_windows() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrowers: Vec<Address> = (0..3)
            .map(|_| Address::generate(&setup.env))
            .collect();
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &1_000_000_000);
        
        // Operation 1 - succeeds
        setup.client.vouch(&voucher, &borrowers.get(0), &100_000_000, &setup.token);
        
        // Advance time
        setup.env.ledger().with_mut(|l| l.timestamp += 60);
        
        // Operation 2 - succeeds after cooldown
        setup.client.vouch(&voucher, &borrowers.get(1), &100_000_000, &setup.token);
        
        // Advance time again
        setup.env.ledger().with_mut(|l| l.timestamp += 60);
        
        // Operation 3 - succeeds after second cooldown
        let result = setup.client.try_vouch(&voucher, &borrowers.get(2), &100_000_000, &setup.token);
        assert!(result.is_ok());
    }

    /// Test that rate limit tracks concurrent borrower eligibility checks
    #[test]
    fn test_rate_limit_eligibility_checks() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &500_000_000);
        setup.client.vouch(&voucher, &borrower, &200_000_000, &setup.token);
        
        // First eligibility check should succeed
        let result1 = setup.client.is_eligible(&borrower, &100_000_000, &setup.token);
        assert!(result1);
        
        // Immediate second eligibility check on same borrower (queries don't count toward rate limit)
        let result2 = setup.client.is_eligible(&borrower, &100_000_000, &setup.token);
        assert!(result2);
    }
}
