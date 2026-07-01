/// Issue #1065 — Threshold Voting for Loan Slashes
///
/// Tests covering:
/// - DataKey::SlashVote(borrower) tracking
/// - vote_slash(voucher, borrower, approve) — voucher can vote once
/// - execute_slash_vote(borrower) if 2/3 (≈6667 bps) approve
/// - 7-day cooldown between successive slash proposals
#[cfg(test)]
mod slash_voting_flow_tests {
    use crate::{
        types::DEFAULT_SLASH_PROPOSAL_COOLDOWN_SECS, ContractError, LoanStatus,
        QuorumCreditContract, QuorumCreditContractClient,
    };
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    // ── Helpers ───────────────────────────────────────────────────────────────

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        admin: Address,
        token_id: Address,
    }

    /// Bootstrap a contract with the quorum set to 6667 bps (≈ 2/3), zero slash delay so
    /// execute_slash_vote can be called immediately after quorum is reached, and the ledger
    /// timestamp set to a value large enough to comfortably test 7-day cooldowns.
    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let admins = Vec::from_array(&env, [admin.clone()]);

        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        // Pre-fund the contract so it can disburse loans and yield.
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &100_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());

        // Use a large base timestamp so subtracting 7 days never wraps.
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        Setup {
            env,
            client,
            admin,
            token_id: token_id.address(),
        }
    }

    /// Mint tokens to `voucher` and stake for `borrower`, then advance time past the
    /// vouch-age requirement so the loan can be requested immediately.
    fn do_vouch(s: &Setup, voucher: &Address, borrower: &Address, stake: i128) {
        StellarAssetClient::new(&s.env, &s.token_id).mint(voucher, &stake);
        s.client.vouch(voucher, borrower, &stake, &s.token_id);
        // Advance past DEFAULT_MIN_VOUCH_AGE_SECS (24 h) so the vouch is eligible.
        let t = s.env.ledger().timestamp();
        s.env.ledger().with_mut(|l| l.timestamp = t + 90_001);
    }

    fn do_loan(s: &Setup, borrower: &Address, amount: i128, threshold: i128) {
        s.client.request_loan(
            borrower,
            &amount,
            &threshold,
            &String::from_str(&s.env, "test loan"),
            &s.token_id,
        );
    }

    fn advance(s: &Setup, secs: u64) {
        let t = s.env.ledger().timestamp();
        s.env.ledger().with_mut(|l| l.timestamp = t + secs);
    }

    // ── Tests: DataKey::SlashVote tracking ───────────────────────────────────

    /// After the first vote_slash call, get_slash_vote returns a non-None record
    /// with the voter's stake tallied.
    #[test]
    fn test_slash_vote_record_created_on_first_vote() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 500_000);

        assert!(
            s.client.get_slash_vote(&borrower).is_none(),
            "vote record should not exist before any vote"
        );

        s.client.vote_slash(&voucher, &borrower, &true);

        let record = s.client.get_slash_vote(&borrower).expect("vote record must exist after voting");
        assert_eq!(record.approve_stake, 1_000_000);
        assert_eq!(record.reject_stake, 0);
        assert_eq!(record.voters.len(), 1);
        assert!(!record.executed);
    }

    /// A reject vote is correctly tracked separately from an approve vote.
    #[test]
    fn test_slash_vote_records_reject_vote() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 2_000_000);
        do_loan(&s, &borrower, 100_000, 1_000_000);

        s.client.vote_slash(&voucher, &borrower, &false);

        let record = s.client.get_slash_vote(&borrower).unwrap();
        assert_eq!(record.approve_stake, 0);
        assert_eq!(record.reject_stake, 2_000_000);
    }

    // ── Tests: vote_slash — voucher can vote only once ────────────────────────

    /// Casting a second vote by the same voucher returns AlreadyVoted.
    #[test]
    fn test_double_vote_returns_already_voted() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 500_000);

        s.client.vote_slash(&voucher, &borrower, &true);

        let result = s.client.try_vote_slash(&voucher, &borrower, &true);
        assert_eq!(
            result,
            Err(Ok(ContractError::AlreadyVoted)),
            "second vote by the same voucher must return AlreadyVoted"
        );
    }

    /// A non-voucher address (no stake for the borrower) cannot vote.
    #[test]
    fn test_non_voucher_cannot_vote() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);
        let outsider = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 500_000);

        let result = s.client.try_vote_slash(&outsider, &borrower, &true);
        assert_eq!(
            result,
            Err(Ok(ContractError::VoucherNotFound)),
            "address with no stake must not be able to vote"
        );
    }

    /// Multiple vouchers can each vote once, and their stakes are tallied correctly.
    #[test]
    fn test_multiple_vouchers_vote_stake_tallied() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);
        let v3 = Address::generate(&s.env);

        do_vouch(&s, &v1, &borrower, 3_000_000);
        do_vouch(&s, &v2, &borrower, 3_000_000);
        do_vouch(&s, &v3, &borrower, 3_000_000);
        do_loan(&s, &borrower, 100_000, 5_000_000);

        s.client.vote_slash(&v1, &borrower, &true);
        s.client.vote_slash(&v2, &borrower, &false);

        let record = s.client.get_slash_vote(&borrower).unwrap();
        assert_eq!(record.approve_stake, 3_000_000);
        assert_eq!(record.reject_stake, 3_000_000);
        assert_eq!(record.voters.len(), 2);
    }

    // ── Tests: execute_slash_vote — 2/3 quorum required ──────────────────────

    /// When exactly 2/3 of total stake approves, the slash executes and the
    /// loan transitions to Defaulted.
    #[test]
    fn test_execute_slash_vote_succeeds_at_two_thirds_quorum() {
        let s = setup();
        // Lower quorum to exactly 6667 bps (≈ 2/3) — this is the default, but set it explicitly.
        s.client.set_slash_vote_quorum(&Vec::from_array(&s.env, [s.admin.clone()]), &6_667);

        let borrower = Address::generate(&s.env);
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);
        let v3 = Address::generate(&s.env);

        // Three equal vouchers: total stake = 3_000_000
        do_vouch(&s, &v1, &borrower, 1_000_000);
        do_vouch(&s, &v2, &borrower, 1_000_000);
        do_vouch(&s, &v3, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 2_000_000);

        // v1 and v2 approve (2_000_000 / 3_000_000 ≈ 66.67% ≥ 6667 bps)
        s.client.vote_slash(&v1, &borrower, &true);
        s.client.vote_slash(&v2, &borrower, &true);

        // Quorum was reached automatically; slash is pending — execute it.
        let result = s.client.try_execute_slash_vote(&borrower);
        // Either auto-executed (SlashAlreadyExecuted) or still pending (Ok).
        // Either way, loan must be Defaulted or quorum must have triggered slash.
        // We accept both outcomes: quorum auto-executes or manual execute succeeds.
        match result {
            Ok(Ok(())) => {
                assert_eq!(s.client.loan_status(&borrower), LoanStatus::Defaulted);
            }
            Err(Ok(ContractError::SlashAlreadyExecuted)) => {
                // Auto-executed on the vote — check state
                assert_eq!(s.client.loan_status(&borrower), LoanStatus::Defaulted);
            }
            other => panic!("unexpected result: {:?}", other),
        }
    }

    /// When less than 2/3 of stake has approved, execute_slash_vote returns QuorumNotMet.
    #[test]
    fn test_execute_slash_vote_fails_below_two_thirds() {
        let s = setup();
        s.client.set_slash_vote_quorum(&Vec::from_array(&s.env, [s.admin.clone()]), &6_667);

        let borrower = Address::generate(&s.env);
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);
        let v3 = Address::generate(&s.env);

        // Three equal vouchers: total stake = 3_000_000; need ≥ 2_000_001 to reach 6667 bps.
        do_vouch(&s, &v1, &borrower, 1_000_000);
        do_vouch(&s, &v2, &borrower, 1_000_000);
        do_vouch(&s, &v3, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 2_000_000);

        // Only v1 approves (1_000_000 / 3_000_000 ≈ 33% — below 66.67% quorum)
        s.client.vote_slash(&v1, &borrower, &true);

        let result = s.client.try_execute_slash_vote(&borrower);
        assert_eq!(
            result,
            Err(Ok(ContractError::QuorumNotMet)),
            "slash must not execute below 2/3 quorum"
        );
        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Active);
    }

    /// With no slash vote record at all, execute_slash_vote returns SlashVoteNotFound.
    #[test]
    fn test_execute_slash_vote_no_record_returns_not_found() {
        let s = setup();
        let borrower = Address::generate(&s.env);

        let result = s.client.try_execute_slash_vote(&borrower);
        assert_eq!(result, Err(Ok(ContractError::SlashVoteNotFound)));
    }

    /// Calling execute_slash_vote a second time returns SlashAlreadyExecuted.
    #[test]
    fn test_execute_slash_vote_twice_returns_already_executed() {
        let s = setup();
        // Set quorum to 1 bps so a single small vote triggers execution.
        s.client.set_slash_vote_quorum(&Vec::from_array(&s.env, [s.admin.clone()]), &1);

        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 500_000);

        s.client.vote_slash(&voucher, &borrower, &true);

        // First call: either Ok or AlreadyExecuted (auto-executed in vote_slash).
        let _ = s.client.try_execute_slash_vote(&borrower);

        // Second call must always return SlashAlreadyExecuted.
        let result = s.client.try_execute_slash_vote(&borrower);
        assert_eq!(result, Err(Ok(ContractError::SlashAlreadyExecuted)));
    }

    // ── Tests: 7-day cooldown between slash proposals ─────────────────────────

    /// Initiating a second slash proposal within 7 days of the first returns SlashCooldownActive.
    #[test]
    fn test_second_proposal_within_cooldown_rejected() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);

        do_vouch(&s, &v1, &borrower, 1_000_000);
        do_vouch(&s, &v2, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 1_000_000);

        // First proposal initiated by v1.
        s.client.vote_slash(&v1, &borrower, &false);

        // Advance only 3 days — still inside the 7-day cooldown.
        advance(&s, 3 * 24 * 60 * 60);

        // A second voucher tries to start a *new* proposal for the same borrower.
        // Because the current proposal already has v2 as an eligible but un-voted voter,
        // v2 can still vote on the *existing* proposal.
        // To test the cooldown we need a fresh loan — so repay and take a new loan.
        // But repay is blocked while the slash proposal is active... instead just test
        // that a new borrower triggers the cooldown check after the first proposal fires.

        // ---- Simpler scenario: single voucher, proposal already initiated ----
        let borrower2 = Address::generate(&s.env);
        let voucher2 = Address::generate(&s.env);

        do_vouch(&s, &voucher2, &borrower2, 1_000_000);
        do_loan(&s, &borrower2, 100_000, 500_000);

        // First proposal for borrower2.
        s.client.vote_slash(&voucher2, &borrower2, &false);

        // Now advance only 1 day — within cooldown.
        advance(&s, 1 * 24 * 60 * 60);

        // Attempt a second proposal for borrower2 from a fresh voucher (simulated by
        // minting and vouching a new address, then requesting a new loan).
        // Since the first proposal is still active (not executed), the second *first-voter*
        // attempt is blocked by the proposal cooldown.
        // We verify this by checking the LastSlashProposalAt key indirectly:
        // trying to initiate a new proposal for borrower2 with a different voucher
        // on the *same* loan would hit AlreadyVoted or be the existing proposal.
        // The cleanest way: repay + re-loan, then verify cooldown on the new loan.
        // For now validate via the existing loan path — a second distinct voucher
        // that has NOT voted can cast their vote on the open proposal without hitting
        // the proposal cooldown (only the *initiator* — first voter — triggers it).

        // The proposal cooldown gate is: fired when vote.voters.is_empty(), i.e. the
        // very first vote that initiates the proposal. Subsequent votes on the same open
        // proposal are fine. So we test the cooldown by ensuring a *new* proposal (new loan)
        // is blocked within 7 days.

        // Repay existing loan for borrower2 so we can take a new one.
        StellarAssetClient::new(&s.env, &s.token_id).mint(&borrower2, &102_000);
        s.client.repay(&borrower2, &102_000);

        // Take new loan for borrower2 with the same voucher.
        do_loan(&s, &borrower2, 100_000, 500_000);

        // Attempting to initiate a fresh proposal (first vote) within the 7-day window.
        let result = s.client.try_vote_slash(&voucher2, &borrower2, &true);
        assert_eq!(
            result,
            Err(Ok(ContractError::SlashCooldownActive)),
            "new slash proposal within 7-day cooldown must be rejected"
        );
    }

    /// A new slash proposal succeeds after the 7-day cooldown has elapsed.
    #[test]
    fn test_second_proposal_after_cooldown_succeeds() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let voucher = Address::generate(&s.env);

        do_vouch(&s, &voucher, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 500_000);

        // First proposal (rejected / no quorum reached).
        s.client.vote_slash(&voucher, &borrower, &false);

        // Repay the loan so we can take a new one.
        StellarAssetClient::new(&s.env, &s.token_id).mint(&borrower, &102_000);
        s.client.repay(&borrower, &102_000);

        // Advance past the 7-day proposal cooldown.
        advance(&s, DEFAULT_SLASH_PROPOSAL_COOLDOWN_SECS + 1);

        // Take a new loan.
        do_loan(&s, &borrower, 100_000, 500_000);

        // Should succeed — cooldown has passed.
        let result = s.client.try_vote_slash(&voucher, &borrower, &true);
        assert!(
            result.is_ok(),
            "new slash proposal after 7-day cooldown must succeed, got: {:?}",
            result
        );
    }

    /// Verify DEFAULT_SLASH_PROPOSAL_COOLDOWN_SECS is exactly 7 days (604800 seconds).
    #[test]
    fn test_proposal_cooldown_constant_is_seven_days() {
        assert_eq!(
            DEFAULT_SLASH_PROPOSAL_COOLDOWN_SECS,
            7 * 24 * 60 * 60,
            "DEFAULT_SLASH_PROPOSAL_COOLDOWN_SECS must be 604800 (7 days)"
        );
    }

    // ── Tests: full voting flow ───────────────────────────────────────────────

    /// Happy-path: 2 of 3 equal vouchers approve → quorum met → loan is Defaulted.
    #[test]
    fn test_full_slash_voting_flow_quorum_met() {
        let s = setup();
        s.client.set_slash_vote_quorum(&Vec::from_array(&s.env, [s.admin.clone()]), &6_667);

        let borrower = Address::generate(&s.env);
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);
        let v3 = Address::generate(&s.env);

        do_vouch(&s, &v1, &borrower, 1_000_000);
        do_vouch(&s, &v2, &borrower, 1_000_000);
        do_vouch(&s, &v3, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 2_000_000);

        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Active);

        s.client.vote_slash(&v1, &borrower, &true);
        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Active);

        s.client.vote_slash(&v2, &borrower, &true);

        // v1 + v2 = 2_000_000 / 3_000_000 ≈ 66.67% — quorum reached, slash auto-executed.
        assert_eq!(
            s.client.loan_status(&borrower),
            LoanStatus::Defaulted,
            "loan must be Defaulted once 2/3 quorum is reached"
        );
    }

    /// Sad-path: only 1 of 3 equal vouchers approves → quorum NOT met → loan stays Active.
    #[test]
    fn test_full_slash_voting_flow_quorum_not_met() {
        let s = setup();
        s.client.set_slash_vote_quorum(&Vec::from_array(&s.env, [s.admin.clone()]), &6_667);

        let borrower = Address::generate(&s.env);
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);
        let v3 = Address::generate(&s.env);

        do_vouch(&s, &v1, &borrower, 1_000_000);
        do_vouch(&s, &v2, &borrower, 1_000_000);
        do_vouch(&s, &v3, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 2_000_000);

        s.client.vote_slash(&v1, &borrower, &true);  // 1/3 — not enough

        assert_eq!(
            s.client.loan_status(&borrower),
            LoanStatus::Active,
            "loan must remain Active when quorum is not met"
        );

        let record = s.client.get_slash_vote(&borrower).unwrap();
        assert!(!record.executed);
    }

    /// Mixed votes: approve stake is exactly at the threshold boundary.
    /// 2 approve at 1_500_000 each, 1 rejects at 1_000_000 → total = 4_000_000,
    /// approve = 3_000_000 / 4_000_000 = 75% > 66.67% → slash executes.
    #[test]
    fn test_slash_executes_when_approve_exceeds_two_thirds_with_rejects() {
        let s = setup();
        s.client.set_slash_vote_quorum(&Vec::from_array(&s.env, [s.admin.clone()]), &6_667);

        let borrower = Address::generate(&s.env);
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);
        let v3 = Address::generate(&s.env);

        do_vouch(&s, &v1, &borrower, 1_500_000);
        do_vouch(&s, &v2, &borrower, 1_500_000);
        do_vouch(&s, &v3, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 3_000_000);

        s.client.vote_slash(&v3, &borrower, &false); // reject 1_000_000
        s.client.vote_slash(&v1, &borrower, &true);  // approve 1_500_000 (cumulative 1_500_000/4_000_000 = 37.5%)
        s.client.vote_slash(&v2, &borrower, &true);  // approve 1_500_000 (cumulative 3_000_000/4_000_000 = 75%)

        assert_eq!(
            s.client.loan_status(&borrower),
            LoanStatus::Defaulted,
            "slash must execute when approve stake exceeds 2/3 of total stake"
        );
    }

    /// All vouchers reject → quorum not met → loan stays Active.
    #[test]
    fn test_all_vouchers_reject_no_slash() {
        let s = setup();
        let borrower = Address::generate(&s.env);
        let v1 = Address::generate(&s.env);
        let v2 = Address::generate(&s.env);

        do_vouch(&s, &v1, &borrower, 1_000_000);
        do_vouch(&s, &v2, &borrower, 1_000_000);
        do_loan(&s, &borrower, 100_000, 1_000_000);

        s.client.vote_slash(&v1, &borrower, &false);
        s.client.vote_slash(&v2, &borrower, &false);

        assert_eq!(s.client.loan_status(&borrower), LoanStatus::Active);

        let record = s.client.get_slash_vote(&borrower).unwrap();
        assert_eq!(record.approve_stake, 0);
        assert_eq!(record.reject_stake, 2_000_000);
        assert!(!record.executed);
    }

    /// Default quorum stored by the contract is 6667 bps (≈ 66.67%).
    #[test]
    fn test_default_quorum_is_6667_bps() {
        let s = setup();
        assert_eq!(
            s.client.get_slash_vote_quorum(),
            6_667,
            "default slash vote quorum must be 6667 bps (≈ 66.67%)"
        );
    }
}
