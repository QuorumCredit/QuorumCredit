#[cfg(test)]
mod cooldown_bypass_tests {
    use crate::{
        errors::ContractError, types::DEFAULT_VOUCH_COOLDOWN_SECS, QuorumCreditContract,
        QuorumCreditContractClient,
    };
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin1: Address,
        admin2: Address,
        admin3: Address,
        token_id: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let admin3 = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone(), admin3.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin1.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &10_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &3, &token_id.address());

        env.ledger().with_mut(|l| l.timestamp = 100_000);

        Setup {
            env,
            client,
            admin1,
            admin2,
            admin3,
            token_id: token_id.address(),
        }
    }

    fn setup_vouch(s: &Setup) -> (Address, Address) {
        let voucher = Address::generate(&s.env);
        let borrower = Address::generate(&s.env);
        let stake = 1_000_000i128;

        StellarAssetClient::new(&s.env, &s.token_id).mint(&voucher, &stake);
        s.client
            .vouch(&voucher, &borrower, &stake, &s.token_id);

        // Advance past default cooldown so next vouch would be blocked
        s.env
            .ledger()
            .with_mut(|l| l.timestamp = s.env.ledger().timestamp() + 1);

        // Now any vouch attempt within DEFAULT_VOUCH_COOLDOWN_SECS should fail
        (voucher, borrower)
    }

    #[test]
    fn test_request_cooldown_bypass_success() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        // Request bypass
        let reason = String::from_str(&s.env, "Loan default imminent");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        // Verify bypass request exists
        let bypass = s.client.get_cooldown_bypass(&borrower, &voucher);
        assert!(bypass.is_some());
        let bypass = bypass.unwrap();
        assert_eq!(bypass.voucher, voucher);
        assert_eq!(bypass.borrower, borrower);
        assert_eq!(bypass.reason, reason);
        assert!(!bypass.approved);
    }

    #[test]
    fn test_request_cooldown_bypass_not_voucher_fails() {
        let s = setup();
        let (_, borrower) = setup_vouch(&s);
        let non_voucher = Address::generate(&s.env);

        let reason = String::from_str(&s.env, "Emergency");
        let result = s
            .client
            .try_request_cooldown_bypass(&non_voucher, &borrower, &reason);
        assert_eq!(result, Err(Ok(ContractError::VoucherNotFound)));
    }

    #[test]
    fn test_request_cooldown_bypass_duplicate_fails() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        let reason = String::from_str(&s.env, "Emergency");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        let result = s
            .client
            .try_request_cooldown_bypass(&voucher, &borrower, &reason);
        assert_eq!(
            result,
            Err(Ok(ContractError::CooldownBypassAlreadyRequested))
        );
    }

    #[test]
    fn test_vote_bypass_non_admin_fails() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);
        let non_admin = Address::generate(&s.env);

        let reason = String::from_str(&s.env, "Emergency");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        let result = s
            .client
            .try_vote_bypass(&non_admin, &voucher, &borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::UnauthorizedCaller)));
    }

    #[test]
    fn test_vote_bypass_no_request_fails() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        let result = s
            .client
            .try_vote_bypass(&s.admin1, &voucher, &borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::CooldownBypassNotFound)));
    }

    #[test]
    fn test_vote_bypass_approval_threshold_2_of_3() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        let reason = String::from_str(&s.env, "Loan default imminent");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        // First admin approves — should not yet reach 2/3
        s.client
            .vote_bypass(&s.admin1, &voucher, &borrower, &true);
        let bypass = s.client.get_cooldown_bypass(&borrower, &voucher).unwrap();
        assert!(!bypass.approved);
        assert_eq!(bypass.approvers.len(), 1);

        // Second admin approves — now 2/3 threshold met
        s.client
            .vote_bypass(&s.admin2, &voucher, &borrower, &true);
        let bypass = s.client.get_cooldown_bypass(&borrower, &voucher).unwrap();
        assert!(bypass.approved);
        assert_eq!(bypass.approvers.len(), 2);
    }

    #[test]
    fn test_vote_bypass_double_vote_fails() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        let reason = String::from_str(&s.env, "Emergency");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        s.client
            .vote_bypass(&s.admin1, &voucher, &borrower, &true);
        let result = s
            .client
            .try_vote_bypass(&s.admin1, &voucher, &borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::AlreadyVoted)));
    }

    #[test]
    fn test_vote_bypass_after_approved_fails() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        let reason = String::from_str(&s.env, "Emergency");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        // Two admins approve => 2/3 met
        s.client
            .vote_bypass(&s.admin1, &voucher, &borrower, &true);
        s.client
            .vote_bypass(&s.admin2, &voucher, &borrower, &true);

        // Third admin tries to approve — should be rejected since already approved
        let result = s
            .client
            .try_vote_bypass(&s.admin3, &voucher, &borrower, &true);
        assert_eq!(result, Err(Ok(ContractError::CooldownBypassAlreadyApproved)));
    }

    #[test]
    fn test_vote_bypass_rejection_recorded() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        let reason = String::from_str(&s.env, "Emergency");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        // Admin rejects
        s.client
            .vote_bypass(&s.admin1, &voucher, &borrower, &false);
        let bypass = s.client.get_cooldown_bypass(&borrower, &voucher).unwrap();
        assert!(!bypass.approved);
        assert_eq!(bypass.approvers.len(), 1);
    }

    #[test]
    fn test_cooldown_bypass_allows_vouch_during_cooldown() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        // Try to vouch immediately (within cooldown) — should be blocked
        let stake2 = 500_000i128;
        StellarAssetClient::new(&s.env, &s.token_id).mint(&voucher, &stake2);
        let result = s
            .client
            .try_vouch(&voucher, &borrower, &stake2, &s.token_id);
        assert_eq!(result, Err(Ok(ContractError::VouchCooldownActive)));

        // Request and approve bypass
        let reason = String::from_str(&s.env, "Loan default imminent");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);
        s.client
            .vote_bypass(&s.admin1, &voucher, &borrower, &true);
        s.client
            .vote_bypass(&s.admin2, &voucher, &borrower, &true);

        // Now vouch should succeed despite cooldown
        let result = s
            .client
            .try_vouch(&voucher, &borrower, &stake2, &s.token_id);
        assert!(result.is_ok());
    }

    #[test]
    fn test_clear_cooldown_bypass() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        let reason = String::from_str(&s.env, "Emergency");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        // Clear with admin approval
        let admin_signers = Vec::from_array(&s.env, [s.admin1.clone()]);
        s.client
            .clear_cooldown_bypass(&admin_signers, &borrower, &voucher);

        let bypass = s.client.get_cooldown_bypass(&borrower, &voucher);
        assert!(bypass.is_none());
    }

    #[test]
    fn test_has_cooldown_bypass() {
        let s = setup();
        let (voucher, borrower) = setup_vouch(&s);

        // Before request, has_cooldown_bypass returns false
        assert!(!s.client.has_cooldown_bypass(&voucher, &borrower));

        let reason = String::from_str(&s.env, "Emergency");
        s.client
            .request_cooldown_bypass(&voucher, &borrower, &reason);

        // After request but before approval, returns false
        assert!(!s.client.has_cooldown_bypass(&voucher, &borrower));

        // Approve
        s.client
            .vote_bypass(&s.admin1, &voucher, &borrower, &true);
        s.client
            .vote_bypass(&s.admin2, &voucher, &borrower, &true);

        // After approval, returns true
        assert!(s.client.has_cooldown_bypass(&voucher, &borrower));
    }
}
