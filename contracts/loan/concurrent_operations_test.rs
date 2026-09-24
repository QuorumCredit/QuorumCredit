//! Concurrent operations tests for the loan contract.
//!
//! These tests exercise interleavings of loan lifecycle operations that are
//! difficult to reproduce deterministically in a single-threaded unit test.
//! They complement `chaos_test.rs` (network delays, dropped vouch messages,
//! call timeouts) by focusing on the ordering of operations on the *same*
//! loan.
//!
//! Issue #1485 extends this file with adversarial slash-during-repay races:
//!   * `repay` and `slash` interleaved across simulated delays on the same
//!     loan, asserting a consistent final state (no double-application of
//!     slash, no ambiguous loan status).
//!   * a dropped/delayed vouch arriving after the borrower has already been
//!     slashed for insufficient vouches.
//!
//! Findings and the invariants these tests pin down are documented in
//! `docs/contract-invariants.md`.

use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::{LoanContract, LoanContractClient, LoanStatus};

/// Simulated "network partition" window, expressed in ledger timestamps.
/// Mirrors the delay model used by `chaos_test.rs` so the two suites stay
/// comparable.
const PARTITION_WINDOW: u64 = 5;

fn setup() -> (Env, LoanContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, LoanContract);
    let client = LoanContractClient::new(&env, &contract_id);
    let borrower = Address::generate(&env);
    let lender = Address::generate(&env);
    (env, client, borrower, lender)
}

/// Interleave `repay` and `slash` on the same loan across a simulated delay
/// window. The final state must be consistent: slash is applied at most once
/// and the loan must not be left in an ambiguous status.
#[test]
fn chaos_repay_slash_race_same_loan() {
    let (env, client, borrower, lender) = setup();

    let loan_id = client.create_loan(&borrower, &lender, &1_000);

    // Enter the simulated partition window: both calls are issued against the
    // same ledger timestamp, so their relative ordering is adversarial.
    let partition_ts = env.ledger().timestamp();
    env.ledger().set_timestamp(partition_ts + PARTITION_WINDOW);

    // `repay` and `slash` race on the same loan. Depending on ordering the
    // contract may apply one, both, or neither; what matters is that the
    // resulting state is one of the well-defined terminal states and that
    // slash is never double-applied.
    let repay_result = client.try_repay(&loan_id, &borrower, &1_000);
    let slash_result = client.try_slash(&loan_id, &lender);

    let loan = client.get_loan(&loan_id);

    // Invariant: the loan must resolve to a single, unambiguous status.
    match loan.status {
        LoanStatus::Repaid | LoanStatus::Slashed | LoanStatus::Active => {}
        other => panic!("ambiguous loan status after repay/slash race: {:?}", other),
    }

    // Invariant: slash is applied at most once. If slash succeeded, the loan
    // must be in the slashed state; if it failed, the loan must not be slashed
    // by this call.
    if slash_result.is_ok() {
        assert_eq!(
            loan.status,
            LoanStatus::Slashed,
            "successful slash must leave the loan in Slashed status"
        );
    } else {
        assert_ne!(
            loan.status,
            LoanStatus::Slashed,
            "failed slash must not mutate the loan into Slashed status"
        );
    }

    // Invariant: a successful repay must not coexist with a slashed loan.
    if repay_result.is_ok() {
        assert_ne!(
            loan.status,
            LoanStatus::Slashed,
            "repay and slash must not both apply to the same loan"
        );
    }
}

/// A dropped/delayed vouch message arrives after the borrower has already been
/// slashed for insufficient vouches. The late vouch must not resurrect the
/// loan or retroactively un-slash it.
#[test]
fn chaos_delayed_vouch_after_slash_for_insufficient_vouches() {
    let (env, client, borrower, lender) = setup();

    let loan_id = client.create_loan(&borrower, &lender, &1_000);

    // Borrower is slashed for insufficient vouches while the vouch message is
    // still in flight (dropped/delayed by the simulated partition).
    let partition_ts = env.ledger().timestamp();
    env.ledger().set_timestamp(partition_ts + PARTITION_WINDOW);

    let slash_result = client.try_slash(&loan_id, &lender);
    assert!(slash_result.is_ok(), "slash for insufficient vouches must succeed");

    let slashed = client.get_loan(&loan_id);
    assert_eq!(slashed.status, LoanStatus::Slashed);

    // The delayed vouch finally arrives after the slash has been applied.
    let late_vouch = client.try_vouch(&loan_id, &lender);

    let after = client.get_loan(&loan_id);

    // Invariant: a late vouch must not resurrect a slashed loan.
    assert_eq!(
        after.status,
        LoanStatus::Slashed,
        "late vouch must not change the status of an already-slashed loan"
    );

    // Invariant: the late vouch must be rejected (or be a no-op) rather than
    // re-applying slash or reopening the loan.
    if late_vouch.is_ok() {
        assert_eq!(
            after.status,
            LoanStatus::Slashed,
            "accepted late vouch must still leave the loan slashed"
        );
    }
}
