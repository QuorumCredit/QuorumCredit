//! Issue: Batch loan request "cart" system.
//!
//! Lets a borrower stage multiple prospective loan requests (different
//! amounts / tenures) before submitting them together, mirroring an
//! e-commerce cart. Submission applies a small volume discount for
//! borrowers batching 3+ requests, and abandonment (carts created but
//! never submitted) is tracked for product analytics.
//!
//! Note: the underlying protocol only allows a single *active* loan per
//! borrower (see `helpers::has_active_loan`), so batch submission below
//! processes cart items in order and surfaces a per-item result — later
//! items will naturally fail with `ActiveLoanExists` unless earlier items
//! failed to disburse. This module does not change that invariant; it
//! only orchestrates staging and sequential submission against it.

use crate::errors::ContractError;
use soroban_sdk::{contracttype, panic_with_error, symbol_short, Address, Env, String, Vec};

/// Local storage keys for this module, kept separate from the shared
/// `DataKey` enum.
#[contracttype]
#[derive(Clone)]
pub enum CartKey {
    /// borrower -> LoanCart
    Cart(Address),
    /// Global cart funnel statistics.
    AbandonmentStats,
}

/// A single staged loan request within a borrower's cart.
#[contracttype]
#[derive(Clone)]
pub struct CartItem {
    pub amount: i128,
    /// Requested loan tenure, in seconds.
    pub tenure_secs: u64,
    pub added_at: u64,
}

/// A borrower's in-progress batch of staged loan requests.
#[contracttype]
#[derive(Clone)]
pub struct LoanCart {
    pub borrower: Address,
    pub items: Vec<CartItem>,
    pub created_at: u64,
    pub last_updated: u64,
    pub submitted: bool,
}

/// Outcome of submitting a single cart item as part of a batch request.
#[contracttype]
#[derive(Clone)]
pub struct BatchLoanRequestResult {
    pub item_index: u32,
    pub requested_amount: i128,
    /// Amount actually requested after any volume discount was applied —
    /// equal to `requested_amount` (no discount) unless `discount_applied`
    /// is true. Only ever discounted for an item that actually succeeded:
    /// the protocol's single-active-loan invariant means every item after
    /// the first success will fail with `ActiveLoanExists` regardless of
    /// amount (see `request_loan`'s check ordering), so reporting a
    /// discounted price for a request that was never disbursed would be
    /// misleading (issue #1397).
    pub discounted_amount: i128,
    /// Whether a volume discount was actually realized for this item, i.e.
    /// `success && discounted_amount < requested_amount`.
    pub discount_applied: bool,
    pub tenure_secs: u64,
    pub success: bool,
    /// `ContractError` discriminant if the underlying request failed.
    pub error_code: Option<u32>,
}

/// Protocol-wide cart funnel statistics, used to measure abandonment.
#[contracttype]
#[derive(Clone)]
pub struct CartAbandonmentStats {
    pub carts_created: u32,
    pub carts_submitted: u32,
    pub items_added: u32,
    pub items_submitted: u32,
    /// Items removed individually via `remove_cart_item` (#1396). Tracked
    /// separately from abandonment: a borrower editing down a cart before
    /// submitting is a normal part of the flow, not a funnel drop-off —
    /// counting it as abandonment would understate genuine engagement.
    pub items_removed: u32,
    /// Items edited in place via `update_cart_item` (#1396). Tracked
    /// separately for the same reason: an edit is not an abandonment signal.
    pub items_edited: u32,
}

const VOLUME_DISCOUNT_THRESHOLD: u32 = 3;
const VOLUME_DISCOUNT_BPS: i128 = 100; // 1%

fn load_stats(env: &Env) -> CartAbandonmentStats {
    env.storage()
        .instance()
        .get(&CartKey::AbandonmentStats)
        .unwrap_or(CartAbandonmentStats {
            carts_created: 0,
            carts_submitted: 0,
            items_added: 0,
            items_submitted: 0,
            items_removed: 0,
            items_edited: 0,
        })
}

fn save_stats(env: &Env, stats: &CartAbandonmentStats) {
    env.storage()
        .instance()
        .set(&CartKey::AbandonmentStats, stats);
}

/// Stage a new loan request in the borrower's cart. Creates the cart if
/// this is the borrower's first staged item.
///
/// Staging multiple items is always allowed — there is no limit here tied to
/// the protocol's single-active-loan constraint, since a borrower may stage
/// several candidate requests to compare before submitting. That constraint
/// is enforced at *submission* time instead (see `submit_batch_loan_request`),
/// where it determines how many staged items can actually disburse
/// (issue #1397).
pub fn add_to_loan_cart(
    env: Env,
    borrower: Address,
    amount: i128,
    tenure_secs: u64,
) -> LoanCart {
    borrower.require_auth();
    assert!(amount > 0, "cart: amount must be positive");
    assert!(tenure_secs > 0, "cart: tenure must be positive");

    let now = env.ledger().timestamp();
    let mut stats = load_stats(&env);

    let mut cart: LoanCart = env
        .storage()
        .persistent()
        .get(&CartKey::Cart(borrower.clone()))
        .unwrap_or_else(|| {
            stats.carts_created += 1;
            LoanCart {
                borrower: borrower.clone(),
                items: Vec::new(&env),
                created_at: now,
                last_updated: now,
                submitted: false,
            }
        });

    cart.items.push_back(CartItem {
        amount,
        tenure_secs,
        added_at: now,
    });
    cart.last_updated = now;
    cart.submitted = false;

    stats.items_added += 1;
    save_stats(&env, &stats);

    env.storage()
        .persistent()
        .set(&CartKey::Cart(borrower.clone()), &cart);

    env.events().publish(
        (symbol_short!("cart"), symbol_short!("added")),
        (borrower, amount, tenure_secs),
    );

    cart
}

/// Load `borrower`'s cart, panicking with `ContractError::NotFound` if
/// `item_index` isn't a valid index into it. Shared by `remove_cart_item`
/// and `update_cart_item` (#1396) so both reject an out-of-range index the
/// same way.
fn load_cart_with_valid_index(env: &Env, borrower: &Address, item_index: u32) -> LoanCart {
    let cart: LoanCart = env
        .storage()
        .persistent()
        .get(&CartKey::Cart(borrower.clone()))
        .unwrap_or_else(|| panic_with_error!(env, ContractError::NotFound));

    if item_index >= cart.items.len() {
        panic_with_error!(env, ContractError::NotFound);
    }

    cart
}

/// Remove a single staged item from the borrower's cart by index, without
/// discarding the rest of the cart. Previously the only way to correct a
/// mis-entered item was `abandon_loan_cart`, which throws away every staged
/// item (#1396).
///
/// Panics with `ContractError::NotFound` if the borrower has no cart, or if
/// `item_index` is out of range for it.
pub fn remove_cart_item(env: Env, borrower: Address, item_index: u32) -> LoanCart {
    borrower.require_auth();

    let mut cart = load_cart_with_valid_index(&env, &borrower, item_index);
    cart.items.remove(item_index);
    cart.last_updated = env.ledger().timestamp();

    let mut stats = load_stats(&env);
    stats.items_removed += 1;
    save_stats(&env, &stats);

    env.storage()
        .persistent()
        .set(&CartKey::Cart(borrower.clone()), &cart);

    env.events().publish(
        (symbol_short!("cart"), symbol_short!("removed")),
        (borrower, item_index),
    );

    cart
}

/// Replace the amount/tenure of a single staged item in place, without
/// disturbing its position or the rest of the cart (#1396).
///
/// Panics with `ContractError::NotFound` if the borrower has no cart, or if
/// `item_index` is out of range for it; with the same `assert!`s as
/// `add_to_loan_cart` if the new amount/tenure aren't positive.
pub fn update_cart_item(
    env: Env,
    borrower: Address,
    item_index: u32,
    amount: i128,
    tenure_secs: u64,
) -> LoanCart {
    borrower.require_auth();
    assert!(amount > 0, "cart: amount must be positive");
    assert!(tenure_secs > 0, "cart: tenure must be positive");

    let mut cart = load_cart_with_valid_index(&env, &borrower, item_index);
    let now = env.ledger().timestamp();

    let existing = cart.items.get(item_index).unwrap();
    cart.items.set(
        item_index,
        CartItem {
            amount,
            tenure_secs,
            added_at: existing.added_at,
        },
    );
    cart.last_updated = now;

    let mut stats = load_stats(&env);
    stats.items_edited += 1;
    save_stats(&env, &stats);

    env.storage()
        .persistent()
        .set(&CartKey::Cart(borrower.clone()), &cart);

    env.events().publish(
        (symbol_short!("cart"), symbol_short!("edited")),
        (borrower, item_index, amount, tenure_secs),
    );

    cart
}

/// Read a borrower's current cart contents.
pub fn get_loan_cart(env: Env, borrower: Address) -> LoanCart {
    env.storage()
        .persistent()
        .get(&CartKey::Cart(borrower.clone()))
        .unwrap_or(LoanCart {
            borrower,
            items: Vec::new(&env),
            created_at: 0,
            last_updated: 0,
            submitted: false,
        })
}

/// Clear a borrower's cart without submitting it. Counts toward
/// abandonment statistics since the cart had items staged but never
/// resulted in a submission.
pub fn abandon_loan_cart(env: Env, borrower: Address) {
    borrower.require_auth();

    if let Some(cart) = env
        .storage()
        .persistent()
        .get::<CartKey, LoanCart>(&CartKey::Cart(borrower.clone()))
    {
        if !cart.submitted && cart.items.len() > 0 {
            env.events().publish(
                (symbol_short!("cart"), symbol_short!("abandon")),
                (borrower.clone(), cart.items.len()),
            );
        }
    }

    env.storage()
        .persistent()
        .remove(&CartKey::Cart(borrower.clone()));
}

/// Compute the discounted amount for a single item given the cart's total
/// size. Batches of 3 or more staged loans receive a 1% discount on each
/// item's requested principal.
fn apply_volume_discount(amount: i128, cart_size: u32) -> i128 {
    if cart_size >= VOLUME_DISCOUNT_THRESHOLD {
        amount - (amount * VOLUME_DISCOUNT_BPS / 10_000)
    } else {
        amount
    }
}

/// Submit every staged item in the borrower's cart as an individual loan
/// request, applying the volume discount when 3+ items are present.
/// Clears the cart afterward and records submission statistics.
///
/// Returns a per-item result so callers can see which requests succeeded;
/// see the module-level note about the single-active-loan invariant.
///
/// **Under that invariant, at most one item per submission can actually
/// disburse**: the first successful `request_loan` call creates the
/// borrower's active loan, and `request_loan` checks for an existing active
/// loan before it even looks at the requested amount, so every later item
/// fails with `ActiveLoanExists` regardless of the amount or any discount
/// (issue #1397). `discounted_amount`/`discount_applied` on the returned
/// results reflect this: only an item that actually succeeds can carry a
/// realized discount — a failed item's `discounted_amount` always equals
/// `requested_amount`, so callers never display a discounted price for a
/// loan that was never funded. If the batch produced fewer successes than
/// staged items, a `("cart", "partial")` event is published alongside the
/// usual `("cart", "submit")` event so downstream consumers (dashboards,
/// analytics) can flag it rather than assume every staged item was funded.
pub fn submit_batch_loan_request(
    env: Env,
    borrower: Address,
    loan_purpose: String,
    threshold: i128,
    token: Address,
) -> Vec<BatchLoanRequestResult> {
    borrower.require_auth();

    let cart: LoanCart = env
        .storage()
        .persistent()
        .get(&CartKey::Cart(borrower.clone()))
        .unwrap_or(LoanCart {
            borrower: borrower.clone(),
            items: Vec::new(&env),
            created_at: env.ledger().timestamp(),
            last_updated: env.ledger().timestamp(),
            submitted: false,
        });

    let cart_size = cart.items.len();
    let mut results: Vec<BatchLoanRequestResult> = Vec::new(&env);
    let mut stats = load_stats(&env);
    let mut success_count: u32 = 0;

    for i in 0..cart_size {
        let item = cart.items.get(i).unwrap();
        let discounted = apply_volume_discount(item.amount, cart_size);

        let outcome = crate::loan::request_loan(
            env.clone(),
            borrower.clone(),
            discounted,
            threshold,
            loan_purpose.clone(),
            token.clone(),
        );

        let (success, error_code) = match outcome {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e as u32)),
        };

        if success {
            stats.items_submitted += 1;
            success_count += 1;
        }

        // Only a request that actually disbursed can carry a realized
        // discount — reporting `discounted` for a failed item would present
        // a price that was never actually honored (issue #1397).
        let discounted_amount = if success { discounted } else { item.amount };

        results.push_back(BatchLoanRequestResult {
            item_index: i,
            requested_amount: item.amount,
            discounted_amount,
            discount_applied: success && discounted_amount < item.amount,
            tenure_secs: item.tenure_secs,
            success,
            error_code,
        });
    }

    stats.carts_submitted += 1;
    save_stats(&env, &stats);

    if success_count < cart_size {
        // Signals that the batch could not be fully honored — expected under
        // the single-active-loan invariant for any cart with 2+ items, not a
        // failure of the cart mechanism itself (issue #1397).
        env.events().publish(
            (symbol_short!("cart"), symbol_short!("partial")),
            (borrower.clone(), success_count, cart_size),
        );
    }

    env.storage().persistent().set(
        &CartKey::Cart(borrower.clone()),
        &LoanCart {
            borrower: borrower.clone(),
            items: Vec::new(&env),
            created_at: cart.created_at,
            last_updated: env.ledger().timestamp(),
            submitted: true,
        },
    );

    env.events().publish(
        (symbol_short!("cart"), symbol_short!("submit")),
        (borrower, cart_size),
    );

    results
}

/// Read protocol-wide cart funnel statistics (creation vs. submission vs.
/// abandonment), for dashboards and product analytics.
pub fn get_cart_abandonment_stats(env: Env) -> CartAbandonmentStats {
    load_stats(&env)
}

/// Convenience error used by wrapper entry points when a cart operation is
/// attempted with no cart present.
pub fn cart_exists(env: Env, borrower: Address) -> bool {
    env.storage()
        .persistent()
        .has(&CartKey::Cart(borrower))
}
