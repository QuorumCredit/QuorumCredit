//! # Insurance Marketplace
//!
//! On-chain insurance marketplace for QuorumCredit microloans.
//!
//! ## Design
//!
//! ### Provider Adapter Pattern
//!
//! The central abstraction is [`QuoteProvider`], a trait that turns a product
//! definition + loan amount into a `(coverage_amount, premium_amount)` pair.
//! This is the explicit seam called for in issue #1218.
//!
//! Two concrete implementations are shipped:
//!
//! * [`StaticRateProvider`] — **explicit fallback**, labeled as such.  It
//!   computes quotes using the product's stored `coverage_pct_bps` /
//!   `premium_bps` constants with no external call.  This is the
//!   `StaticRateProvider` that replaces the silent, comment-only "TODO: call
//!   real API" arithmetic from the original TypeScript version.
//!
//! * [`MockProvider`] — test-only helper.  Takes an explicit rate table so
//!   that two instances with *different* tables produce *different* quotes,
//!   proving the adapter boundary is exercised by tests.  Only compiled under
//!   `#[cfg(test)]`.
//!
//! In production a third adapter (`ExternalHttpProvider` or similar) would
//! implement `QuoteProvider` and be selected via the provider's `adapter_tag`.
//! The routing logic in `fetch_provider_quote` already has the dispatch point.
//!
//! ### Persistence
//!
//! All state — providers, products, quotes, claims — lives in Soroban's
//! `persistent()` storage, keyed by the monotonic ID counters stored in
//! `instance()`.  This is the same pattern used by loans, vouches, and pools.
//! Because Soroban persistent storage is shared ledger state, every node in a
//! validator set (the "multi-instance" deployment) sees the same data.

use crate::errors::ContractError;
use crate::types::{
    ClaimStatus, DataKey, InsuranceClaim, InsuranceProduct, InsuranceProvider, InsuranceQuote,
};
use soroban_sdk::{symbol_short, Bytes, Env, String as SorobanString};

// ─────────────────────────────────────────────────────────────────────────────
// QuoteProvider trait
// ─────────────────────────────────────────────────────────────────────────────

/// Adapter interface for insurance quote computation.
///
/// Implementors translate a `product` + `loan_amount` into a concrete
/// `(coverage_amount, premium_amount)` pair denominated in stroops.
///
/// The trait is intentionally minimal: it carries no `Env` dependency so it
/// can be implemented both on-chain (using only arithmetic) and — in a future
/// iteration — off-chain (by querying a third-party REST API before submitting
/// the result in an oracle transaction).
pub trait QuoteProvider {
    /// Compute `(coverage_amount, premium_amount)` for `loan_amount` stroops.
    ///
    /// Implementations **must** be deterministic for the same inputs when
    /// called on-chain, but are free to return provider-specific values that
    /// differ from another provider's implementation for the same inputs.
    fn compute_quote(
        &self,
        product: &InsuranceProduct,
        loan_amount: i128,
    ) -> (i128, i128);
}

// ─────────────────────────────────────────────────────────────────────────────
// StaticRateProvider — explicit fallback
// ─────────────────────────────────────────────────────────────────────────────

/// **Fallback** quote provider that uses the product's stored basis-point rates.
///
/// This is the explicit, correctly-labeled replacement for the "In production,
/// call actual third-party API / For now, calculate based on static product
/// rates" comment found in the original TypeScript implementation.  It is NOT
/// silently masquerading as a live call; it is the declared fallback when no
/// live adapter is wired in.
///
/// Formula:
/// ```text
/// coverage_amount = loan_amount * coverage_pct_bps / 10_000
/// premium_amount  = loan_amount * premium_bps       / 10_000
/// ```
pub struct StaticRateProvider;

impl QuoteProvider for StaticRateProvider {
    fn compute_quote(&self, product: &InsuranceProduct, loan_amount: i128) -> (i128, i128) {
        let coverage = loan_amount * (product.coverage_pct_bps as i128) / 10_000;
        let premium = loan_amount * (product.premium_bps as i128) / 10_000;
        (coverage, premium)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// MockProvider — test-only adapter with explicit rate overrides
// ─────────────────────────────────────────────────────────────────────────────

/// Test-only provider whose rates are injected at construction time.
///
/// Using `MockProvider` (rather than `StaticRateProvider`) in tests guarantees
/// that two providers with *different* `coverage_bps`/`premium_bps` constructor
/// arguments will produce *different* quotes for the same loan amount — proving
/// the adapter boundary is exercised rather than both paths falling through to
/// the same static formula.
#[cfg(any(test, feature = "testutils"))]
pub struct MockProvider {
    /// Coverage fraction override in bps (10 000 = 100 %).
    pub coverage_bps: u32,
    /// Premium fraction override in bps.
    pub premium_bps: u32,
}

#[cfg(any(test, feature = "testutils"))]
impl QuoteProvider for MockProvider {
    fn compute_quote(&self, _product: &InsuranceProduct, loan_amount: i128) -> (i128, i128) {
        let coverage = loan_amount * (self.coverage_bps as i128) / 10_000;
        let premium = loan_amount * (self.premium_bps as i128) / 10_000;
        (coverage, premium)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Increment and return the next value for a u64 counter stored in instance storage.
fn next_id(env: &Env, key: &DataKey) -> u64 {
    let next = env
        .storage()
        .instance()
        .get::<DataKey, u64>(key)
        .unwrap_or(0)
        .checked_add(1)
        .expect("ID counter overflow");
    env.storage().instance().set(key, &next);
    next
}

/// Load a provider or return `ProviderNotFound`.
pub fn load_provider(env: &Env, provider_id: u64) -> Result<InsuranceProvider, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::InsuranceProvider(provider_id))
        .ok_or(ContractError::ProviderNotFound)
}

/// Load a product or return `ProductNotFound`.
pub fn load_product(env: &Env, product_id: u64) -> Result<InsuranceProduct, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::InsuranceProduct(product_id))
        .ok_or(ContractError::ProductNotFound)
}

/// Load a quote or return `QuoteNotFound`.
pub fn load_quote(env: &Env, quote_id: u64) -> Result<InsuranceQuote, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::InsuranceQuote(quote_id))
        .ok_or(ContractError::QuoteNotFound)
}

/// Load a claim or return `ClaimNotFound`.
pub fn load_claim(env: &Env, claim_id: u64) -> Result<InsuranceClaim, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::InsuranceClaim(claim_id))
        .ok_or(ContractError::ClaimNotFound)
}

/// Dispatch to the correct `QuoteProvider` implementation based on the
/// provider's `adapter_tag`.
///
/// Currently two tags are supported:
/// - `b"static"` → `StaticRateProvider` (explicit fallback)
///
/// Any unrecognised tag **also** falls back to `StaticRateProvider` and emits
/// a `ins_warn / fallback` event so operators can observe the downgrade.
/// When a real off-chain adapter is integrated, add a new arm here.
fn dispatch_quote_provider(
    env: &Env,
    provider: &InsuranceProvider,
    product: &InsuranceProduct,
    loan_amount: i128,
) -> (i128, i128) {
    let static_tag = Bytes::from_slice(env, b"static");
    if provider.adapter_tag == static_tag {
        StaticRateProvider.compute_quote(product, loan_amount)
    } else {
        // Unrecognised adapter tag: degrade gracefully to StaticRateProvider.
        // Emit a warning event so monitoring can detect the gap.
        env.events().publish(
            (symbol_short!("ins_warn"), symbol_short!("fallback")),
            provider.id,
        );
        StaticRateProvider.compute_quote(product, loan_amount)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Public marketplace functions
// ─────────────────────────────────────────────────────────────────────────────

/// Register a new insurance provider.
///
/// `adapter_tag` selects the quote-computation strategy.  Pass `b"static"` to
/// use `StaticRateProvider`.  Any other value is reserved for future
/// off-chain adapters.
///
/// Returns the new provider's ID.
pub fn register_provider(
    env: Env,
    name: SorobanString,
    adapter_tag: Bytes,
) -> Result<u64, ContractError> {
    let id = next_id(&env, &DataKey::InsuranceProviderCounter);

    let provider = InsuranceProvider {
        id,
        name,
        adapter_tag,
        active: true,
        registered_at: env.ledger().timestamp(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::InsuranceProvider(id), &provider);

    env.events()
        .publish((symbol_short!("ins_prov"), symbol_short!("added")), id);

    Ok(id)
}

/// Add a coverage product under an existing, active provider.
///
/// Returns the new product's ID.
pub fn add_product(
    env: Env,
    provider_id: u64,
    name: SorobanString,
    coverage_pct_bps: u32,
    premium_bps: u32,
) -> Result<u64, ContractError> {
    let provider = load_provider(&env, provider_id)?;
    if !provider.active {
        return Err(ContractError::ProviderInactive);
    }

    assert!(
        coverage_pct_bps > 0 && coverage_pct_bps <= 10_000,
        "coverage_pct_bps must be 1-10000"
    );
    assert!(premium_bps > 0, "premium_bps must be > 0");

    let id = next_id(&env, &DataKey::InsuranceProductCounter);

    let product = InsuranceProduct {
        id,
        provider_id,
        name,
        coverage_pct_bps,
        premium_bps,
        active: true,
    };

    env.storage()
        .persistent()
        .set(&DataKey::InsuranceProduct(id), &product);

    env.events()
        .publish((symbol_short!("ins_prod"), symbol_short!("added")), id);

    Ok(id)
}

/// Compute and persist a new quote for `borrower` against `product_id`.
///
/// The quote's `(coverage_amount, premium_amount)` are computed by the
/// `QuoteProvider` implementation selected by the product's provider's
/// `adapter_tag` — see `dispatch_quote_provider`.
///
/// Quotes are stored in persistent storage so they are visible to every
/// validator node sharing the same ledger state.
///
/// Returns the new quote's ID.
pub fn fetch_quote(
    env: Env,
    product_id: u64,
    borrower: soroban_sdk::Address,
    loan_amount: i128,
) -> Result<u64, ContractError> {
    borrower.require_auth();

    assert!(loan_amount > 0, "loan_amount must be positive");

    let product = load_product(&env, product_id)?;
    if !product.active {
        return Err(ContractError::ProviderInactive);
    }

    let provider = load_provider(&env, product.provider_id)?;
    if !provider.active {
        return Err(ContractError::ProviderInactive);
    }

    let (coverage_amount, premium_amount) =
        dispatch_quote_provider(&env, &provider, &product, loan_amount);

    let id = next_id(&env, &DataKey::InsuranceQuoteCounter);

    let quote = InsuranceQuote {
        id,
        product_id,
        provider_id: product.provider_id,
        borrower,
        loan_amount,
        coverage_amount,
        premium_amount,
        accepted: false,
        issued_at: env.ledger().timestamp(),
    };

    env.storage()
        .persistent()
        .set(&DataKey::InsuranceQuote(id), &quote);

    env.events()
        .publish((symbol_short!("ins_quot"), symbol_short!("issued")), id);

    Ok(id)
}

/// Mark a quote as accepted (premium paid off-chain or via a separate
/// token transfer).  In a full implementation the contract would pull the
/// premium from the borrower here; the hook is left for the integration layer.
///
/// Returns `QuoteAlreadyAccepted` if already active.
pub fn accept_quote(env: Env, quote_id: u64) -> Result<(), ContractError> {
    let mut quote = load_quote(&env, quote_id)?;
    if quote.accepted {
        return Err(ContractError::QuoteAlreadyAccepted);
    }
    quote.accepted = true;
    env.storage()
        .persistent()
        .set(&DataKey::InsuranceQuote(quote_id), &quote);

    env.events()
        .publish((symbol_short!("ins_quot"), symbol_short!("accepted")), quote_id);

    Ok(())
}

/// File a claim against an accepted quote.
///
/// The quote must be accepted and must not already have a claim filed.
/// Returns the new claim's ID.
pub fn file_claim(env: Env, quote_id: u64) -> Result<u64, ContractError> {
    let quote = load_quote(&env, quote_id)?;
    if !quote.accepted {
        return Err(ContractError::QuoteNotAccepted);
    }

    // Guard: only one active claim per quote.
    let claim_counter: u64 = env
        .storage()
        .instance()
        .get(&DataKey::InsuranceClaimCounter)
        .unwrap_or(0);
    for cid in 1..=claim_counter {
        if let Some(existing) = env
            .storage()
            .persistent()
            .get::<DataKey, InsuranceClaim>(&DataKey::InsuranceClaim(cid))
        {
            if existing.quote_id == quote_id
                && !matches!(existing.status, ClaimStatus::Rejected)
            {
                return Err(ContractError::ClaimAlreadyFiled);
            }
        }
    }

    let id = next_id(&env, &DataKey::InsuranceClaimCounter);

    let claim = InsuranceClaim {
        id,
        quote_id,
        borrower: quote.borrower,
        status: ClaimStatus::Pending,
        payout_amount: quote.coverage_amount,
        filed_at: env.ledger().timestamp(),
        resolved_at: None,
    };

    env.storage()
        .persistent()
        .set(&DataKey::InsuranceClaim(id), &claim);

    env.events()
        .publish((symbol_short!("ins_clm"), symbol_short!("filed")), id);

    Ok(id)
}

/// Approve a pending claim.  Admin-only; the caller must pass valid admin signers.
pub fn approve_claim(env: Env, admin_signers: soroban_sdk::Vec<soroban_sdk::Address>, claim_id: u64) -> Result<(), ContractError> {
    crate::helpers::require_admin_approval(&env, &admin_signers);
    let mut claim = load_claim(&env, claim_id)?;
    if !matches!(claim.status, ClaimStatus::Pending) {
        return Err(ContractError::InvalidClaimStatus);
    }
    claim.status = ClaimStatus::Approved;
    claim.resolved_at = Some(env.ledger().timestamp());
    env.storage()
        .persistent()
        .set(&DataKey::InsuranceClaim(claim_id), &claim);

    env.events()
        .publish((symbol_short!("ins_clm"), symbol_short!("approved")), claim_id);

    Ok(())
}

/// Reject a pending claim.
pub fn reject_claim(env: Env, admin_signers: soroban_sdk::Vec<soroban_sdk::Address>, claim_id: u64) -> Result<(), ContractError> {
    crate::helpers::require_admin_approval(&env, &admin_signers);
    let mut claim = load_claim(&env, claim_id)?;
    if !matches!(claim.status, ClaimStatus::Pending) {
        return Err(ContractError::InvalidClaimStatus);
    }
    claim.status = ClaimStatus::Rejected;
    claim.resolved_at = Some(env.ledger().timestamp());
    env.storage()
        .persistent()
        .set(&DataKey::InsuranceClaim(claim_id), &claim);

    env.events()
        .publish((symbol_short!("ins_clm"), symbol_short!("rejected")), claim_id);

    Ok(())
}

/// Pay out an approved claim via the protocol token.
///
/// Transfers `claim.payout_amount` from the contract's reserve to the
/// borrower's address.  Marks the claim as `Paid`.
pub fn pay_claim(env: Env, admin_signers: soroban_sdk::Vec<soroban_sdk::Address>, claim_id: u64) -> Result<(), ContractError> {
    crate::helpers::require_admin_approval(&env, &admin_signers);
    let mut claim = load_claim(&env, claim_id)?;
    if !matches!(claim.status, ClaimStatus::Approved) {
        return Err(ContractError::InvalidClaimStatus);
    }

    // Look up the token from the quote's provider product for correct denomination.
    // We use the protocol token (Config.token) for all payouts.
    let cfg: crate::types::Config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .expect("contract not initialized");

    let token = soroban_sdk::token::Client::new(&env, &cfg.token);
    token.transfer(
        &env.current_contract_address(),
        &claim.borrower,
        &claim.payout_amount,
    );

    claim.status = ClaimStatus::Paid;
    claim.resolved_at = Some(env.ledger().timestamp());
    env.storage()
        .persistent()
        .set(&DataKey::InsuranceClaim(claim_id), &claim);

    env.events().publish(
        (symbol_short!("ins_clm"), symbol_short!("paid")),
        (claim_id, claim.payout_amount),
    );

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Read-only views
// ─────────────────────────────────────────────────────────────────────────────

pub fn get_provider(env: Env, provider_id: u64) -> Option<InsuranceProvider> {
    env.storage()
        .persistent()
        .get(&DataKey::InsuranceProvider(provider_id))
}

pub fn get_product(env: Env, product_id: u64) -> Option<InsuranceProduct> {
    env.storage()
        .persistent()
        .get(&DataKey::InsuranceProduct(product_id))
}

pub fn get_quote(env: Env, quote_id: u64) -> Option<InsuranceQuote> {
    env.storage()
        .persistent()
        .get(&DataKey::InsuranceQuote(quote_id))
}

pub fn get_claim(env: Env, claim_id: u64) -> Option<InsuranceClaim> {
    env.storage()
        .persistent()
        .get(&DataKey::InsuranceClaim(claim_id))
}
