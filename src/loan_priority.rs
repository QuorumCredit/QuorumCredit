/// Issue: Loan Priority / Subordination (senior-junior debt structures).
///
/// Today every loan shares equal claim on default proceeds. This module adds a
/// priority queue so loans can be tagged Senior / Mezzanine / Junior, and default
/// proceeds are routed through a waterfall: Senior is made whole first, then
/// Mezzanine, then Junior absorbs any shortfall.
///
/// Issue #12 fix: PriorityQueue is now keyed by `pool_id` so multiple concurrent
/// syndication pools / origination batches can each maintain their own independent
/// priority tranche structure.  The single-queue callers now pass an explicit
/// `pool_id`; `PriorityDataKey::Queue` has been replaced with
/// `PriorityDataKey::Queue(u64)`.
use soroban_sdk::{contracttype, symbol_short, Address, Env, Vec};

use crate::errors::ContractError;
use crate::helpers;

/// Priority tier for a loan. Lower-numbered tiers are paid first in a default
/// waterfall.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoanPriority {
    Senior,
    Mezzanine,
    Junior,
}

impl LoanPriority {
    /// Sort rank used to order the priority queue (0 = paid first).
    fn rank(&self) -> u32 {
        match self {
            LoanPriority::Senior => 0,
            LoanPriority::Mezzanine => 1,
            LoanPriority::Junior => 2,
        }
    }
}

/// A single entry in the priority queue: a loan tagged with its tranche.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriorityLoanEntry {
    pub loan_id: u64,
    pub borrower: Address,
    pub priority: LoanPriority,
    pub amount: i128,
}

/// The full ordered priority queue, persisted so waterfall routing can be
/// replayed deterministically.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriorityQueue {
    pub pool_id: u64,
    pub entries: Vec<PriorityLoanEntry>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// One line of the default waterfall distribution record: how much of the
/// total recovered proceeds a given loan/tranche actually received.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaterfallDistributionEntry {
    pub loan_id: u64,
    pub borrower: Address,
    pub priority: LoanPriority,
    pub owed: i128,
    pub paid: i128,
}

/// A full record of one waterfall run, for audit purposes.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaterfallRun {
    pub run_id: u64,
    pub pool_id: u64,
    pub total_proceeds: i128,
    pub distributed: i128,
    pub shortfall: i128,
    pub timestamp: u64,
    pub entries: Vec<WaterfallDistributionEntry>,
}

/// A pending governance proposal to change a loan's priority tier.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriorityChangeProposal {
    pub proposal_id: u64,
    pub pool_id: u64,
    pub loan_id: u64,
    pub new_priority: LoanPriority,
    pub proposer: Address,
    pub approvals: Vec<Address>,
    pub created_at: u64,
    pub executed: bool,
}

/// Storage keys for the loan priority module.
///
/// Issue #12: `Queue` is now parameterised by `pool_id` so each syndication
/// pool / origination batch has its own independent priority queue.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
enum PriorityDataKey {
    /// pool_id → PriorityQueue  (replaces the former bare `Queue` key)
    Queue(u64),
    WaterfallRunCounter,
    WaterfallRun(u64),
    ProposalCounter,
    Proposal(u64),
}

// ── helpers ───────────────────────────────────────────────────────────────────

/// Insertion-sort `entries` by tranche rank (Senior first).
/// O(n²) is intentional — queues are small (per-pool scale) and this avoids
/// pulling in std sorting under no_std.
fn sort_by_priority(env: &Env, entries: Vec<PriorityLoanEntry>) -> Vec<PriorityLoanEntry> {
    let mut sorted: Vec<PriorityLoanEntry> = Vec::new(env);
    for entry in entries.iter() {
        let mut insert_at = sorted.len();
        for i in 0..sorted.len() {
            if sorted.get(i).unwrap().priority.rank() > entry.priority.rank() {
                insert_at = i;
                break;
            }
        }
        sorted.insert(insert_at, entry.clone());
    }
    sorted
}

// ── public API ────────────────────────────────────────────────────────────────

/// Build (or replace) the loan priority queue for **`pool_id`** from a
/// caller-supplied set of (loan, tranche) tuples.  Each pool maintains its own
/// independent queue; calling this for pool A does not affect pool B.
///
/// The queue is stored sorted Senior → Mezzanine → Junior so waterfall routing
/// can walk it in order.
///
/// Issue #12: Added `pool_id` parameter; storage key changed from
/// `PriorityDataKey::Queue` to `PriorityDataKey::Queue(pool_id)`.
pub fn create_loan_priority_queue(
    env: Env,
    admin_signers: Vec<Address>,
    pool_id: u64,
    loans: Vec<PriorityLoanEntry>,
) -> Result<(), ContractError> {
    helpers::require_admin_approval(&env, &admin_signers);

    if loans.is_empty() {
        return Err(ContractError::InvalidAmount);
    }

    let sorted = sort_by_priority(&env, loans.clone());
    let now = env.ledger().timestamp();
    let queue = PriorityQueue {
        pool_id,
        entries: sorted,
        created_at: now,
        updated_at: now,
    };
    env.storage()
        .persistent()
        .set(&PriorityDataKey::Queue(pool_id), &queue);

    env.events().publish(
        (symbol_short!("prio"), symbol_short!("queue_set")),
        (pool_id, loans.len() as u32, now),
    );

    Ok(())
}

/// Read the current priority queue for the given `pool_id`.
///
/// Issue #12: Now keyed by `pool_id` instead of a single global key.
pub fn get_loan_priority_queue(env: Env, pool_id: u64) -> Vec<PriorityLoanEntry> {
    env.storage()
        .persistent()
        .get::<PriorityDataKey, PriorityQueue>(&PriorityDataKey::Queue(pool_id))
        .map(|q| q.entries)
        .unwrap_or(Vec::new(&env))
}

/// Route recovered default proceeds through the Senior → Mezzanine → Junior
/// waterfall for **`pool_id`**.  Senior loans are made whole first (up to
/// `amount` owed), any remainder flows to Mezzanine, and Junior absorbs
/// whatever is left — which may be zero in a severe default.
///
/// Returns the full distribution record and persists it for later audit/
/// read-back via `get_waterfall_run`.
///
/// Issue #12: Added `pool_id` parameter; looks up `PriorityDataKey::Queue(pool_id)`
/// and records `pool_id` in the `WaterfallRun` for traceability.
pub fn route_default_proceeds(
    env: Env,
    admin_signers: Vec<Address>,
    pool_id: u64,
    total_proceeds: i128,
) -> Result<WaterfallRun, ContractError> {
    helpers::require_admin_approval(&env, &admin_signers);

    if total_proceeds < 0 {
        return Err(ContractError::InvalidAmount);
    }

    let queue = env
        .storage()
        .persistent()
        .get::<PriorityDataKey, PriorityQueue>(&PriorityDataKey::Queue(pool_id))
        .ok_or(ContractError::InvalidAmount)?;

    let mut remaining = total_proceeds;
    let mut entries: Vec<WaterfallDistributionEntry> = Vec::new(&env);
    let mut distributed: i128 = 0;

    for e in queue.entries.iter() {
        let paid = if remaining >= e.amount {
            e.amount
        } else if remaining > 0 {
            remaining
        } else {
            0
        };
        remaining -= paid;
        distributed += paid;

        if paid > 0 {
            let loan = helpers::get_loan_by_id(&env, &e.loan_id)?;
            let token_client = token::Client::new(&env, &loan.token_address);
            token_client.transfer(&env.current_contract_address(), &e.borrower, &paid);

            // Per-entry payout event, distinct from the aggregate "waterfl" event
            // below — lets a caller watch exactly which loans/addresses were paid
            // and in which token, rather than only the run-wide totals.
            env.events().publish(
                (symbol_short!("prio"), symbol_short!("payout")),
                (e.loan_id, e.borrower.clone(), paid, loan.token_address.clone()),
            );
        }

        entries.push_back(WaterfallDistributionEntry {
            loan_id: e.loan_id,
            borrower: e.borrower.clone(),
            priority: e.priority.clone(),
            owed: e.amount,
            paid,
        });
    }

    let run_id: u64 = env
        .storage()
        .persistent()
        .get(&PriorityDataKey::WaterfallRunCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage()
        .persistent()
        .set(&PriorityDataKey::WaterfallRunCounter, &run_id);

    let run = WaterfallRun {
        run_id,
        pool_id,
        total_proceeds,
        distributed,
        shortfall: total_proceeds - distributed,
        timestamp: env.ledger().timestamp(),
        entries,
    };
    env.storage()
        .persistent()
        .set(&PriorityDataKey::WaterfallRun(run_id), &run);

    // #1392: clear the queue now that this batch has actually been paid out —
    // see the double-payout guard note in the doc comment above.
    env.storage().persistent().remove(&PriorityDataKey::Queue);

    env.events().publish(
        (symbol_short!("prio"), symbol_short!("waterfl")),
        (run_id, pool_id, total_proceeds, distributed),
    );

    Ok(run)
}

/// Read back a previously executed waterfall distribution run.
pub fn get_waterfall_run(env: Env, run_id: u64) -> Option<WaterfallRun> {
    env.storage()
        .persistent()
        .get(&PriorityDataKey::WaterfallRun(run_id))
}

/// Propose a change to a loan's priority tier within **`pool_id`**.
/// Requires the same multi-admin approval threshold as other governance
/// actions; the proposer is recorded as the first approval.
///
/// Issue #12: `pool_id` is now recorded on the proposal so `approve_priority_change`
/// can look up the correct per-pool queue.
pub fn propose_priority_change(
    env: Env,
    proposer: Address,
    pool_id: u64,
    loan_id: u64,
    new_priority: LoanPriority,
) -> Result<u64, ContractError> {
    proposer.require_auth();
    if !helpers::is_admin(&env, &proposer) {
        return Err(ContractError::UnauthorizedCaller);
    }

    let proposal_id: u64 = env
        .storage()
        .persistent()
        .get(&PriorityDataKey::ProposalCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    env.storage()
        .persistent()
        .set(&PriorityDataKey::ProposalCounter, &proposal_id);

    let mut approvals = Vec::new(&env);
    approvals.push_back(proposer.clone());

    let proposal = PriorityChangeProposal {
        proposal_id,
        pool_id,
        loan_id,
        new_priority,
        proposer,
        approvals,
        created_at: env.ledger().timestamp(),
        executed: false,
    };
    env.storage()
        .persistent()
        .set(&PriorityDataKey::Proposal(proposal_id), &proposal);

    Ok(proposal_id)
}

/// Approve a pending priority-change proposal. Once the number of distinct
/// admin approvals meets the contract's admin threshold, the change is applied
/// to the **pool-specific** priority queue (the loan is re-tagged and the queue
/// re-sorted).
///
/// Issue #12: Now reads/writes `PriorityDataKey::Queue(proposal.pool_id)`.
pub fn approve_priority_change(
    env: Env,
    approver: Address,
    proposal_id: u64,
) -> Result<bool, ContractError> {
    approver.require_auth();
    if !helpers::is_admin(&env, &approver) {
        return Err(ContractError::UnauthorizedCaller);
    }

    let mut proposal: PriorityChangeProposal = env
        .storage()
        .persistent()
        .get(&PriorityDataKey::Proposal(proposal_id))
        .ok_or(ContractError::InvalidAmount)?;

    if proposal.executed {
        return Err(ContractError::InvalidAmount);
    }
    if !proposal.approvals.iter().any(|a| a == approver) {
        proposal.approvals.push_back(approver);
    }

    let cfg = helpers::config(&env);
    let threshold_met = proposal.approvals.len() >= cfg.admin_threshold;

    if threshold_met {
        let mut queue = env
            .storage()
            .persistent()
            .get::<PriorityDataKey, PriorityQueue>(&PriorityDataKey::Queue(proposal.pool_id))
            .ok_or(ContractError::InvalidAmount)?;

        let mut updated: Vec<PriorityLoanEntry> = Vec::new(&env);
        for e in queue.entries.iter() {
            if e.loan_id == proposal.loan_id {
                updated.push_back(PriorityLoanEntry {
                    loan_id: e.loan_id,
                    borrower: e.borrower.clone(),
                    priority: proposal.new_priority.clone(),
                    amount: e.amount,
                });
            } else {
                updated.push_back(e.clone());
            }
        }

        queue.entries = sort_by_priority(&env, updated);
        queue.updated_at = env.ledger().timestamp();
        env.storage()
            .persistent()
            .set(&PriorityDataKey::Queue(proposal.pool_id), &queue);

        proposal.executed = true;
    }

    env.storage()
        .persistent()
        .set(&PriorityDataKey::Proposal(proposal_id), &proposal);

    Ok(proposal.executed)
}

/// Read a priority-change governance proposal.
pub fn get_priority_change_proposal(env: Env, proposal_id: u64) -> Option<PriorityChangeProposal> {
    env.storage()
        .persistent()
        .get(&PriorityDataKey::Proposal(proposal_id))
}
