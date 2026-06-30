#[cfg(test)]
mod slash_threshold_voting_tests {
    use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        token_holder: Address,
        token_id: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token_holder = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(&env, &token_id.address()).mint(&token_holder, &1_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        env.ledger().with_mut(|l| l.timestamp = 90_000);

        Setup {
            env,
            client,
            admin,
            token_holder,
            token_id: token_id.address(),
        }
    }

    #[test]
    fn test_proposal_created_correctly() {
        let s = setup();
        let id = s.client.propose_slash_threshold(&s.admin, &3_000);
        let p = s.client.get_slash_threshold_proposal(&id).unwrap();
        assert_eq!(p.proposed_threshold, 3_000);
        assert_eq!(p.proposer, s.admin);
        assert!(!p.finalized);
    }

    #[test]
    fn test_votes_tallied_and_approved_updates_threshold() {
        let s = setup();
        let voter = s.token_holder.clone();
        let id = s.client.propose_slash_threshold(&s.admin, &4_000);
        s.client.vote_slash_threshold(&voter, &id, &true);
        s.client.vote_slash_threshold(&s.admin, &id, &true);

        let period = s.client.get_config().voting_period_seconds;
        s.env.ledger().with_mut(|l| l.timestamp += period + 1);

        s.client.finalize_slash_threshold(&id);
        assert_eq!(s.client.get_config().slash_bps, 4_000);
    }

    #[test]
    fn test_rejected_proposal_discards_without_change() {
        let s = setup();
        let original = s.client.get_config().slash_bps;
        let id = s.client.propose_slash_threshold(&s.admin, &2_000);
        s.client.vote_slash_threshold(&s.admin, &id, &false);
        s.client.vote_slash_threshold(&s.token_holder, &id, &false);

        let period = s.client.get_config().voting_period_seconds;
        s.env.ledger().with_mut(|l| l.timestamp += period + 1);

        s.client.finalize_slash_threshold(&id);
        assert_eq!(s.client.get_config().slash_bps, original);
    }

    #[test]
    fn test_vote_after_period_rejected() {
        let s = setup();
        let id = s.client.propose_slash_threshold(&s.admin, &3_500);
        let period = s.client.get_config().voting_period_seconds;
        s.env.ledger().with_mut(|l| l.timestamp += period + 1);

        let result = s.client.try_vote_slash_threshold(&s.admin, &id, &true);
        assert_eq!(result, Err(Ok(ContractError::VotingPeriodEnded)));
    }

    #[test]
    fn test_finalize_before_period_rejected() {
        let s = setup();
        let id = s.client.propose_slash_threshold(&s.admin, &3_500);
        let result = s.client.try_finalize_slash_threshold(&id);
        assert_eq!(result, Err(Ok(ContractError::TimelockNotReady)));
    }

    /// Mutation target: finalize uses strict majority (`approve_votes > reject_votes`).
    #[test]
    fn test_tie_vote_keeps_original_slash_threshold() {
        let s = setup();
        let original = s.client.get_config().slash_bps;
        let id = s.client.propose_slash_threshold(&s.admin, &3_000);
        s.client.vote_slash_threshold(&s.admin, &id, &true);
        s.client.vote_slash_threshold(&s.token_holder, &id, &false);

        let period = s.client.get_config().voting_period_seconds;
        s.env.ledger().with_mut(|l| l.timestamp += period + 1);

        s.client.finalize_slash_threshold(&id);
        assert_eq!(s.client.get_config().slash_bps, original);
    }

    /// Mutation target: invalid threshold guard (`new_threshold <= 0`).
    #[test]
    fn test_propose_zero_threshold_rejected() {
        let s = setup();
        let result = s.client.try_propose_slash_threshold(&s.admin, &0);
        assert_eq!(result, Err(Ok(ContractError::InvalidBps)));
    }

    /// Mutation target: invalid threshold guard (`new_threshold > 10_000`).
    #[test]
    fn test_propose_threshold_above_max_rejected() {
        let s = setup();
        let result = s.client.try_propose_slash_threshold(&s.admin, &10_001);
        assert_eq!(result, Err(Ok(ContractError::InvalidBps)));
    }

    /// Mutation target: duplicate voter guard in vote_slash_threshold.
    #[test]
    fn test_double_vote_on_slash_threshold_rejected() {
        let s = setup();
        let id = s.client.propose_slash_threshold(&s.admin, &3_500);
        s.client.vote_slash_threshold(&s.admin, &id, &true);
        let result = s.client.try_vote_slash_threshold(&s.admin, &id, &false);
        assert_eq!(result, Err(Ok(ContractError::AlreadyVoted)));
    }

    /// Mutation target: finalize expiry window (`now > voting_end + voting_period_seconds`).
    #[test]
    fn test_finalize_after_expiry_window_rejected() {
        let s = setup();
        let mut cfg = s.client.get_config();
        cfg.voting_period_seconds = 60;
        s.client.set_config(&Vec::from_array(&s.env, [s.admin.clone()]), &cfg);

        let id = s.client.propose_slash_threshold(&s.admin, &3_500);
        let period = s.client.get_config().voting_period_seconds;
        // Finalize window closes after voting_end + voting_period.
        s.env
            .ledger()
            .with_mut(|l| l.timestamp += period * 2 + 2);

        let result = s.client.try_finalize_slash_threshold(&id);
        assert_eq!(result, Err(Ok(ContractError::TimelockExpired)));
    }
}
