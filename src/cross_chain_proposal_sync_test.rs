// Unit tests for the CrossChainProposalSync data type (Issue #906 / #970).
// These tests exercise the struct fields directly — the full workflow is
// covered in cross_chain_governance_test.rs.

#[cfg(test)]
mod tests {
    use crate::types::{CrossChainProposalSync, GovernanceProposalStatus};
    use soroban_sdk::{Bytes, Env, String as SorobanString, Vec};

    fn make_sync(env: &Env, chains: &[&str], status: GovernanceProposalStatus) -> CrossChainProposalSync {
        let mut target_chains: Vec<SorobanString> = Vec::new(env);
        for c in chains {
            target_chains.push_back(SorobanString::from_str(env, c));
        }
        CrossChainProposalSync {
            id: 1,
            source_chain: SorobanString::from_str(env, "stellar"),
            target_chains: target_chains.clone(),
            proposal_type: SorobanString::from_str(env, "fee"),
            proposal_data: Bytes::new(env),
            votes_required: target_chains.len(),
            votes_received: 0,
            voted_chains: Vec::new(env),
            status,
            created_at: 1000,
            eta: 5000,
            expires_at: 5000 + 7 * 24 * 60 * 60,
        }
    }

    #[test]
    fn test_creation_fields() {
        let env = Env::default();
        let sync = make_sync(&env, &["ethereum", "polygon"], GovernanceProposalStatus::Pending);
        assert_eq!(sync.id, 1);
        assert_eq!(sync.votes_required, 2);
        assert_eq!(sync.votes_received, 0);
        assert_eq!(sync.status, GovernanceProposalStatus::Pending);
    }

    #[test]
    fn test_vote_accumulation() {
        let env = Env::default();
        let mut sync = make_sync(&env, &["ethereum", "polygon"], GovernanceProposalStatus::Pending);
        sync.votes_received += 1;
        sync.votes_received += 1;
        assert_eq!(sync.votes_received, 2);
        assert_eq!(sync.votes_received, sync.votes_required);
    }

    #[test]
    fn test_quorum_transitions_to_approved() {
        let env = Env::default();
        let mut sync = make_sync(&env, &["ethereum"], GovernanceProposalStatus::Pending);
        sync.votes_received = sync.votes_required;
        if sync.votes_received >= sync.votes_required {
            sync.status = GovernanceProposalStatus::Approved;
        }
        assert_eq!(sync.status, GovernanceProposalStatus::Approved);
    }

    #[test]
    fn test_multi_target_chain_count() {
        let env = Env::default();
        let sync = make_sync(&env, &["ethereum", "polygon", "arbitrum", "optimism"], GovernanceProposalStatus::Pending);
        assert_eq!(sync.target_chains.len(), 4);
        assert_eq!(sync.votes_required, 4);
    }

    #[test]
    fn test_cancellation() {
        let env = Env::default();
        let mut sync = make_sync(&env, &["ethereum"], GovernanceProposalStatus::Pending);
        sync.status = GovernanceProposalStatus::Cancelled;
        assert_eq!(sync.status, GovernanceProposalStatus::Cancelled);
    }

    #[test]
    fn test_voted_chains_tracks_votes() {
        let env = Env::default();
        let mut sync = make_sync(&env, &["ethereum", "polygon"], GovernanceProposalStatus::Pending);
        sync.voted_chains.push_back(SorobanString::from_str(&env, "ethereum"));
        assert_eq!(sync.voted_chains.len(), 1);
    }
}
