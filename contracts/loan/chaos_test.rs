//! Chaos testing for the loan contract.
//!
//! Simulates adversarial network conditions (delays, dropped messages, call
//! timeouts) via ledger-timestamp manipulation and asserts that the contract
//! converges to a consistent state.
//!
//! Issue #1085 introduced the base chaos harness. Issue #1485 extends it to
//! cover the specific adversarial interleaving of a `slash` call racing a
//! `repay` call on the same loan within the same simulated network-partition
//! window, plus a dropped/delayed vouch arriving after a borrower has already
//! been slashed for insufficient vouches.

use soroban_sdk::{testutils::Ledger, Address, Env};

use crate::{LoanContract, LoanContractClient, LoanStatus};

/// Simulated network delay, expressed in ledgers.
const NETWORK_DELAY_LEDGERS: u32 = 5;

/// Advance the ledger sequence and timestamp to simulate a network delay.
fn simulate_delay(env: &Env, ledgers: u32) {
    let seq = env.ledger().sequence();
    env.ledger().set_sequence_number(seq + ledgers);
    let ts = env.ledger().timestamp();
    env.ledger().set_timestamp(ts + (ledgers as u64) * 5);
}

/// Build a loan that is active and has been vouched for by `voucher`.
fn setup_active_loan<'a>(
    env: &'a Env,
    client: &LoanContractClient<'a>,
    borrower: &Address,
    voucher: &Address,
    amount: i128,
) -> u64 {
    let loan_id = client.create_loan(borrower, &amount);
    client.vouch(voucher, &loan_id, &amount);
    loan_id
}

/// Chaos scenario: `repay` and `slash` race on the same loan.
///
/// The two calls are interleaved across a simulated network-partition window.
/// Regardless of ordering, the final state must be consistent:
///   * slash is applied at most once (no double-application),
///   * the loan is not left in an ambiguous status,
///   * the borrower's outstanding balance never goes negative.
#[test]
fn chaos_repay_slash_race_same_loan() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, LoanContract);
    let client = LoanContractClient::new(&env, &contract_id);

    let borrower = Address::generate(&env);
    let voucher = Address::generate(&env);
    let amount: i128 = 1_000;

    let loan_id = setup_active_loan(&env, &client, &borrower, &voucher, amount);

    // Simulate a network partition: the repay call is issued, then the ledger
    // advances before the slash call lands.
    client.repay(&borrower, &loan_id, &(amount / 2));
    simulate_delay(&env, NETWORK_DELAY_LEDGERS);

    // Slash races the in-flight repay. It must not double-apply.
    client.slash(&voucher, &loan_id);

    // A second slash attempt on the same loan must be a no-op / rejected and
    // must not mutate state further.
    let before = client.get_loan(&loan_id);
    let _ = client.try_slash(&voucher, &loan_id);
    let after = client.get_loan(&loan_id);

    assert_eq!(
        before, after,
        "second slash must not double-apply on a loan already slashed"
    );

    // Final state must be unambiguous: either repaid-and-closed or slashed,
    // never a half-applied mixture.
    let loan = client.get_loan(&loan_id);
    assert!(
        matches!(loan.status, LoanStatus::Repaid | LoanStatus::Slashed | LoanStatus::Defaulted),
        "loan left in ambiguous status after repay/slash race: {:?}",
        loan.status
    );
    assert!(
        loan.outstanding >= 0,
        "outstanding balance went negative after repay/slash race"
    );
}

/// Chaos scenario: a vouch message is delayed and arrives *after* the borrower
/// has already been slashed for insufficient vouches.
///
/// The late vouch must not retroactively un-slash the loan or resurrect it.
#[test]
fn chaos_late_vouch_after_slash_for_insufficient_vouches() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, LoanContract);
    let client = LoanContractClient::new(&env, &contract_id);

    let borrower = Address::generate(&env);
    let late_voucher = Address::generate(&env);
    let amount: i128 = 1_000;

    // Create a loan with no vouches, then slash it for insufficient vouches.
    let loan_id = client.create_loan(&borrower, &amount);
    client.slash(&borrower, &loan_id);

    let slashed = client.get_loan(&loan_id);
    assert_eq!(slashed.status, LoanStatus::Slashed);

    // The vouch was in flight during the partition and only now arrives.
    simulate_delay(&env, NETWORK_DELAY_LEDGERS);
    let _ = client.try_vouch(&late_voucher, &loan_id, &amount);

    // The late vouch must not resurrect or un-slash the loan.
    let after = client.get_loan(&loan_id);
    assert_eq!(
        after.status, LoanStatus::Slashed,
        "late vouch must not un-slash a loan already slashed for insufficient vouches"
    );
    assert_eq!(
        after, slashed,
        "late vouch must not mutate the state of an already-slashed loan"
    );
}
