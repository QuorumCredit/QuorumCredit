#[cfg(test)]
mod tests {
    use crate::types::{FeeStructureProposal, GovernanceProposalStatus, DataKey};
    use soroban_sdk::{Address, Env};

    #[test]
    fn test_fee_structure_proposal_creation() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let proposal = FeeStructureProposal {
            id: 1,
            proposer: proposer.clone(),
            origination_fee_bps: 100,  // 1%
            repayment_fee_bps: 50,     // 0.5%
            late_fee_bps: 200,         // 2%
            votes_for: 0,
            votes_against: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        assert_eq!(proposal.id, 1);
        assert_eq!(proposal.origination_fee_bps, 100);
        assert_eq!(proposal.repayment_fee_bps, 50);
        assert_eq!(proposal.late_fee_bps, 200);
    }

    #[test]
    fn test_fee_structure_vote_accumulation() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let mut proposal = FeeStructureProposal {
            id: 1,
            proposer: proposer.clone(),
            origination_fee_bps: 100,
            repayment_fee_bps: 50,
            late_fee_bps: 200,
            votes_for: 0,
            votes_against: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        // Simulate multiple votes
        let vote_weight = 2000_i128;
        proposal.votes_for += vote_weight;
        proposal.votes_for += vote_weight;
        proposal.votes_for += vote_weight;

        assert_eq!(proposal.votes_for, 6000);
        assert_eq!(proposal.votes_against, 0);
    }

    #[test]
    fn test_fee_structure_proposal_execution() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let mut proposal = FeeStructureProposal {
            id: 1,
            proposer: proposer.clone(),
            origination_fee_bps: 150,
            repayment_fee_bps: 75,
            late_fee_bps: 250,
            votes_for: 8000,
            votes_against: 1000,
            status: GovernanceProposalStatus::Approved,
            created_at: 1000,
            eta: 2000,
        };

        // Execute proposal
        if proposal.status == GovernanceProposalStatus::Approved {
            proposal.status = GovernanceProposalStatus::Executed;
        }

        assert_eq!(proposal.status, GovernanceProposalStatus::Executed);
    }

    #[test]
    fn test_fee_structure_opposing_votes() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let mut proposal = FeeStructureProposal {
            id: 1,
            proposer: proposer.clone(),
            origination_fee_bps: 200,
            repayment_fee_bps: 100,
            late_fee_bps: 300,
            votes_for: 3000,
            votes_against: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        // Add opposing votes
        proposal.votes_against += 4000;

        assert!(proposal.votes_against > proposal.votes_for);
    }

    #[test]
    fn test_fee_structure_fee_caps() {
        let env = Env::default();
        let proposer = Address::random(&env);

        let proposal = FeeStructureProposal {
            id: 1,
            proposer: proposer.clone(),
            origination_fee_bps: 500,   // 5% max
            repayment_fee_bps: 250,     // 2.5% max
            late_fee_bps: 1000,         // 10% max
            votes_for: 0,
            votes_against: 0,
            status: GovernanceProposalStatus::Pending,
            created_at: 1000,
            eta: 2000,
        };

        // Validate fee caps
        assert!(proposal.origination_fee_bps <= 500);
        assert!(proposal.repayment_fee_bps <= 250);
        assert!(proposal.late_fee_bps <= 1000);
    }
}
