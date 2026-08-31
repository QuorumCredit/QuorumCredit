# Loan Priority & Waterfall

> Senior/mezzanine/junior debt tranching and default-proceeds routing — the `loan_priority` module.

---

## Overview

By default, every loan in QuorumCredit shares equal claim on recovered default proceeds. The loan priority module introduces **debt tranching**: each loan can be tagged as Senior, Mezzanine, or Junior, and when default proceeds are distributed, a waterfall algorithm ensures that higher-priority (Senior) loans are made whole first, with lower-priority (Junior) tranches absorbing whatever shortfall remains.

This mirrors the structured finance pattern used in traditional asset-backed securities and enables risk-differentiated lending within a single protocol deployment.

**Key capabilities:**

- Tag loans with a priority tier at queue creation time.
- Route recovered proceeds through a deterministic waterfall (Senior → Mezzanine → Junior).
- Every waterfall run is persisted for audit read-back.
- Governance process to re-tag a loan's tranche after the queue is created, requiring multi-admin approval.

---

## Data Structures

### `LoanPriority` (enum)

The three supported tranches, in descending seniority:

```rust
pub enum LoanPriority {
    Senior,     // rank 0 — paid first; lowest risk to vouchers
    Mezzanine,  // rank 1 — paid second; intermediate risk
    Junior,     // rank 2 — paid last; absorbs the shortfall
}
```

Lower rank = higher priority = paid first. `LoanPriority::Senior` has rank `0`.

### `PriorityLoanEntry`

A single entry in the priority queue: one loan tagged with its tranche.

```rust
pub struct PriorityLoanEntry {
    pub loan_id: u64,
    pub borrower: Address,
    pub priority: LoanPriority,
    pub amount: i128,   // Amount owed for this loan, in stroops
}
```

### `PriorityQueue`

The full ordered queue, persisted in contract storage.

```rust
pub struct PriorityQueue {
    pub entries: Vec<PriorityLoanEntry>,   // Sorted Senior → Mezzanine → Junior
    pub created_at: u64,
    pub updated_at: u64,
}
```

The queue is sorted by rank at creation time (and re-sorted after any governance tranche change). Waterfall routing walks `entries` from index 0 to the end.

### `WaterfallDistributionEntry`

One line of a waterfall run record: how much a given loan actually received.

```rust
pub struct WaterfallDistributionEntry {
    pub loan_id: u64,
    pub borrower: Address,
    pub priority: LoanPriority,
    pub owed: i128,    // Amount owed at waterfall time, in stroops
    pub paid: i128,    // Amount actually distributed, in stroops (may be < owed)
}
```

### `WaterfallRun`

The full audit record for one waterfall execution.

```rust
pub struct WaterfallRun {
    pub run_id: u64,
    pub total_proceeds: i128,   // Total recovered proceeds routed into this run
    pub distributed: i128,      // Total amount actually paid out
    pub shortfall: i128,        // total_proceeds - distributed (unpaid remainder)
    pub timestamp: u64,
    pub entries: Vec<WaterfallDistributionEntry>,
}
```

`shortfall > 0` means that even Junior tranche loans were not fully paid. `shortfall == 0` means all loans were made whole.

### `PriorityChangeProposal`

A pending governance proposal to re-tag a loan's tranche.

```rust
pub struct PriorityChangeProposal {
    pub proposal_id: u64,
    pub loan_id: u64,
    pub new_priority: LoanPriority,
    pub proposer: Address,
    pub approvals: Vec<Address>,   // Admin addresses that have approved
    pub created_at: u64,
    pub executed: bool,
}
```

---

## Functions

### `create_loan_priority_queue(env, admin_signers, loans) -> Result<(), ContractError>`

Build (or replace) the loan priority queue.

- Requires admin approval (`admin_signers` must meet the configured threshold).
- Accepts a `Vec<PriorityLoanEntry>` in any order; the function sorts them by rank (Senior first) using insertion sort before persisting.
- Returns `ContractError::InvalidAmount` if `loans` is empty.
- Replaces any previously existing queue.
- Emits event `(prio, queue_set)` with data `(entry_count, timestamp)`.

**Sorting:** Insertion sort is used deliberately — priority queues are small (per-loan-pool scale) so O(n²) is fine, and this approach keeps the implementation dependency-free under Soroban's `no_std` constraints.

### `get_loan_priority_queue(env) -> Vec<PriorityLoanEntry>`

Read the current priority queue entries. Returns an empty vector if no queue has been created. No auth required.

### `route_default_proceeds(env, admin_signers, total_proceeds) -> Result<WaterfallRun, ContractError>`

Execute the waterfall distribution algorithm over the current priority queue.

- Requires admin approval.
- Returns `ContractError::InvalidAmount` if `total_proceeds < 0` or if no queue exists.
- Walks the sorted queue from Senior to Junior, distributing `min(remaining, entry.amount)` to each entry.
- Persists the `WaterfallRun` record for audit read-back via `get_waterfall_run`.
- Increments the waterfall run counter.
- Emits event `(prio, waterfl)` with data `(run_id, total_proceeds, distributed)`.

**Algorithm:**

```
remaining = total_proceeds
for each entry in queue (Senior → Mezzanine → Junior):
    paid = min(remaining, entry.amount)
    remaining -= paid
    record WaterfallDistributionEntry { owed: entry.amount, paid }
shortfall = total_proceeds - sum(paid)
```

Senior loans receive up to their full `amount`. Any remainder flows to Mezzanine. Junior absorbs whatever is left — which may be zero in a severe default. This is the defining characteristic of a junior tranche.

### `get_waterfall_run(env, run_id) -> Option<WaterfallRun>`

Read back a previously executed waterfall run by its ID. Returns `None` if no run with that ID exists. No auth required.

### `propose_priority_change(env, proposer, loan_id, new_priority) -> Result<u64, ContractError>`

Propose re-tagging a loan's tranche. Returns the new `proposal_id`.

- `proposer` must be an admin (checked via `helpers::is_admin`).
- The proposer is recorded as the first approval.
- Returns `ContractError::UnauthorizedCaller` if `proposer` is not an admin.

### `approve_priority_change(env, approver, proposal_id) -> Result<bool, ContractError>`

Approve a pending priority-change proposal.

- `approver` must be an admin.
- Duplicate approvals from the same address are ignored.
- Returns `ContractError::UnauthorizedCaller` if `approver` is not an admin.
- Returns `ContractError::InvalidAmount` if the proposal does not exist or has already been executed.
- Once the number of distinct approvals meets the contract's `admin_threshold`, the proposal is executed:
  - The loan is re-tagged in the priority queue with `new_priority`.
  - The queue is re-sorted (insertion sort by rank).
  - `proposal.executed` is set to `true`.
- Returns `true` if the proposal was executed in this call, `false` if quorum is not yet met.

### `get_priority_change_proposal(env, proposal_id) -> Option<PriorityChangeProposal>`

Read a pending or executed priority-change governance proposal. No auth required.

---

## Waterfall Algorithm: Worked Example

Given the following queue and `total_proceeds = 800 XLM`:

| Loan | Tranche | Owed |
|---|---|---|
| Loan A | Senior | 500 XLM |
| Loan B | Mezzanine | 400 XLM |
| Loan C | Junior | 300 XLM |

Waterfall execution:

```
remaining = 800 XLM

Loan A (Senior, owed 500):  paid = min(800, 500) = 500;  remaining = 300
Loan B (Mezzanine, owed 400): paid = min(300, 400) = 300; remaining = 0
Loan C (Junior, owed 300):  paid = min(0, 300) = 0;      remaining = 0

WaterfallRun:
  total_proceeds = 800
  distributed    = 800   (500 + 300 + 0)
  shortfall      = 0     (Senior and Mezzanine partially or fully covered)

Distribution entries:
  Loan A: owed=500, paid=500  ✓ (fully paid)
  Loan B: owed=400, paid=300  ~ (partial)
  Loan C: owed=300, paid=0    ✗ (zero — absorbed the full shortfall)
```

---

## Governance: Re-tagging a Tranche

Changing a loan's priority tranche after the queue is built requires multi-admin approval. This prevents a single compromised key from unilaterally elevating a Junior loan to Senior.

**Process:**

1. An admin calls `propose_priority_change(proposer, loan_id, new_priority)` — this registers the proposal and records the proposer as the first approval.
2. Other admins call `approve_priority_change(approver, proposal_id)` until the `admin_threshold` is met.
3. On the approval that meets quorum, the queue is immediately updated and re-sorted. No separate `execute` call is needed.

The proposal is idempotent after execution (`executed = true`); further approval calls will return `ContractError::InvalidAmount`.

---

## Events

| Topic | Data | Trigger |
|---|---|---|
| `(prio, queue_set)` | `(entry_count: u32, timestamp: u64)` | Priority queue created/replaced |
| `(prio, waterfl)` | `(run_id: u64, total_proceeds: i128, distributed: i128)` | Waterfall distribution run executed |

---

## Storage

| Key | Type | Purpose |
|---|---|---|
| `PriorityDataKey::Queue` | `PriorityQueue` | Persistent — the current sorted queue |
| `PriorityDataKey::WaterfallRunCounter` | `u64` | Persistent — monotonic run ID counter |
| `PriorityDataKey::WaterfallRun(run_id)` | `WaterfallRun` | Persistent — individual waterfall audit records |
| `PriorityDataKey::ProposalCounter` | `u64` | Persistent — monotonic proposal ID counter |
| `PriorityDataKey::Proposal(proposal_id)` | `PriorityChangeProposal` | Persistent — governance proposals |

---

## Example Flow

```javascript
// 1. Create a priority queue with three loans
await contract.createLoanPriorityQueue(
    [admin],
    [
        { loan_id: 1n, borrower: addrA, priority: "Senior",     amount: 500_000_000_000n },
        { loan_id: 2n, borrower: addrB, priority: "Junior",     amount: 300_000_000_000n },
        { loan_id: 3n, borrower: addrC, priority: "Mezzanine",  amount: 400_000_000_000n },
    ]
);
// Queue is automatically sorted: Loan 1 (Senior), Loan 3 (Mezzanine), Loan 2 (Junior)

// 2. Route 800 XLM of recovered proceeds
const run = await contract.routeDefaultProceeds([admin], 8_000_000_000_000n);
console.log(`Run #${run.run_id}: distributed ${run.distributed}, shortfall ${run.shortfall}`);

// 3. Audit a run later
const audit = await contract.getWaterfallRun(run.run_id);

// 4. Propose and approve a tranche change (Junior → Mezzanine)
const proposalId = await contract.proposePriorityChange(admin, 2n, "Mezzanine");
await contract.approvePriorityChange(admin2, proposalId);
// If admin_threshold == 2, this approval executes the change immediately.
```
