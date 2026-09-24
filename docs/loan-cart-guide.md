# Loan Cart Guide

> Batch loan staging with volume discounts and abandonment analytics — the `loan_cart` module.

---

## Overview

The loan cart system brings an e-commerce metaphor to microlending: a borrower can stage multiple prospective loan requests (varying amounts and tenures) into a "cart" before committing any of them. When the borrower is ready, they submit the entire cart as a single atomic batch operation.

This pattern is useful when a borrower wants to explore options (e.g. "should I request 500 XLM for 30 days or 200 XLM for 90 days?") without locking funds or triggering the single-active-loan restriction on speculative requests.

**Key benefits:**

- Stage multiple loan configurations without disbursing any of them.
- Receive a volume discount (1%) when batching 3 or more items.
- Abandoned carts are tracked for product funnel analytics.
- All-or-nothing semantics are not enforced at the cart level — the cart processes items sequentially and surfaces a per-item result (see [Single-Active-Loan Caveat](#single-active-loan-caveat)).

---

## Data Structures

### `CartItem`

A single staged loan request within a borrower's cart.

```rust
pub struct CartItem {
    pub amount: i128,       // Requested principal in stroops
    pub tenure_secs: u64,   // Requested loan duration in seconds
    pub added_at: u64,      // Ledger timestamp when item was staged
}
```

### `LoanCart`

A borrower's full in-progress batch of staged requests.

```rust
pub struct LoanCart {
    pub borrower: Address,      // Owner of this cart
    pub items: Vec<CartItem>,   // Staged loan requests (in insertion order)
    pub created_at: u64,        // Timestamp of cart creation
    pub last_updated: u64,      // Timestamp of the most recent add/submit/abandon
    pub submitted: bool,        // True after submit_batch_loan_request is called
}
```

A cart is automatically created the first time `add_to_loan_cart` is called for a borrower. There is at most one cart per borrower at any given time.

### `BatchLoanRequestResult`

The outcome of one item in a batch submission.

```rust
pub struct BatchLoanRequestResult {
    pub item_index: u32,          // Zero-based index of the item in the cart
    pub requested_amount: i128,   // Original staged amount (before discount)
    pub discounted_amount: i128,  // Amount actually submitted (after volume discount)
    pub tenure_secs: u64,         // Requested tenure
    pub success: bool,            // Whether request_loan succeeded for this item
    pub error_code: Option<u32>,  // ContractError discriminant on failure; None on success
}
```

### `CartAbandonmentStats`

Protocol-wide funnel analytics, persisted in instance storage.

```rust
pub struct CartAbandonmentStats {
    pub carts_created: u32,     // Total carts ever created
    pub carts_submitted: u32,   // Carts submitted with at least one item
    pub items_added: u32,       // Total items ever staged across all carts
    pub items_submitted: u32,   // Total items that were submitted (regardless of success)
}
```

The abandonment rate is derived off-chain as:
```
abandonment_rate = 1 - (carts_submitted / carts_created)
```

---

## Functions

### `add_to_loan_cart(env, borrower, amount, tenure_secs) -> LoanCart`

Stage a new loan request in the borrower's cart.

- Requires the borrower's auth signature.
- Creates a new cart (and increments `carts_created`) if one does not already exist.
- Appends a `CartItem` and increments `items_added` in the global stats.
- Emits event `(cart, added)` with data `(borrower, amount, tenure_secs)`.
- Panics if `amount <= 0` or `tenure_secs == 0`.

### `get_loan_cart(env, borrower) -> LoanCart`

Read the borrower's current cart. Returns an empty, unsubmitted `LoanCart` if no cart exists (does not create one). Safe to call at any time.

### `abandon_loan_cart(env, borrower)`

Clear a borrower's cart without submitting it.

- Requires the borrower's auth signature.
- If the cart has unstaged items and has not been submitted, emits event `(cart, abandon)` with data `(borrower, item_count)`.
- Removes the cart from persistent storage (the borrower can start a fresh cart afterward).
- Does **not** modify `CartAbandonmentStats` directly — the abandonment rate is computed from the existing `carts_created` vs `carts_submitted` counters.

### `submit_batch_loan_request(env, borrower, loan_purpose, threshold, token) -> Vec<BatchLoanRequestResult>`

Submit every staged item in the borrower's cart as individual `request_loan` calls.

- Requires the borrower's auth signature.
- Applies the [volume discount](#volume-discount) when `cart.items.len() >= 3`.
- Calls `request_loan` for each item in insertion order; collects `BatchLoanRequestResult` for each.
- Increments `items_submitted` for every item that succeeded.
- Increments `carts_submitted` **only if** the cart had at least one item (zero-item carts are a no-op and do not count as a submission — see [#1402 fix](#zero-item-cart-behavior)).
- Clears the cart items and marks `submitted = true` in persistent storage.
- Emits event `(cart, submit)` with data `(borrower, cart_size)`.

### `get_cart_abandonment_stats(env) -> CartAbandonmentStats`

Read the global cart funnel statistics. No auth required. Safe to call at any time.

### `cart_exists(env, borrower) -> bool`

Returns `true` if a persistent cart record exists for the given borrower. Does not inspect cart contents or submission state.

---

## Volume Discount

When a borrower stages **3 or more items** in a single cart, each item receives a 1% discount on its requested principal at submission time.

| Constant | Value | Meaning |
|---|---|---|
| `VOLUME_DISCOUNT_THRESHOLD` | `3` | Minimum items required to trigger the discount |
| `VOLUME_DISCOUNT_BPS` | `100` | Discount in basis points (100 bps = 1%) |

**Discount formula:**

```rust
discounted_amount = amount - (amount * 100 / 10_000)
                  = amount * 99 / 100  // effectively 99% of the original
```

The discount is applied per item at submission time, **not** at staging time. `BatchLoanRequestResult.requested_amount` always carries the original staged amount; `discounted_amount` carries the value actually passed to `request_loan`.

> **Note:** The discount reduces the requested principal rather than adjusting interest rates, so it integrates cleanly with the existing single-loan request path.

---

## Single-Active-Loan Caveat

The underlying QuorumCredit protocol enforces a **one active loan per borrower** invariant. Submitting a cart does not bypass this.

In a batch submission, items are processed sequentially (index 0 first). Once the first item succeeds and a loan is disbursed:

- The second item's `request_loan` call will fail with `ActiveLoanExists` (error code `2`).
- All subsequent items will also fail with `ActiveLoanExists`.
- `BatchLoanRequestResult.success` will be `false` for those items, with `error_code = Some(2)`.

**This is expected behavior.** The cart is a staging and convenience layer, not a mechanism for bypassing the single-loan constraint. The typical intended use case is a borrower staging several options (different amounts/tenures) and submitting them knowing that at most one will succeed — letting the protocol's first-item-wins ordering act as a selection mechanism.

---

## Abandonment Tracking

A cart is "abandoned" when it is created (at least one item staged) but never submitted. The distinction matters for product analytics:

- `carts_created` increments when the first `add_to_loan_cart` call creates a new cart.
- `carts_submitted` increments when `submit_batch_loan_request` is called with at least one staged item.
- The gap (`carts_created - carts_submitted`) represents abandoned carts.

`abandon_loan_cart` allows a borrower to explicitly discard a cart. It emits the `(cart, abandon)` event for real-time monitoring but does not increment any stats counter — the abandonment is implicit in the `carts_created` vs `carts_submitted` delta.

---

## Zero-Item Cart Behavior

As of fix #1402, calling `submit_batch_loan_request` on an empty cart (no items staged) is a **no-op**:

- No `request_loan` calls are made.
- `carts_submitted` is **not** incremented (empty submits are not real submissions).
- The `(cart, submit)` event is still emitted with `cart_size = 0`.
- The cart's `submitted` flag is set to `true` and items are cleared (cart is consumed).

This prevents empty-cart calls from skewing funnel analytics.

---

## Events

| Topic | Data | Trigger |
|---|---|---|
| `(cart, added)` | `(borrower, amount, tenure_secs)` | Item staged via `add_to_loan_cart` |
| `(cart, abandon)` | `(borrower, item_count)` | Non-empty, unsubmitted cart cleared via `abandon_loan_cart` |
| `(cart, submit)` | `(borrower, cart_size)` | Batch submitted via `submit_batch_loan_request` |

---

## Storage

The cart module uses two storage keys, kept separate from the shared `DataKey` enum:

| Key | Type | Purpose |
|---|---|---|
| `CartKey::Cart(borrower)` | `LoanCart` | Persistent — the borrower's active cart |
| `CartKey::AbandonmentStats` | `CartAbandonmentStats` | Instance — global funnel counters |

---

## Example Flow

```javascript
// Stage two loan options
await contract.addToLoanCart(borrower, 500_000_000n, 2592000); // 50 XLM, 30 days
await contract.addToLoanCart(borrower, 200_000_000n, 7776000); // 20 XLM, 90 days

// Check eligibility before submitting
const cart = await contract.getLoanCart(borrower);
console.log(`${cart.items.length} items staged`);

// Submit — first item wins; second will fail with ActiveLoanExists
const results = await contract.submitBatchLoanRequest(
    borrower,
    "Business inventory purchase",
    400_000_000n,  // threshold
    tokenAddress
);

results.forEach((r, i) => {
    if (r.success) {
        console.log(`Item ${i}: loan disbursed for ${r.discounted_amount} stroops`);
    } else {
        console.log(`Item ${i}: failed with error code ${r.error_code}`);
    }
});
```
