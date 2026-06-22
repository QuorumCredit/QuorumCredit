#[cfg(test)]
mod governance_queue_tests {
    use crate::{
        ContractError, GovernanceAction, GovernanceProposal, GovernanceProposalStatus,
        GovernanceQueueConfig, QuorumCreditContract, QuorumCreditContractClient,
    };
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup_governance() -> (
        Env,
        QuorumCreditContractClient<'static>,
        Address,
        Address,
        Address,
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin1 = Address::generate(&env);
        let admin2 = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin1.clone(), admin2.clone()]);
        let token = env
            .register_stellar_asset_contract_v2(admin1.clone())
            .address();

        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &2, &token);

        (env, client, admin1, admin2, deployer)
    }

    #[test]
    fn test_propose_governance_action() {
        let (_env, client, admin1, _admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&_env, "Pause contract for maintenance"),
            )
            .unwrap();

        assert_eq!(proposal_id, 1);

        // Verify proposal was created
        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.id, proposal_id);
        assert_eq!(proposal.proposer, admin1);
        assert_eq!(proposal.status, GovernanceProposalStatus::Pending);
        assert_eq!(proposal.approvals.len(), 0);
        assert_eq!(proposal.rejections.len(), 0);
    }

    #[test]
    fn test_propose_unauthorized() {
        let (env, client, admin1, _admin2, _deployer) = setup_governance();
        let non_admin = Address::generate(&env);

        // Non-admin should not be able to propose
        let result = client.try_propose_governance_action(
            &non_admin,
            &GovernanceAction::Pause,
            &soroban_sdk::String::from_str(&env, "Pause contract"),
        );

        assert_eq!(result, Err(Ok(ContractError::UnauthorizedCaller)));
    }

    #[test]
    fn test_approve_governance_action() {
        let (env, client, admin1, admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&env, "Pause contract"),
            )
            .unwrap();

        // Approve with admin1
        client.approve_governance_action(&admin1, proposal_id).unwrap();

        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.approvals.len(), 1);
        assert_eq!(proposal.status, GovernanceProposalStatus::Pending); // Still pending, needs 2 approvals

        // Approve with admin2 (meets threshold)
        client.approve_governance_action(&admin2, proposal_id).unwrap();

        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.approvals.len(), 2);
        assert_eq!(proposal.status, GovernanceProposalStatus::Approved);
    }

    #[test]
    fn test_approve_unauthorized() {
        let (env, client, admin1, _admin2, _deployer) = setup_governance();
        let non_admin = Address::generate(&env);

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&env, "Pause contract"),
            )
            .unwrap();

        // Non-admin should not be able to approve
        let result = client.try_approve_governance_action(&non_admin, proposal_id);

        assert_eq!(result, Err(Ok(ContractError::UnauthorizedCaller)));
    }

    #[test]
    fn test_double_approval() {
        let (_env, client, admin1, _admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&_env, "Pause contract"),
            )
            .unwrap();

        // Approve with admin1
        client.approve_governance_action(&admin1, proposal_id).unwrap();

        // Try to approve again with admin1
        let result = client.try_approve_governance_action(&admin1, proposal_id);

        assert_eq!(result, Err(Ok(ContractError::AlreadyVoted)));
    }

    #[test]
    fn test_reject_governance_action() {
        let (_env, client, admin1, admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&_env, "Pause contract"),
            )
            .unwrap();

        // Reject with admin1
        client.reject_governance_action(&admin1, proposal_id).unwrap();

        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.rejections.len(), 1);
        assert_eq!(proposal.status, GovernanceProposalStatus::Pending);

        // Reject with admin2 (meets threshold, should cancel)
        client.reject_governance_action(&admin2, proposal_id).unwrap();

        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.rejections.len(), 2);
        assert_eq!(proposal.status, GovernanceProposalStatus::Cancelled);
    }

    #[test]
    fn test_execute_governance_action() {
        let (env, client, admin1, admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&env, "Pause contract"),
            )
            .unwrap();

        // Approve with both admins
        client.approve_governance_action(&admin1, proposal_id).unwrap();
        client.approve_governance_action(&admin2, proposal_id).unwrap();

        // Advance time past timelock delay (24 hours)
        env.ledger().set_timestamp(env.ledger().timestamp() + 24 * 60 * 60 + 1);

        // Execute the proposal
        client.execute_governance_action(proposal_id).unwrap();

        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.status, GovernanceProposalStatus::Executed);
        assert!(proposal.executed_at.is_some());

        // Verify contract is paused
        assert!(client.is_paused());
    }

    #[test]
    fn test_execute_before_timelock() {
        let (env, client, admin1, admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&env, "Pause contract"),
            )
            .unwrap();

        // Approve with both admins
        client.approve_governance_action(&admin1, proposal_id).unwrap();
        client.approve_governance_action(&admin2, proposal_id).unwrap();

        // Try to execute before timelock delay
        let result = client.try_execute_governance_action(proposal_id);

        assert_eq!(result, Err(Ok(ContractError::TimelockDelayNotElapsed)));
    }

    #[test]
    fn test_execute_after_expiry() {
        let (env, client, admin1, admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&env, "Pause contract"),
            )
            .unwrap();

        // Approve with both admins
        client.approve_governance_action(&admin1, proposal_id).unwrap();
        client.approve_governance_action(&admin2, proposal_id).unwrap();

        // Advance time past execution window (24 hours + 7 days)
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + 24 * 60 * 60 + 7 * 24 * 60 * 60 + 1);

        // Try to execute after expiry
        let result = client.try_execute_governance_action(proposal_id);

        assert_eq!(result, Err(Ok(ContractError::ExecutionWindowPassed)));

        // Verify proposal is marked as expired
        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.status, GovernanceProposalStatus::Expired);
    }

    #[test]
    fn test_cancel_governance_action() {
        let (_env, client, admin1, _admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&_env, "Pause contract"),
            )
            .unwrap();

        // Cancel by proposer
        client.cancel_governance_action(&admin1, proposal_id).unwrap();

        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.status, GovernanceProposalStatus::Cancelled);
    }

    #[test]
    fn test_cancel_by_admin() {
        let (_env, client, admin1, admin2, _deployer) = setup_governance();

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&_env, "Pause contract"),
            )
            .unwrap();

        // Cancel by another admin
        client.cancel_governance_action(&admin2, proposal_id).unwrap();

        let proposal: GovernanceProposal = client
            .get_governance_proposal(proposal_id)
            .unwrap()
            .try_into()
            .unwrap();

        assert_eq!(proposal.status, GovernanceProposalStatus::Cancelled);
    }

    #[test]
    fn test_cancel_unauthorized() {
        let (env, client, admin1, _admin2, _deployer) = setup_governance();
        let non_admin = Address::generate(&env);

        // Propose a pause action
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&env, "Pause contract"),
            )
            .unwrap();

        // Non-admin should not be able to cancel
        let result = client.try_cancel_governance_action(&non_admin, proposal_id);

        assert_eq!(result, Err(Ok(ContractError::UnauthorizedCaller)));
    }

    #[test]
    fn test_set_governance_queue_config() {
        let (_env, client, admin1, admin2, _deployer) = setup_governance();

        let new_config = GovernanceQueueConfig {
            timelock_delay: 12 * 60 * 60, // 12 hours
            execution_window: 3 * 24 * 60 * 60, // 3 days
            require_multisig: false,
        };

        client.set_governance_queue_config(
            &Vec::from_array(&_env, [admin1.clone(), admin2.clone()]),
            &new_config,
        );

        let retrieved_config: GovernanceQueueConfig = client
            .get_governance_queue_config_view()
            .try_into()
            .unwrap();

        assert_eq!(retrieved_config.timelock_delay, 12 * 60 * 60);
        assert_eq!(retrieved_config.execution_window, 3 * 24 * 60 * 60);
        assert_eq!(retrieved_config.require_multisig, false);
    }

    #[test]
    fn test_governance_action_set_protocol_fee() {
        let (env, client, admin1, admin2, _deployer) = setup_governance();

        // Propose to set protocol fee
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::SetProtocolFee(500), // 5%
                &soroban_sdk::String::from_str(&env, "Set protocol fee to 5%"),
            )
            .unwrap();

        // Approve with both admins
        client.approve_governance_action(&admin1, proposal_id).unwrap();
        client.approve_governance_action(&admin2, proposal_id).unwrap();

        // Advance time past timelock
        env.ledger().set_timestamp(env.ledger().timestamp() + 24 * 60 * 60 + 1);

        // Execute
        client.execute_governance_action(proposal_id).unwrap();

        // Verify fee was set
        assert_eq!(client.get_protocol_fee(), 500);
    }

    #[test]
    fn test_governance_action_add_admin() {
        let (env, client, admin1, admin2, _deployer) = setup_governance();
        let new_admin = Address::generate(&env);

        // Propose to add admin
        let proposal_id = client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::AddAdmin(new_admin.clone()),
                &soroban_sdk::String::from_str(&env, "Add new admin"),
            )
            .unwrap();

        // Approve with both admins
        client.approve_governance_action(&admin1, proposal_id).unwrap();
        client.approve_governance_action(&admin2, proposal_id).unwrap();

        // Advance time past timelock
        env.ledger().set_timestamp(env.ledger().timestamp() + 24 * 60 * 60 + 1);

        // Execute
        client.execute_governance_action(proposal_id).unwrap();

        // Verify admin was added
        let admins = client.get_admins();
        assert_eq!(admins.len(), 3);
        assert!(admins.iter().any(|a| a == new_admin));
    }

    #[test]
    fn test_governance_proposal_count() {
        let (env, client, admin1, _admin2, _deployer) = setup_governance();

        // Initially 0 proposals
        assert_eq!(client.get_governance_proposal_count(), 0);

        // Create a proposal
        client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::Pause,
                &soroban_sdk::String::from_str(&env, "Pause contract"),
            )
            .unwrap();

        // Count should be 1
        assert_eq!(client.get_governance_proposal_count(), 1);

        // Create another proposal
        client
            .propose_governance_action(
                &admin1,
                &GovernanceAction::SetProtocolFee(500),
                &soroban_sdk::String::from_str(&env, "Set fee"),
            )
            .unwrap();

        // Count should be 2
        assert_eq!(client.get_governance_proposal_count(), 2);
    }
}
