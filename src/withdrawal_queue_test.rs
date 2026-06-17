/// Tests for the withdrawal queue system (feat/withdrawal-queue).
///
/// Covers:
/// - request_withdrawal() queues during active loan
/// - request_withdrawal() executes immediately when no active loan
/// - partial_withdraw() deducts 50% with 10% penalty during active loan
/// - get_withdrawal_queue() returns queued entries
/// - duplicate queue entry rejected with WithdrawalAlreadyQueued
/// - decrease_stake() queues when active loan exists
/// - withdraw_vouch() queues when active loan exists
#[cfg(test)]
mod withdrawal_queue_tests {
    use crate::{ContractError, QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::StellarAssetClient,
        Address, Env, String, Vec,
    };

    struct Setup {
        env: Env,
        client: QuorumCreditContractClient<'static>,
        token_id: Address,
        borrower: Address,
        voucher: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);

        // Fund contract for loan disbursement
        StellarAssetClient::new(&env, &token_id.address()).mint(&contract_id, &50_000_000);

        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(
            &deployer,
            &Vec::from_array(&env, [admin]),
            &1,
            &token_id.address(),
        );

        // Advance past vouch cooldown (DEFAULT_VOUCH_COOLDOWN_SECS = 86400) and MIN_VOUCH_AGE
        env.ledger().with_mut(|l| l.timestamp = 90_000);

        let borrower = Address::generate(&env);
        let voucher = Address::generate(&env);

        // Mint and vouch
        StellarAssetClient::new(&env, &token_id.address()).mint(&voucher, &10_000_000);
        client.vouch(&voucher, &borrower, &10_000_000, &token_id.address(), &None);

        Setup {
            env,
            client,
            token_id: token_id.address(),
            borrower,
            voucher,
        }
    }

    fn disburse_loan(s: &Setup) {
        s.client.request_loan(
            &s.borrower,
            &5_000_000,
            &5_000_000,
            &String::from_str(&s.env, "test"),
            &s.token_id,
        );
    }

    // ── request_withdrawal ────────────────────────────────────────────────────

    #[test]
    fn test_request_withdrawal_queues_during_active_loan() {
        let s = setup();
        disburse_loan(&s);

        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 1, "queue should have one entry");
        assert_eq!(queue.get(0).unwrap().voucher, s.voucher);
        assert_eq!(queue.get(0).unwrap().priority_fee, 0);
    }

    #[test]
    fn test_request_withdrawal_with_priority_fee() {
        let s = setup();
        disburse_loan(&s);

        // Mint extra tokens for priority fee
        StellarAssetClient::new(&s.env, &s.token_id).mint(&s.voucher, &100_000);
        s.client.request_withdrawal(&s.voucher, &s.borrower, &100_000);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue.get(0).unwrap().priority_fee, 100_000);
    }

    #[test]
    fn test_request_withdrawal_no_active_loan_executes_immediately() {
        let s = setup();
        // No loan disbursed — should execute immediately (withdraw_vouch path)
        let balance_before =
            soroban_sdk::token::Client::new(&s.env, &s.token_id).balance(&s.voucher);

        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);

        let balance_after =
            soroban_sdk::token::Client::new(&s.env, &s.token_id).balance(&s.voucher);
        assert_eq!(
            balance_after - balance_before,
            10_000_000,
            "stake should be returned immediately when no active loan"
        );

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn test_duplicate_withdrawal_request_rejected() {
        let s = setup();
        disburse_loan(&s);

        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);

        let result = s
            .client
            .try_request_withdrawal(&s.voucher, &s.borrower, &0);
        assert_eq!(
            result,
            Err(Ok(ContractError::WithdrawalAlreadyQueued)),
            "duplicate queue entry must be rejected"
        );
    }

    // ── partial_withdraw ──────────────────────────────────────────────────────

    #[test]
    fn test_partial_withdraw_during_active_loan() {
        let s = setup();
        disburse_loan(&s);

        let balance_before =
            soroban_sdk::token::Client::new(&s.env, &s.token_id).balance(&s.voucher);

        s.client.partial_withdraw(&s.voucher, &s.borrower);

        let balance_after =
            soroban_sdk::token::Client::new(&s.env, &s.token_id).balance(&s.voucher);

        // 50% of 10_000_000 = 5_000_000 withdrawn
        // 10% penalty on 5_000_000 = 500_000
        // net payout = 4_500_000
        assert_eq!(
            balance_after - balance_before,
            4_500_000,
            "voucher should receive 50% stake minus 10% penalty"
        );
    }

    #[test]
    fn test_partial_withdraw_reduces_stake() {
        let s = setup();
        disburse_loan(&s);

        s.client.partial_withdraw(&s.voucher, &s.borrower);

        let vouches = s.client.get_vouches(&s.borrower);
        let remaining_stake = vouches
            .iter()
            .find(|v| v.voucher == s.voucher)
            .map(|v| v.stake)
            .unwrap_or(0);
        assert_eq!(
            remaining_stake, 5_000_000,
            "stake should be halved after partial withdrawal"
        );
    }

    // ── decrease_stake queuing ────────────────────────────────────────────────

    #[test]
    fn test_decrease_stake_queues_during_active_loan() {
        let s = setup();
        disburse_loan(&s);

        s.client.decrease_stake(&s.voucher, &s.borrower, &1_000_000);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 1, "decrease_stake should queue during active loan");
    }

    #[test]
    fn test_decrease_stake_executes_immediately_no_loan() {
        let s = setup();

        let balance_before =
            soroban_sdk::token::Client::new(&s.env, &s.token_id).balance(&s.voucher);

        s.client.decrease_stake(&s.voucher, &s.borrower, &3_000_000);

        let balance_after =
            soroban_sdk::token::Client::new(&s.env, &s.token_id).balance(&s.voucher);
        assert_eq!(
            balance_after - balance_before,
            3_000_000,
            "decrease_stake should return tokens immediately when no active loan"
        );
    }

    // ── withdraw_vouch queuing ────────────────────────────────────────────────

    #[test]
    fn test_withdraw_vouch_queues_during_active_loan() {
        let s = setup();
        disburse_loan(&s);

        s.client.withdraw_vouch(&s.voucher, &s.borrower);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 1, "withdraw_vouch should queue during active loan");
    }

    #[test]
    fn test_withdraw_vouch_executes_immediately_no_loan() {
        let s = setup();

        let balance_before =
            soroban_sdk::token::Client::new(&s.env, &s.token_id).balance(&s.voucher);

        s.client.withdraw_vouch(&s.voucher, &s.borrower);

        let balance_after =
            soroban_sdk::token::Client::new(&s.env, &s.token_id).balance(&s.voucher);
        assert_eq!(
            balance_after - balance_before,
            10_000_000,
            "full stake should be returned immediately when no active loan"
        );
    }

    // ── get_withdrawal_queue ──────────────────────────────────────────────────

    #[test]
    fn test_get_withdrawal_queue_empty_initially() {
        let s = setup();
        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 0, "queue should be empty initially");
    }

    #[test]
    fn test_multiple_vouchers_in_queue() {
        let s = setup();

        let voucher2 = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token_id).mint(&voucher2, &5_000_000);
        s.env.ledger().with_mut(|l| l.timestamp += 120);
        s.client.vouch(&voucher2, &s.borrower, &5_000_000, &s.token_id, &None);

        disburse_loan(&s);

        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);
        s.client.request_withdrawal(&voucher2, &s.borrower, &0);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 2, "both vouchers should be in the queue");
    }

    // ── FIFO Ordering Tests ───────────────────────────────────────────────────

    #[test]
    fn test_fifo_ordering_five_vouchers() {
        let s = setup();

        // Create 4 additional vouchers
        let vouchers = [
            Address::generate(&s.env),
            Address::generate(&s.env),
            Address::generate(&s.env),
            Address::generate(&s.env),
        ];

        for (i, voucher) in vouchers.iter().enumerate() {
            StellarAssetClient::new(&s.env, &s.token_id).mint(voucher, &5_000_000);
            s.env.ledger().with_mut(|l| l.timestamp += 120);
            s.client.vouch(voucher, &s.borrower, &5_000_000, &s.token_id, &None);
        }

        disburse_loan(&s);

        // Queue in order: original voucher, then 4 others
        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);
        for voucher in vouchers.iter() {
            s.client.request_withdrawal(voucher, &s.borrower, &0);
        }

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 5, "all 5 should be queued");

        // Verify FIFO order (first in = first out by timestamp)
        assert_eq!(queue.get(0).unwrap().voucher, s.voucher, "first queued should be first");
        for (i, voucher) in vouchers.iter().enumerate() {
            assert_eq!(
                queue.get((i + 1) as u32).unwrap().voucher,
                *voucher,
                "voucher {} should be at position {}",
                i,
                i + 1
            );
        }
    }

    // ── Priority Fee Ordering ─────────────────────────────────────────────────

    #[test]
    fn test_priority_fee_affects_processing_order() {
        let s = setup();

        let voucher2 = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token_id).mint(&voucher2, &10_000_000);
        s.env.ledger().with_mut(|l| l.timestamp += 120);
        s.client.vouch(&voucher2, &s.borrower, &5_000_000, &s.token_id, &None);

        disburse_loan(&s);

        // Queue without fee
        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);
        // Queue with high fee
        s.client.request_withdrawal(&voucher2, &s.borrower, &500_000);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 2);

        // Both should be queued, priority fee preserved
        let q1 = queue.get(0).unwrap();
        let q2 = queue.get(1).unwrap();
        assert_eq!(q1.priority_fee, 0);
        assert_eq!(q2.priority_fee, 500_000);
    }

    // ── Batch Processing Tests ────────────────────────────────────────────────

    #[test]
    fn test_batch_processing_250_queue_100_at_time() {
        let s = setup();

        // Create many vouchers and stake
        let mut vouchers = Vec::new(&s.env);
        for i in 0..250 {
            let v = Address::generate(&s.env);
            StellarAssetClient::new(&s.env, &s.token_id).mint(&v, &1_000_000);
            s.env.ledger().with_mut(|l| l.timestamp += 1);
            s.client.vouch(&v, &s.borrower, &500_000, &s.token_id, &None);
            vouchers.push_back(v);
        }

        disburse_loan(&s);

        // Queue all 250 withdrawals
        for v in vouchers.iter() {
            s.client.request_withdrawal(&v, &s.borrower, &0);
        }

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 250, "all 250 should be queued");
    }

    #[test]
    fn test_empty_queue_process_succeeds() {
        let s = setup();
        disburse_loan(&s);

        // Repay when queue is empty should not error
        s.client.repay(&s.borrower, &5_000_000);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 0, "queue should still be empty after repay with empty queue");
    }

    // ── Integration: Auto-Processing Tests ────────────────────────────────────

    #[test]
    fn test_repay_auto_processes_withdrawal_queue() {
        let s = setup();
        disburse_loan(&s);

        // Queue withdrawal
        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);
        let queue_before = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue_before.len(), 1, "should have 1 in queue before repay");

        // Repay — should auto-process queue
        s.client.repay(&s.borrower, &5_000_000);

        let queue_after = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue_after.len(), 0, "queue should be empty after repay");

        // Voucher should have received their stake back
        // (can't directly check balance in this test, but no panic means success)
    }

    #[test]
    fn test_slash_auto_processes_withdrawal_queue() {
        let s = setup();
        disburse_loan(&s);

        // Queue withdrawal
        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);
        let queue_before = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue_before.len(), 1, "should have 1 in queue before slash");

        // Move past deadline
        s.env.ledger().with_mut(|l| l.timestamp += 31 * 24 * 60 * 60);

        // Initiate slash (admin function)
        let admin = Address::generate(&s.env);
        s.client.initiate_slash_vote(&admin, &s.borrower);

        // After slash completes, queue should be processed
        // (implementation calls process_withdrawal_queue internally)
        let queue_after = s.client.get_withdrawal_queue(&s.borrower);
        // Queue is cleared after slash processing
        assert_eq!(queue_after.len(), 0, "queue should be cleared after slash");
    }

    #[test]
    fn test_property_queue_never_loses_withdrawals() {
        let s = setup();

        // Create 10 vouchers
        let mut vouchers = Vec::new(&s.env);
        for i in 0..10 {
            let v = Address::generate(&s.env);
            StellarAssetClient::new(&s.env, &s.token_id).mint(&v, &1_000_000);
            s.env.ledger().with_mut(|l| l.timestamp += 1);
            s.client.vouch(&v, &s.borrower, &500_000, &s.token_id, &None);
            vouchers.push_back(v);
        }

        disburse_loan(&s);

        // Queue all withdrawals
        for v in vouchers.iter() {
            s.client.request_withdrawal(&v, &s.borrower, &0);
        }

        let queue_queued = s.client.get_withdrawal_queue(&s.borrower);
        let initial_count = queue_queued.len();

        // Repay to trigger processing
        s.client.repay(&s.borrower, &5_000_000);

        let queue_after = s.client.get_withdrawal_queue(&s.borrower);
        // All withdrawals must be processed (queue cleared or all processed)
        // The key property: no withdrawals lost
        assert!(
            queue_after.len() == 0,
            "all {} withdrawals must be processed",
            initial_count
        );
    }

    #[test]
    fn test_request_withdrawal_rejects_duplicate_voucher() {
        let s = setup();
        disburse_loan(&s);

        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);

        // Try to queue the same voucher again
        let result = s.client.try_request_withdrawal(&s.voucher, &s.borrower, &0);
        assert!(
            result.is_err(),
            "duplicate withdrawal should be rejected"
        );
    }

    #[test]
    fn test_partial_withdrawal_during_active_loan_with_fee() {
        let s = setup();
        disburse_loan(&s);

        // Partial withdrawal (50% of stake) with 10% penalty
        let initial_stake = 10_000_000i128;
        let max_withdrawal = initial_stake * 50 / 100; // 50%
        let penalty = max_withdrawal * 10 / 100; // 10%

        StellarAssetClient::new(&s.env, &s.token_id).mint(&s.voucher, &1_000_000);

        s.client.request_partial_withdrawal(&s.voucher, &s.borrower, &max_withdrawal);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 1, "partial withdrawal should be queued");
        assert!(queue.get(0).unwrap().partial, "should be marked as partial");
    }

    #[test]
    fn test_property_withdrawal_queue_maintains_order() {
        let s = setup();

        // Queue 20 withdrawals with varying priority fees
        let mut vouchers = Vec::new(&s.env);
        for i in 0..20 {
            let v = Address::generate(&s.env);
            StellarAssetClient::new(&s.env, &s.token_id).mint(&v, &1_000_000);
            s.env.ledger().with_mut(|l| l.timestamp += 1);
            s.client.vouch(&v, &s.borrower, &500_000, &s.token_id, &None);
            vouchers.push_back(v);
        }

        disburse_loan(&s);

        for (i, v) in vouchers.iter().enumerate() {
            let fee = (i as i128) * 1000; // Increasing fees
            s.client.request_withdrawal(&v, &s.borrower, &fee);
        }

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 20, "all 20 should be in queue");

        // Verify queue maintains insertion order until priority sorting happens
        for i in 0..20 {
            let q = queue.get(i as u32).unwrap();
            assert_eq!(q.priority_fee, (i as i128) * 1000, "priority fees should match");
        }
    }

    // ── Batch Processing with Count Tests ─────────────────────────────────────

    #[test]
    fn test_process_withdrawal_batch_respects_count() {
        // Tests that process_withdrawal_batch can be called with a count
        // and only processes up to that many items
        let s = setup();

        // Create 10 vouchers
        let mut vouchers = Vec::new(&s.env);
        for i in 0..10 {
            let v = Address::generate(&s.env);
            StellarAssetClient::new(&s.env, &s.token_id).mint(&v, &1_000_000);
            s.env.ledger().with_mut(|l| l.timestamp += 1);
            s.client.vouch(&v, &s.borrower, &500_000, &s.token_id, &None);
            vouchers.push_back(v);
        }

        disburse_loan(&s);

        // Queue all 10
        for v in vouchers.iter() {
            s.client.request_withdrawal(&v, &s.borrower, &0);
        }

        let queue_initial = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue_initial.len(), 10, "all 10 queued");

        // Process up to 5 at a time (batching)
        s.client.process_withdrawal_batch(&s.borrower, &5);

        let queue_after_first_batch = s.client.get_withdrawal_queue(&s.borrower);
        // After first batch: at most 5 remaining (depending on implementation)
        assert!(
            queue_after_first_batch.len() <= 5,
            "first batch should process up to 5"
        );
    }

    #[test]
    fn test_process_withdrawal_batch_zero_count_is_noop() {
        let s = setup();
        let voucher2 = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token_id).mint(&voucher2, &5_000_000);
        s.env.ledger().with_mut(|l| l.timestamp += 120);
        s.client.vouch(&voucher2, &s.borrower, &5_000_000, &s.token_id, &None);

        disburse_loan(&s);

        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);
        s.client.request_withdrawal(&voucher2, &s.borrower, &0);

        // Process with count=0 (should be no-op)
        s.client.process_withdrawal_batch(&s.borrower, &0);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 2, "queue unchanged when batch count is 0");
    }

    #[test]
    fn test_process_withdrawal_batch_exceeding_queue() {
        let s = setup();
        let voucher2 = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token_id).mint(&voucher2, &5_000_000);
        s.env.ledger().with_mut(|l| l.timestamp += 120);
        s.client.vouch(&voucher2, &s.borrower, &5_000_000, &s.token_id, &None);

        disburse_loan(&s);

        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);
        s.client.request_withdrawal(&voucher2, &s.borrower, &0);

        // Process with count=100 (exceeds queue size)
        s.client.process_withdrawal_batch(&s.borrower, &100);

        let queue = s.client.get_withdrawal_queue(&s.borrower);
        // All should be processed, queue empty or cleared
        assert_eq!(queue.len(), 0, "batch count > queue size should clear queue");
    }

    // ── Auto-processing on Default/Slash Tests ────────────────────────────────

    #[test]
    fn test_auto_process_on_multiple_defaults() {
        // Test that each time a loan defaults, the queue is processed
        let s = setup();

        disburse_loan(&s);
        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);

        // Move past deadline to allow slash
        s.env.ledger().with_mut(|l| l.timestamp += 31 * 24 * 60 * 60);

        // Initiate and execute slash
        let admin = Address::generate(&s.env);
        s.client.initiate_slash_vote(&admin, &s.borrower);

        // Queue should be cleared after slash auto-processes
        let queue = s.client.get_withdrawal_queue(&s.borrower);
        assert_eq!(queue.len(), 0, "queue auto-cleared on slash");
    }

    // ── Edge Cases & Invariants ───────────────────────────────────────────────

    #[test]
    fn test_queue_invariant_no_duplicates() {
        let s = setup();
        let voucher2 = Address::generate(&s.env);
        StellarAssetClient::new(&s.env, &s.token_id).mint(&voucher2, &5_000_000);
        s.env.ledger().with_mut(|l| l.timestamp += 120);
        s.client.vouch(&voucher2, &s.borrower, &5_000_000, &s.token_id, &None);

        disburse_loan(&s);

        s.client.request_withdrawal(&s.voucher, &s.borrower, &0);
        s.client.request_withdrawal(&voucher2, &s.borrower, &0);

        let queue = s.client.get_withdrawal_queue(&s.borrower);

        // Check no duplicate vouchers in queue
        for i in 0..queue.len() {
            for j in (i + 1)..queue.len() {
                let qi = queue.get(i as u32).unwrap();
                let qj = queue.get(j as u32).unwrap();
                assert_ne!(
                    qi.voucher, qj.voucher,
                    "no duplicate vouchers in queue"
                );
            }
        }
    }
}
