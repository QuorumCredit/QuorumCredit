// Comprehensive integration tests for #970 Cross-Chain Governance / Multi-chain Voting.

#[cfg(test)]
mod tests {
    use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
    use crate::types::{GovernanceProposalStatus, CROSS_CHAIN_PROPOSAL_ETA_SECS, CROSS_CHAIN_PROPOSAL_EXPIRY_SECS};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Bytes, Env, String as SorobanString, Vec,
    };

    // ── Helpers ────────────────────────────────────────────────────────────

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        relayer: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let relayer = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        // Start at a non-zero timestamp so ETA math is meaningful.
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        Setup { env, client, admin, relayer }
    }

    fn str(env: &Env, s: &str) -> SorobanString {
        SorobanString::from_str(env, s)
    }

    fn chains(env: &Env, names: &[&str]) -> Vec<SorobanString> {
        let mut v: Vec<SorobanString> = Vec::new(env);
        for n in names { v.push_back(str(env, n)); }
        v
    }

    // ── propose_cross_chain ────────────────────────────────────────────────

    #[test]
    fn test_propose_creates_pending_proposal() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum", "polygon"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.id, sync_id);
        assert_eq!(p.votes_required, 2);
        assert_eq!(p.votes_received, 0);
        assert_eq!(p.status, GovernanceProposalStatus::Pending);
        // ETA should be roughly now + ETA_SECS
        assert!(p.eta > 100_000);
        assert!(p.expires_at > p.eta);
    }

    #[test]
    fn test_propose_increments_count() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);
        assert_eq!(s.client.get_cross_chain_proposal_count(), 0);

        s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "risk"),
            &Bytes::new(&s.env),
        );
        assert_eq!(s.client.get_cross_chain_proposal_count(), 1);

        s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["polygon"]),
            &str(&s.env, "timelock"),
            &Bytes::new(&s.env),
        );
        assert_eq!(s.client.get_cross_chain_proposal_count(), 2);
    }

    #[test]
    fn test_propose_empty_chains_fails() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);
        let result = s.client.try_propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &Vec::new(&s.env),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );
        assert_eq!(result, Err(Ok(ContractError::InvalidAmount)));
    }

    // ── vote_cross_chain ───────────────────────────────────────────────────

    #[test]
    fn test_single_chain_vote_reaches_quorum() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);

        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.votes_received, 1);
        assert_eq!(p.status, GovernanceProposalStatus::Approved);
    }

    #[test]
    fn test_two_chain_quorum_requires_both_votes() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum", "polygon"]),
            &str(&s.env, "risk"),
            &Bytes::new(&s.env),
        );

        // First vote — still pending
        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);
        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.status, GovernanceProposalStatus::Pending);

        // Second vote — quorum reached
        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "polygon"), &true);
        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.status, GovernanceProposalStatus::Approved);
    }

    #[test]
    fn test_reject_vote_does_not_reach_quorum() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        // Reject vote — should not advance to Approved
        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &false);

        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.votes_received, 0);
        assert_eq!(p.status, GovernanceProposalStatus::Pending);
    }

    #[test]
    fn test_double_vote_same_chain_rejected() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum", "polygon"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);

        let result = s.client.try_vote_cross_chain(
            &s.relayer,
            &sync_id,
            &str(&s.env, "ethereum"),
            &true,
        );
        assert_eq!(result, Err(Ok(ContractError::AlreadyVoted)));
    }

    #[test]
    fn test_vote_from_non_target_chain_rejected() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        let result = s.client.try_vote_cross_chain(
            &s.relayer,
            &sync_id,
            &str(&s.env, "arbitrum"), // not in target_chains
            &true,
        );
        assert_eq!(result, Err(Ok(ContractError::InvalidBridgeChain)));
    }

    #[test]
    fn test_vote_on_nonexistent_proposal_fails() {
        let s = setup();
        let result = s.client.try_vote_cross_chain(
            &s.relayer,
            &999,
            &str(&s.env, "ethereum"),
            &true,
        );
        assert_eq!(result, Err(Ok(ContractError::ProposalNotFound)));
    }

    // ── execute_cross_chain ────────────────────────────────────────────────

    #[test]
    fn test_execute_after_timelock_succeeds() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        // Collect the single required vote → Approved
        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);

        // Advance past ETA
        s.env.ledger().with_mut(|l| l.timestamp += CROSS_CHAIN_PROPOSAL_ETA_SECS + 1);

        s.client.execute_cross_chain(&sync_id);

        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.status, GovernanceProposalStatus::Executed);
    }

    #[test]
    fn test_execute_before_timelock_fails() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);

        // Do NOT advance time — ETA not reached
        let result = s.client.try_execute_cross_chain(&sync_id);
        assert_eq!(result, Err(Ok(ContractError::TimelockNotReady)));
    }

    #[test]
    fn test_execute_not_approved_fails() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum", "polygon"]),
            &str(&s.env, "risk"),
            &Bytes::new(&s.env),
        );

        // Only one vote cast out of two required — still Pending
        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);

        s.env.ledger().with_mut(|l| l.timestamp += CROSS_CHAIN_PROPOSAL_ETA_SECS + 1);

        let result = s.client.try_execute_cross_chain(&sync_id);
        assert_eq!(result, Err(Ok(ContractError::QuorumNotMet)));
    }

    #[test]
    fn test_execute_after_expiry_fails() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);

        // Advance past ETA + full expiry window
        s.env.ledger().with_mut(|l| {
            l.timestamp += CROSS_CHAIN_PROPOSAL_ETA_SECS + CROSS_CHAIN_PROPOSAL_EXPIRY_SECS + 1;
        });

        let result = s.client.try_execute_cross_chain(&sync_id);
        assert_eq!(result, Err(Ok(ContractError::TimelockExpired)));

        // Proposal should now be Expired
        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.status, GovernanceProposalStatus::Expired);
    }

    #[test]
    fn test_double_execute_fails() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);
        s.env.ledger().with_mut(|l| l.timestamp += CROSS_CHAIN_PROPOSAL_ETA_SECS + 1);
        s.client.execute_cross_chain(&sync_id);

        let result = s.client.try_execute_cross_chain(&sync_id);
        assert_eq!(result, Err(Ok(ContractError::ProposalAlreadyApproved)));
    }

    // ── cancel_cross_chain ─────────────────────────────────────────────────

    #[test]
    fn test_cancel_pending_proposal() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        s.client.cancel_cross_chain(&admin_signers, &sync_id);

        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.status, GovernanceProposalStatus::Cancelled);
    }

    #[test]
    fn test_cancel_approved_proposal() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);
        // Approved but not yet executed — should be cancellable
        s.client.cancel_cross_chain(&admin_signers, &sync_id);

        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.status, GovernanceProposalStatus::Cancelled);
    }

    #[test]
    fn test_cancel_executed_proposal_fails() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum"]),
            &str(&s.env, "fee"),
            &Bytes::new(&s.env),
        );

        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);
        s.env.ledger().with_mut(|l| l.timestamp += CROSS_CHAIN_PROPOSAL_ETA_SECS + 1);
        s.client.execute_cross_chain(&sync_id);

        let result = s.client.try_cancel_cross_chain(&admin_signers, &sync_id);
        assert_eq!(result, Err(Ok(ContractError::InvalidStateTransition)));
    }

    #[test]
    fn test_cancel_nonexistent_proposal_fails() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);
        let result = s.client.try_cancel_cross_chain(&admin_signers, &999);
        assert_eq!(result, Err(Ok(ContractError::ProposalNotFound)));
    }

    // ── query ──────────────────────────────────────────────────────────────

    #[test]
    fn test_get_nonexistent_proposal_returns_none() {
        let s = setup();
        assert!(s.client.get_cross_chain_proposal(&42).is_none());
    }

    #[test]
    fn test_full_multi_chain_workflow() {
        let s = setup();
        let admin_signers = Vec::from_array(&s.env, [s.admin.clone()]);

        // Propose across 3 chains
        let sync_id = s.client.propose_cross_chain(
            &admin_signers,
            &str(&s.env, "stellar"),
            &chains(&s.env, &["ethereum", "polygon", "arbitrum"]),
            &str(&s.env, "timelock"),
            &Bytes::new(&s.env),
        );

        // Votes from 2 of 3 chains — still pending
        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "ethereum"), &true);
        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "polygon"), &true);
        assert_eq!(
            s.client.get_cross_chain_proposal(&sync_id).unwrap().status,
            GovernanceProposalStatus::Pending
        );

        // Third vote — quorum reached
        s.client.vote_cross_chain(&s.relayer, &sync_id, &str(&s.env, "arbitrum"), &true);
        assert_eq!(
            s.client.get_cross_chain_proposal(&sync_id).unwrap().status,
            GovernanceProposalStatus::Approved
        );

        // Execute after timelock
        s.env.ledger().with_mut(|l| l.timestamp += CROSS_CHAIN_PROPOSAL_ETA_SECS + 1);
        s.client.execute_cross_chain(&sync_id);

        let p = s.client.get_cross_chain_proposal(&sync_id).unwrap();
        assert_eq!(p.status, GovernanceProposalStatus::Executed);
        assert_eq!(p.votes_received, 3);
    }
}
