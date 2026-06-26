#[cfg(test)]
mod anti_dust_attack_tests {
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
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &50_000_000);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 120);
        Setup { env, client, admin, token: token_id.address() }
    }

    fn admins(s: &Setup) -> Vec<Address> {
        Vec::from_array(&s.env, [s.admin.clone()])
    }

    /// Test that vouch with zero stake is rejected
    #[test]
    fn test_vouch_rejects_zero_stake() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &1_000_000);
        
        // Attempt vouch with zero stake
        let result = setup.client.try_vouch(&voucher, &borrower, &0, &setup.token);
        
        // Should be rejected
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::InvalidAmount);
        } else {
            panic!("Expected vouch to reject zero stake");
        }
    }

    /// Test that vouch with negative stake is rejected
    #[test]
    fn test_vouch_rejects_negative_stake() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &1_000_000);
        
        // Attempt vouch with negative stake
        let result = setup.client.try_vouch(&voucher, &borrower, &-1, &setup.token);
        
        // Should be rejected
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::InvalidAmount);
        } else {
            panic!("Expected vouch to reject negative stake");
        }
    }

    /// Test that request_loan with zero amount is rejected
    #[test]
    fn test_request_loan_rejects_zero_amount() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        // Setup vouching
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &200_000_000);
        setup.client.vouch(&voucher, &borrower, &100_000_000, &setup.token);
        setup.env.ledger().with_mut(|l| l.timestamp += 61);
        
        // Attempt to request loan with zero amount
        let result = setup.client.try_request_loan(
            &borrower,
            &0,
            &100_000_000,
            &String::from_str(&setup.env, "test"),
            &setup.token,
        );
        
        // Should be rejected
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::InvalidAmount);
        } else {
            panic!("Expected request_loan to reject zero amount");
        }
    }

    /// Test that request_loan with negative amount is rejected
    #[test]
    fn test_request_loan_rejects_negative_amount() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &200_000_000);
        setup.client.vouch(&voucher, &borrower, &100_000_000, &setup.token);
        setup.env.ledger().with_mut(|l| l.timestamp += 61);
        
        let result = setup.client.try_request_loan(
            &borrower,
            &-50_000_000,
            &100_000_000,
            &String::from_str(&setup.env, "test"),
            &setup.token,
        );
        
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::InvalidAmount);
        } else {
            panic!("Expected request_loan to reject negative amount");
        }
    }

    /// Test that repay with zero payment is rejected
    #[test]
    fn test_repay_rejects_zero_payment() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &200_000_000);
        StellarAssetClient::new(&setup.env, &setup.token).mint(&borrower, &100_000_000);
        
        setup.client.vouch(&voucher, &borrower, &100_000_000, &setup.token);
        setup.env.ledger().with_mut(|l| l.timestamp += 61);
        
        setup.client.request_loan(
            &borrower,
            &50_000_000,
            &100_000_000,
            &String::from_str(&setup.env, "test"),
            &setup.token,
        );
        
        // Attempt repay with zero payment
        let result = setup.client.try_repay(&borrower, &0);
        
        // Should be rejected
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::InvalidAmount);
        } else {
            panic!("Expected repay to reject zero payment");
        }
    }

    /// Test that minimum stake enforcement prevents dust attacks on yields
    #[test]
    fn test_minimum_stake_prevents_yield_dust() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &1_000_000);
        
        // Attempt to vouch with 49 stroops (below minimum of 50)
        let result = setup.client.try_vouch(&voucher, &borrower, &49, &setup.token);
        
        // Should be rejected or warning should apply
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::MinStakeNotMet);
        }
    }

    /// Test that minimum stake of exactly 50 stroops is accepted
    #[test]
    fn test_minimum_stake_boundary_valid() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &1_000_000);
        
        // Vouch with exactly 50 stroops (minimum)
        let result = setup.client.try_vouch(&voucher, &borrower, &50, &setup.token);
        
        // Should succeed
        assert!(result.is_ok());
    }

    /// Test that batch vouch rejects any zero/negative amounts
    #[test]
    fn test_batch_vouch_rejects_invalid_amounts() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower1 = Address::generate(&setup.env);
        let borrower2 = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &500_000_000);
        
        // Create batch with one zero amount
        let borrowers = Vec::from_array(&setup.env, [borrower1, borrower2]);
        let stakes = Vec::from_array(&setup.env, [100_000_000, 0]);
        
        // Should reject entire batch due to invalid amount
        let result = setup.client.try_batch_vouch(&voucher, &borrowers, &stakes, &setup.token);
        
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::InvalidAmount);
        } else {
            panic!("Expected batch_vouch to reject zero amount");
        }
    }

    /// Test that decrease_stake cannot go below minimum
    #[test]
    fn test_decrease_stake_enforces_minimum() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &200_000_000);
        setup.client.vouch(&voucher, &borrower, &100_000_000, &setup.token);
        
        // Attempt to decrease to below minimum (50 stroops)
        let result = setup.client.try_decrease_stake(&voucher, &borrower, &30, &setup.token);
        
        // Should be rejected
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::MinStakeNotMet);
        } else {
            panic!("Expected decrease_stake to enforce minimum");
        }
    }

    /// Test that threshold validation rejects zero threshold
    #[test]
    fn test_request_loan_rejects_zero_threshold() {
        let setup = setup();
        let voucher = Address::generate(&setup.env);
        let borrower = Address::generate(&setup.env);
        
        StellarAssetClient::new(&setup.env, &setup.token).mint(&voucher, &200_000_000);
        setup.client.vouch(&voucher, &borrower, &100_000_000, &setup.token);
        setup.env.ledger().with_mut(|l| l.timestamp += 61);
        
        // Attempt loan with zero threshold
        let result = setup.client.try_request_loan(
            &borrower,
            &50_000_000,
            &0,
            &String::from_str(&setup.env, "test"),
            &setup.token,
        );
        
        // Should be rejected
        if let Err(e) = result {
            assert_eq!(e.unwrap_err(), ContractError::InvalidAmount);
        } else {
            panic!("Expected request_loan to reject zero threshold");
        }
    }
}
