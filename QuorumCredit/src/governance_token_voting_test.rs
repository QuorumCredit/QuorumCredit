#[cfg(test)]
mod tests {
    use crate::*;
    use soroban_sdk::{testutils::Address as _, Address, Env, String};

    #[test]
    fn test_propose_governance_change() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);
        let proposer = Address::random(&env);

        env.mock_all_auths();

        QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        QuorumCreditContract::set_governance_token(
            env.clone(),
            vec![&env, admin.clone()],
            token.clone(),
        )
        .unwrap();

        let description = String::from_slice(&env, "Increase yield to 3%");
        let voting_period = 7 * 24 * 60 * 60; // 7 days

        let proposal_id = QuorumCreditContract::propose_governance_change(
            env.clone(),
            proposer.clone(),
            description,
            voting_period,
        )
        .unwrap();

        let proposal = QuorumCreditContract::get_governance_proposal(env.clone(), proposal_id).unwrap();
        assert_eq!(proposal.id, proposal_id);
        assert_eq!(proposal.proposer, proposer);
        assert!(!proposal.executed);
    }

    #[test]
    fn test_vote_on_governance_change() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);
        let proposer = Address::random(&env);
        let voter = Address::random(&env);

        env.mock_all_auths();

        QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        QuorumCreditContract::set_governance_token(
            env.clone(),
            vec![&env, admin.clone()],
            token.clone(),
        )
        .unwrap();

        let description = String::from_slice(&env, "Increase yield to 3%");
        let voting_period = 7 * 24 * 60 * 60;

        let proposal_id = QuorumCreditContract::propose_governance_change(
            env.clone(),
            proposer.clone(),
            description,
            voting_period,
        )
        .unwrap();

        QuorumCreditContract::vote_on_governance_change(
            env.clone(),
            voter.clone(),
            proposal_id,
            true,
        )
        .unwrap();

        let proposal = QuorumCreditContract::get_governance_proposal(env.clone(), proposal_id).unwrap();
        assert!(proposal.approve_votes > 0);
        assert!(proposal.voters.iter().any(|v| v == voter));
    }

    #[test]
    fn test_execute_governance_change() {
        let env = Env::default();
        let admin = Address::random(&env);
        let deployer = Address::random(&env);
        let token = Address::random(&env);
        let proposer = Address::random(&env);
        let voter = Address::random(&env);

        env.mock_all_auths();

        QuorumCreditContract::initialize(
            env.clone(),
            deployer.clone(),
            vec![&env, admin.clone()],
            1,
            token.clone(),
        )
        .unwrap();

        QuorumCreditContract::set_governance_token(
            env.clone(),
            vec![&env, admin.clone()],
            token.clone(),
        )
        .unwrap();

        let description = String::from_slice(&env, "Increase yield to 3%");
        let voting_period = 1; // 1 second

        let proposal_id = QuorumCreditContract::propose_governance_change(
            env.clone(),
            proposer.clone(),
            description,
            voting_period,
        )
        .unwrap();

        QuorumCreditContract::vote_on_governance_change(
            env.clone(),
            voter.clone(),
            proposal_id,
            true,
        )
        .unwrap();

        // Advance ledger time past voting period
        env.ledger().with_mut(|l| {
            l.timestamp = l.timestamp + 2;
        });

        QuorumCreditContract::execute_governance_change(env.clone(), proposal_id).unwrap();

        let proposal = QuorumCreditContract::get_governance_proposal(env.clone(), proposal_id).unwrap();
        assert!(proposal.executed);
    }
}
