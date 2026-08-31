# Reputation Systems in QuorumCredit

QuorumCredit contains **three distinct reputation systems** that track borrower and voucher trustworthiness in different ways. They are intentionally decoupled, each serving a different purpose. This document describes each system, its storage layout, its role in the protocol, and the design decisions behind keeping them separate.

---

## Overview

| # | System | Location | Scale | Primary Purpose |
|---|--------|----------|-------|----------------|
| 1 | `ReputationNftContract` | External contract (`src/reputation.rs`) | u32, unbounded counter | Optional add-on; cross-contract reputation token |
| 2 | `DataKey::ReputationScore` | Main contract persistent storage (`src/reputation_nft.rs`) | u32, unbounded counter | Badge eligibility counter (Centurion badge gate) |
| 3 | `CreditScore.score` | Main contract persistent storage (`src/credit_score.rs`) | 0–1000 | Authoritative on-chain credit history |

---

## System 1 — ReputationNftContract (External Contract)

**Source:** `src/reputation.rs`

### What it is

`ReputationNftContract` is a **separate, independently deployable** Soroban contract. It is not embedded in the main lending contract. It maintains a per-address `u32` reputation score in its own persistent storage under the key `RepKey::Score(Address)`.

The contract exposes three callable functions:

| Function | Description |
|----------|-------------|
| `initialize(minter)` | One-time setup; registers the address authorised to mint/burn (must be the main lending contract). |
| `mint(to)` | Increments the score of `to` by 1. Only callable by the registered minter. |
| `burn(from)` | Decrements the score of `from` by 1, floored at 0. Only callable by the registered minter. |
| `balance(addr)` | Returns the current score (u32) for `addr`. |

### Storage

```
RepKey::Minter           → Address  (instance storage — the authorised caller)
RepKey::Score(Address)   → u32      (persistent storage — per-address score)
```

The score is an **unbounded counter**: every successful repayment increments it by 1, every slash decrements it by 1 (with a floor of 0). There is no maximum value.

### Wiring to the main contract

The main lending contract stores the deployed address of `ReputationNftContract` under `DataKey::ReputationNft` (instance storage). The admin wires it via:

```
set_reputation_nft(admin_signers, nft_contract_address)
```

Once wired, the main contract calls:

- `ReputationNftExternalClient::mint(&borrower)` — on successful loan repayment (in `loan.rs`)
- `ReputationNftExternalClient::burn(&borrower)` — on slash (in `lib.rs` and `loan.rs`)

The score is also readable by the main contract via `get_reputation()`, which calls `ReputationNftExternalClient::balance()`.

### Important: this is an optional add-on

`DataKey::ReputationNft` is only checked with `.get()`. If the admin has **not** called `set_reputation_nft`, the key is absent, the `if let Some(nft_addr)` branch is skipped, and no cross-contract call occurs. The system silently has no effect.

> **Dead code risk:** If `set_reputation_nft` is never called in a deployment, this entire system is inert. Operators should confirm whether they intend to deploy and wire the external contract, or document explicitly that it is not in use.

### Design rationale

The module comment in `src/reputation.rs` gives three reasons for keeping this as a separate contract:

1. The lending contract can be upgraded without resetting reputation data.
2. A single reputation contract can potentially serve multiple lending contracts.
3. The contract can be audited and deployed independently.

---

## System 2 — DataKey::ReputationScore (Badge Eligibility Counter)

**Source:** `src/reputation_nft.rs` — function `evaluate_and_mint_badges`

### What it is

`DataKey::ReputationScore(Address)` is a `u32` value stored in the **main lending contract's persistent storage**. It is a simple counter used as one of the inputs to badge eligibility evaluation.

### Storage

```
DataKey::ReputationScore(Address) → u32   (persistent storage in the main contract)
```

### How it is used

The function `evaluate_and_mint_badges` reads this key to decide whether a borrower qualifies for the `Centurion` badge:

```rust
let rep_score: u32 = env
    .storage()
    .persistent()
    .get::<DataKey, u32>(&DataKey::ReputationScore(address.clone()))
    .unwrap_or(0);
if rep_score >= CENTURION_SCORE_THRESHOLD {  // threshold = 100
    mint_badge(env, address, BadgeType::Centurion);
}
```

`evaluate_and_mint_badges` is called after each successful repayment alongside the repayment count and voucher-backed count checks.

### Scale

`u32`, unbounded counter. `CENTURION_SCORE_THRESHOLD = 100`.

### Write path

As of the current codebase, `DataKey::ReputationScore` is **read** by `evaluate_and_mint_badges` but there is **no write path in the main contract** that increments this counter. The key is defined in `types.rs` and read in `reputation_nft.rs`, but no `storage().persistent().set(&DataKey::ReputationScore(...), ...)` call exists in the main contract's source.

This means:
- The value defaults to `0` for all addresses via `unwrap_or(0)`.
- The `Centurion` badge cannot currently be earned through normal protocol operation.
- This is a **known gap** — see the Known Limitations section below.

---

## System 3 — CreditScore (Authoritative Credit History)

**Source:** `src/credit_score.rs`, struct defined in `src/types.rs`

### What it is

`CreditScore` is a comprehensive struct stored under `DataKey::CreditScore(Address)` in the main lending contract's persistent storage. It is the **authoritative on-chain credit history** for a borrower.

### Storage

```
DataKey::CreditScore(Address) → CreditScore   (persistent storage in the main contract)
```

### Data structure

```rust
pub struct CreditScore {
    pub score: u32,                    // 0–1000 composite score
    pub tier: CreditTier,              // Poor / Fair / Good / VeryGood / Excellent
    pub last_updated: u64,             // Ledger timestamp of last update
    pub last_decay_timestamp: u64,     // Ledger timestamp of last decay (Issue #1072)
    pub total_loans: u32,              // Lifetime loan count
    pub successful_repayments: u32,    // Lifetime successful repayments
    pub defaults: u32,                 // Lifetime default count
    pub total_borrowed: i128,          // Lifetime principal borrowed (stroops)
    pub total_repaid: i128,            // Lifetime principal repaid (stroops)
    pub account_age: u64,              // Account age in seconds
    pub voucher_count: u32,            // Number of times as a voucher
    pub avg_repayment_time: i64,       // Avg seconds before deadline (negative = late)
}
```

### Score scale

`score` is a **0–1000 composite** computed as a weighted average of five components:

| Component | Description |
|-----------|-------------|
| Repayment history | Success rate minus default penalty |
| Loan count | More loans (up to 10) → higher score |
| Account age | Older account → higher score (capped at 1 year) |
| Vouching activity | Stake-time-weighted vouches (Sybil-resistant) |
| Timeliness | Early repayment bonus / late repayment penalty |

Weights are configurable via `CreditScoreConfig.factors` and must sum to 10,000 basis points.

### Credit tiers

| Score range | Tier |
|-------------|------|
| 0–349 | Poor |
| 350–549 | Fair |
| 550–699 | Good |
| 700–849 | VeryGood |
| 850–1000 | Excellent |

### Write path

`DataKey::CreditScore` is written by `update_credit_score()` in `src/credit_score.rs`. This function is called:

- After a successful repayment (`loan.rs` repayment path)
- Can be called externally via the admin/governance path

Score decay is applied by `apply_reputation_decay()`, which is called by the monthly batch function `apply_reputation_decay_batch()`. Decay is configured via `Config::score_decay_per_month` (in basis points per month).

### Role in the protocol

The `CreditScore` struct drives concrete protocol decisions:

- **Yield bonuses** — `apply_tier_rewards_to_yield()` adds BPS based on tier
- **Max loan amount** — `apply_tier_rewards_to_max_loan()` multiplies the cap by tier multiplier
- **Min stake reduction** — `apply_tier_rewards_to_min_stake()` discounts required stake for Excellent-tier borrowers
- **Loan duration extension** — `apply_tier_rewards_to_duration()` extends duration for trusted borrowers
- **Protocol fee discount** — `apply_tier_rewards_to_fee()` reduces fees for high-tier borrowers
- **Excellent badge gate** — `mint_excellent_badge()` (System 1 functions) reads `DataKey::CreditScore` to check eligibility

---

## Relationship Between the Three Systems

The three systems are **intentionally independent**. They were designed for different purposes and are not synchronized with each other.

```
System 1 (ReputationNftContract)
    ↑ minted/burned via cross-contract call, only if wired
    └── stores score in EXTERNAL contract storage (RepKey::Score)

System 2 (DataKey::ReputationScore)
    └── stores a u32 counter in MAIN contract storage
    └── only READ by evaluate_and_mint_badges; no write path in current code

System 3 (DataKey::CreditScore)
    └── stores full CreditScore struct in MAIN contract storage
    └── authoritative; drives yield, loan limits, fee discounts
    └── written by update_credit_score() after every repayment
```

### Design decision: Systems 1 and 2 are independent from System 3

`DataKey::ReputationScore` (System 2) and `CreditScore.score` (System 3) are **not synchronized**. This is intentional:

- `DataKey::ReputationScore` is a simple integer counter intended to gate a specific badge (`Centurion`). It is conceptually separate from the comprehensive credit history tracked in `CreditScore`.
- `CreditScore.score` is a weighted composite on a 0–1000 scale. Mapping it back to the unbounded u32 counter scale used by System 2 would require an arbitrary conversion that could change meaning as the scoring algorithm evolves.
- Keeping them separate allows each to evolve independently without risk of cross-contaminating semantics.

The external `ReputationNftContract` (System 1) is similarly independent: it is a lightweight counter that can survive contract upgrades to the main lending contract, and optionally serves multiple deployments. Merging it into the main contract would remove these properties.

---

## Badge Eligibility

The protocol issues achievement badges gated by the reputation systems:

| Badge | Score Gate | Source System | Trigger |
|-------|-----------|---------------|---------|
| `FirstLoan` | `RepaymentCount >= 1` | — (repayment count, not reputation score) | First successful repayment |
| `TenLoans` | `RepaymentCount >= 10` | — (repayment count) | 10th successful repayment |
| `Centurion` | `DataKey::ReputationScore >= 100` | **System 2** | Score counter reaches 100 |
| `TopVoucher` | `VoucherBackedCount >= 5` | — (voucher activity count) | Voucher backs 5 repaid loans |
| `TrustPillar` | `VoucherBackedCount >= 20` | — (voucher activity count) | Voucher backs 20 repaid loans |
| Excellent Badge (NFT) | `CreditScore.score >= 850`, `successful_repayments >= 2`, `defaults == 0` | **System 3** | Called by `mint_excellent_badge()` after repayment |

Badges can be staked to earn a yield bonus (additive BPS on top of the base yield):

| Badge | Staked Yield Bonus |
|-------|--------------------|
| `FirstLoan` | +25 bps (0.25%) |
| `TenLoans` | +50 bps (0.50%) |
| `Centurion` | +100 bps (1.00%) |
| `TopVoucher` | +30 bps (0.30%) |
| `TrustPillar` | +75 bps (0.75%) |

---

## Known Limitations / Future Work

### 1. Scale mismatch between Systems 2 and 3

System 2 (`DataKey::ReputationScore`) is an **unbounded u32 counter** (0, 1, 2, …, no maximum). System 3 (`CreditScore.score`) is a **bounded 0–1000 composite score**. These are conceptually incompatible units. If future development needs the Centurion badge to be driven by the credit score (e.g., "award Centurion when credit score reaches 700"), a decision must be made:

- Either change `evaluate_and_mint_badges` to read `DataKey::CreditScore` instead of `DataKey::ReputationScore` and apply a threshold in the 0–1000 range.
- Or keep them separate and define a mapping function (e.g., `reputation_score = credit_score.score / 10` for a 0–100 range).

**Recommendation:** If Systems 2 and 3 need to interoperate, consolidate by reading `CreditScore.score` in `evaluate_and_mint_badges` and update the `CENTURION_SCORE_THRESHOLD` constant to reflect the 0–1000 scale (e.g., `700`). Remove `DataKey::ReputationScore` if it is no longer needed independently.

### 2. DataKey::ReputationScore has no write path

`DataKey::ReputationScore` is only read in `evaluate_and_mint_badges`, but there is no code path in the current main contract that writes this key. The value defaults to `0` for all addresses, making the `Centurion` badge currently unmintable through normal protocol operation. A write path (e.g., incrementing `DataKey::ReputationScore` on successful repayment, analogous to `DataKey::RepaymentCount`) must be added for this badge tier to function.

### 3. The external ReputationNftContract is conditionally dead

`ReputationNftContract` (System 1) is only active when an admin has called `set_reputation_nft`. In deployments where this setup step was skipped, the entire external reputation system is dormant. The main contract handles the missing contract address gracefully (the `if let Some(nft_addr)` guard), but operators should explicitly document whether this contract is deployed and wired for a given deployment.

Additionally, the `#[contractimpl]` block for `ReputationNftContract` is gated behind `#[cfg(test)]`. This means the contract implementation is **only compiled in test builds**, not in production WASM. Operators who intend to deploy `ReputationNftContract` to mainnet must remove the `#[cfg(test)]` gate from the `#[contractimpl]` block.

### 4. Future consolidation path

If a future protocol version needs a single unified reputation signal, the recommended consolidation path is:

1. Make `CreditScore.score` (System 3) the single source of truth.
2. Derive all badge thresholds from `CreditScore.score` on the 0–1000 scale.
3. Deprecate `DataKey::ReputationScore` (System 2) after migrating the `Centurion` threshold.
4. Evaluate whether `ReputationNftContract` (System 1) still provides value as an external score signal, or whether it should be deprecated in favour of reading `CreditScore` via `get_reputation()`.

Any such consolidation should be gated behind an ADR and governance vote, as it changes the conditions under which badges are earned and could affect existing badge holders.

---

## Quick Reference: Storage Keys

| Key | Type | Contract | Written by |
|-----|------|----------|------------|
| `RepKey::Score(Address)` | `u32` | External `ReputationNftContract` | `mint()` / `burn()` on `ReputationNftContract` |
| `RepKey::Minter` | `Address` | External `ReputationNftContract` | `initialize()` on `ReputationNftContract` |
| `DataKey::ReputationNft` | `Address` | Main contract (instance) | `set_reputation_nft()` admin function |
| `DataKey::ReputationScore(Address)` | `u32` | Main contract (persistent) | No active write path (see Known Limitations) |
| `DataKey::CreditScore(Address)` | `CreditScore` | Main contract (persistent) | `update_credit_score()` after repayment |
| `DataKey::ReputationNFTBadge(Address)` | `ReputationNFTRecord` | Main contract (persistent) | `mint_excellent_badge()` after repayment |
| `DataKey::ReputationBadge(Address, BadgeType)` | `Badge` | Main contract (persistent) | `mint_badge()` via `evaluate_and_mint_badges()` |

---

## See Also

- [`src/reputation.rs`](../src/reputation.rs) — `ReputationNftContract` implementation and `mint_excellent_badge` / `burn_excellent_badge` helpers
- [`src/reputation_nft.rs`](../src/reputation_nft.rs) — Badge types, `evaluate_and_mint_badges`, staking, and marketplace
- [`src/credit_score.rs`](../src/credit_score.rs) — `CreditScore` computation, tier mapping, tier rewards, and decay
- [`src/types.rs`](../src/types.rs) — `CreditScore`, `CreditTier`, `DataKey`, `ReputationNFTRecord` struct definitions
- [`docs/credit-score-guide.md`](credit-score-guide.md) — Operator guide for the credit score system
- [`docs/credit-score-algorithm.md`](credit-score-algorithm.md) — Detailed scoring algorithm documentation
