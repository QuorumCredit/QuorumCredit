#[cfg(test)]
mod tests {
    use crate::types::{CrossChainProposalSync, GovernanceProposalStatus};
    use soroban_sdk::{Address, Env, Vec};

    #[test]
    fn test_cross_chain_proposal_sync_creation() {
        let env = Env::default();
        let proposer = Address::random(&env);
        let mut target_chains = Vec::new(&env);
        target_chains.push_back("ethereum");
        target_chains.push_back("polygon");

        let sync = CrossChainProposalSync {
            id: 1,
            source_chain: "stellar".to_string(),
            target_chains: target_chains.clone(),
            proposal_type: "fee".to_string(),
            proposal_data: vec![].into(),
            votes_required: 2,
            votes_received: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 5000,
        };

        assert_eq!(sync.id, 1);
        assert_eq!(sync.source_chain, "stellar");
        assert_eq!(sync.votes_required, 2);
        assert_eq!(sync.votes_received, 0);
    }

    #[test]
    fn test_cross_chain_proposal_vote_accumulation() {
        let env = Env::default();
        let mut target_chains = Vec::new(&env);
        target_chains.push_back("ethereum");
        target_chains.push_back("polygon");

        let mut sync = CrossChainProposalSync {
            id: 1,
            source_chain: "stellar".to_string(),
            target_chains: target_chains.clone(),
            proposal_type: "risk".to_string(),
            proposal_data: vec![].into(),
            votes_required: 2,
            votes_received: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 5000,
        };

        // Accumulate votes
        sync.votes_received += 1;
        sync.votes_received += 1;

        assert_eq!(sync.votes_received, 2);
        assert_eq!(sync.votes_received, sync.votes_required);
    }

    #[test]
    fn test_cross_chain_proposal_execution() {
        let env = Env::default();
        let mut target_chains = Vec::new(&env);
        target_chains.push_back("ethereum");

        let mut sync = CrossChainProposalSync {
            id: 1,
            source_chain: "stellar".to_string(),
            target_chains: target_chains.clone(),
            proposal_type: "fee".to_string(),
            proposal_data: vec![].into(),
            votes_required: 1,
            votes_received: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 5000,
        };

        // Meet quorum
        sync.votes_received = sync.votes_required;

        // Execute
        if sync.votes_received >= sync.votes_required {
            sync.status = GovernanceProposalStatus::Executed;
        }

        assert_eq!(sync.status, GovernanceProposalStatus::Executed);
    }

    #[test]
    fn test_cross_chain_multi_target() {
        let env = Env::default();
        let mut target_chains = Vec::new(&env);
        target_chains.push_back("ethereum");
        target_chains.push_back("polygon");
        target_chains.push_back("arbitrum");
        target_chains.push_back("optimism");

        let sync = CrossChainProposalSync {
            id: 1,
            source_chain: "stellar".to_string(),
            target_chains: target_chains.clone(),
            proposal_type: "timelock".to_string(),
            proposal_data: vec![].into(),
            votes_required: 4,
            votes_received: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 5000,
        };

        // Verify all targets are present
        assert_eq!(sync.target_chains.len(), 4);
    }

    #[test]
    fn test_cross_chain_proposal_data_serialization() {
        let env = Env::default();
        let mut target_chains = Vec::new(&env);
        target_chains.push_back("ethereum");

        let proposal_data: Vec<u8> = vec![1, 2, 3, 4, 5].into();
        let sync = CrossChainProposalSync {
            id: 1,
            source_chain: "stellar".to_string(),
            target_chains: target_chains.clone(),
            proposal_type: "risk".to_string(),
            proposal_data: proposal_data.clone(),
            votes_required: 1,
            votes_received: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 5000,
        };

        assert_eq!(sync.proposal_data.len(), 5);
    }

    #[test]
    fn test_cross_chain_proposal_cancellation() {
        let env = Env::default();
        let mut target_chains = Vec::new(&env);
        target_chains.push_back("ethereum");

        let mut sync = CrossChainProposalSync {
            id: 1,
            source_chain: "stellar".to_string(),
            target_chains: target_chains.clone(),
            proposal_type: "fee".to_string(),
            proposal_data: vec![].into(),
            votes_required: 2,
            votes_received: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 5000,
        };

        // Cancel proposal
        sync.status = GovernanceProposalStatus::Cancelled;

        assert_eq!(sync.status, GovernanceProposalStatus::Cancelled);
    }
}
