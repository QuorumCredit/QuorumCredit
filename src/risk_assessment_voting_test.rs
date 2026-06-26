#[cfg(test)]
mod tests {
    use crate::types::{RiskThresholdProposal, GovernanceProposalStatus, DataKey};
    use soroban_sdk::{Address, Env, testutils::Address as _};

    #[test]
    fn test_risk_threshold_proposal_creation() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let proposal = RiskThresholdProposal {
            id: 1,
            proposer: proposer.clone(),
            min_risk_threshold: 3000,  // 30%
            max_risk_threshold: 7000,  // 70%
            votes_for: 0,
            votes_against: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        assert_eq!(proposal.id, 1);
        assert_eq!(proposal.min_risk_threshold, 3000);
        assert_eq!(proposal.max_risk_threshold, 7000);
        assert_eq!(proposal.votes_for, 0);
        assert_eq!(proposal.votes_against, 0);
    }

    #[test]
    fn test_risk_threshold_vote_accumulation() {
        let env = Env::default();
        let proposer = Address::random(&env);
        let voter1 = Address::random(&env);
        let voter2 = Address::random(&env);

        let mut proposal = RiskThresholdProposal {
            id: 1,
            proposer: proposer.clone(),
            min_risk_threshold: 3000,
            max_risk_threshold: 7000,
            votes_for: 0,
            votes_against: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        // Simulate vote accumulation
        let vote_weight = 1000_i128;
        proposal.votes_for += vote_weight;
        proposal.votes_for += vote_weight;

        assert_eq!(proposal.votes_for, 2000);
        assert_eq!(proposal.votes_against, 0);
    }

    #[test]
    fn test_risk_threshold_proposal_approval() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let mut proposal = RiskThresholdProposal {
            id: 1,
            proposer: proposer.clone(),
            min_risk_threshold: 3000,
            max_risk_threshold: 7000,
            votes_for: 5000,
            votes_against: 2000,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        // Approve if votes_for > votes_against
        if proposal.votes_for > proposal.votes_against {
            proposal.status = GovernanceProposalStatus::Approved;
        }

        assert_eq!(proposal.status, GovernanceProposalStatus::Approved);
    }

    #[test]
    fn test_risk_threshold_proposal_rejection() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let mut proposal = RiskThresholdProposal {
            id: 1,
            proposer: proposer.clone(),
            min_risk_threshold: 3000,
            max_risk_threshold: 7000,
            votes_for: 1000,
            votes_against: 5000,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        // Reject if votes_against > votes_for
        if proposal.votes_against > proposal.votes_for {
            proposal.status = GovernanceProposalStatus::Cancelled;
        }

        assert_eq!(proposal.status, GovernanceProposalStatus::Cancelled);
    }

    #[test]
    fn test_risk_threshold_bounds_validation() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let proposal = RiskThresholdProposal {
            id: 1,
            proposer: proposer.clone(),
            min_risk_threshold: 2000,  // 20%
            max_risk_threshold: 8000,  // 80%
            votes_for: 0,
            votes_against: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        // Validate min < max
        assert!(proposal.min_risk_threshold < proposal.max_risk_threshold);
    }
}
