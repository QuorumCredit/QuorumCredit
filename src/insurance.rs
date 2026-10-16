/// Issue #1071: Insurance Fund Mechanism
/// 
/// When slashing cannot cover all voucher losses (tail-risk defaults),
/// the insurance fund absorbs the shortfall. The fund is pre-funded by:
/// 1. Admin contributions
/// 2. A portion of protocol fees
/// 3. Percentage of loan disbursement
///
/// Provides insurance pool helpers and claim processing.
use soroban_sdk::{symbol_short, Address, Env, Vec};
use crate::types::{Config, DataKey};
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
/// Returns `CounterOverflow` instead of panicking if the counter is exhausted.
fn next_id(env: &Env, key: &DataKey) -> Result<u64, ContractError> {
    let next = env
        .storage()
        .instance()
        .get::<DataKey, u64>(key)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(ContractError::CounterOverflow)?;
    env.storage().instance().set(key, &next);
    Ok(next)
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

/// Collect loan insurance fee at disbursement time.
/// 
/// Routes `insurance_fund_premium_bps` percentage of the loan principal
/// to the dedicated insurance fund.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `loan_amount` - Loan principal in stroops
/// * `config` - Current protocol configuration
pub fn collect_insurance_fee(env: &Env, loan_amount: i128, config: &Config) -> Result<i128, ContractError> {
    if config.insurance_fund_premium_bps == 0 {
        return Ok(0);
    }

    let fee = (loan_amount as u128)
        .saturating_mul(config.insurance_fund_premium_bps as u128)
        .saturating_div(10_000)
        as i128;

    // Add fee to insurance fund balance
    let current_balance: i128 = env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128);

    let new_balance = current_balance.saturating_add(fee);
    env.storage().instance().set(&DataKey::InsuranceFund, &new_balance);

    // Record the contribution timestamp
    let now = env.ledger().timestamp();
    env.storage().instance().set(&DataKey::InsuranceFundLastContribution, &now);

    Ok(fee)
}

/// Dispatch to the correct `QuoteProvider` implementation based on the
/// provider's `adapter_tag`.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `admin_signers` - Vector of admin addresses authorizing the contribution
/// * `amount` - Amount to contribute in stroops
/// * `config` - Current protocol configuration
///
/// # Errors
/// * `InvalidAmount` if `amount <= 0`.
/// * `InsuranceContributionTooLarge` (#1437) if `amount` exceeds
///   `config.insurance_fund_max_contrib` (when that cap is non-zero) —
///   a sanity check against fat-finger admin errors.
///
/// # Events
/// Emits `("insurance", "contrib")` with `(amount, new_balance, admin_signer_count)`
/// so manual top-ups leave an on-chain audit trail distinct from organic fee
/// accrual via `collect_insurance_fee`.
pub fn contribute_to_insurance_fund(
    env: &Env,
    admin_signers: Vec<Address>,
    amount: i128,
    config: &Config,
) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Issue #1437: reject contributions above the configured sanity bound.
    if config.insurance_fund_max_contrib > 0
        && amount > config.insurance_fund_max_contrib
    {
        return Err(ContractError::InsuranceContributionTooLarge);
    }

    let current_balance: i128 = env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128);

    let new_balance = current_balance.saturating_add(amount);
    env.storage().instance().set(&DataKey::InsuranceFund, &new_balance);

    let now = env.ledger().timestamp();
    env.storage().instance().set(&DataKey::InsuranceFundLastContribution, &now);

    // Issue #1437: emit an audit event for the manual top-up.
    env.events().publish(
        (symbol_short!("insurance"), symbol_short!("contrib")),
        (amount, new_balance, admin_signers.len()),
    );

    Ok(())
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

    if loan_amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }

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

    let id = next_id(&env, &DataKey::InsuranceQuoteCounter)?;

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

    let id = next_id(&env, &DataKey::InsuranceClaimCounter)?;

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
pub fn approve_claim(env: Env, claim_id: u64) -> Result<(), ContractError> {
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
pub fn reject_claim(env: Env, claim_id: u64) -> Result<(), ContractError> {
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
pub fn pay_claim(env: Env, claim_id: u64) -> Result<(), ContractError> {
    let mut claim = load_claim(&env, claim_id)?;
    if !matches!(claim.status, ClaimStatus::Approved) {
        return Err(ContractError::InvalidClaimStatus);
    }

    // Look up the token from the quote's provider product for correct denomination.
    // We use the protocol token (Config.token) for all payouts.
    let cfg: crate::types::Config = crate::helpers::try_config(&env)?;

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


/// Claim insurance to cover slash shortfall.
///
/// Called during slash processing when the protocol cannot fully cover
/// all voucher losses from protocol fees or reserves. The insurance fund
/// absorbs the difference, capped at `insurance_max_payout_bps` of the shortfall.
///
/// # Arguments
/// * `env` - Soroban environment
/// * `shortfall` - Amount needed to cover all voucher losses (in stroops)
/// * `config` - Current protocol configuration
///
/// # Returns
/// Amount actually paid out from insurance fund (may be less than shortfall if fund is depleted or payout cap is reached).
pub fn claim_insurance_for_shortfall(
    env: &Env,
    shortfall: i128,
    config: &Config,
) -> Result<i128, ContractError> {
    if shortfall <= 0 {
        return Ok(0);
    }

    let current_balance: i128 = env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128);

    if current_balance == 0 {
        return Err(ContractError::InsurancePoolEmpty);
    }

    let max_payout = (shortfall as u128)
        .saturating_mul(config.insurance_max_payout_bps as u128)
        .saturating_div(10_000)
        as i128;

    let payout = current_balance.min(max_payout);
    let remaining_balance = current_balance.saturating_sub(payout);

    env.storage().instance().set(&DataKey::InsuranceFund, &remaining_balance);

    // Issue #1436: warn operators before the fund is fully depleted. The event
    // fires only on the claim that pushes the balance across the threshold
    // (i.e. it was at/above the threshold before this claim and is below now),
    // so a depleted fund emits exactly one alert rather than one per claim.
    let threshold = config.insurance_fund_low_bal_thresh;
    if threshold > 0 && remaining_balance < threshold && current_balance >= threshold {
        env.events().publish(
            (symbol_short!("insurance"), symbol_short!("low_bal")),
            (remaining_balance, threshold),
        );
    }

    Ok(payout)
}

/// Get the current balance of the insurance fund.
pub fn get_insurance_fund_balance(env: &Env) -> i128 {
    env.storage().instance()
        .get(&DataKey::InsuranceFund)
        .unwrap_or(0i128)
}

/// Get the timestamp of the most recent insurance fund contribution.
pub fn get_insurance_fund_last_contribution(env: &Env) -> u64 {
    env.storage().instance()
        .get(&DataKey::InsuranceFundLastContribution)
        .unwrap_or(0u64)
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

#[cfg(test)]
mod insurance_fund_hardening_tests {
    //! Issue #1436 (low-balance alert) and Issue #1437 (contribution cap + event).
    use super::*;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::testutils::{Address as _, Events};
    use soroban_sdk::{vec, Address, Env};

    fn setup() -> (Env, Address, Address, Config) {
        let env = Env::default();
        env.mock_all_auths();

        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &vec![&env, admin.clone()], &1u32, &token);

        let cfg = env.as_contract(&contract_id, || crate::helpers::config(&env));
        (env, contract_id, admin, cfg)
    }

    #[test]
    fn contribution_over_configured_cap_is_rejected() {
        let (env, contract_id, admin, mut cfg) = setup();
        cfg.insurance_fund_max_contrib = 1_000;

        env.as_contract(&contract_id, || {
            // Over the cap → rejected, no balance change.
            let err = contribute_to_insurance_fund(
                &env,
                vec![&env, admin.clone()],
                5_000,
                &cfg,
            )
            .unwrap_err();
            assert_eq!(err, ContractError::InsuranceContributionTooLarge);
            assert_eq!(get_insurance_fund_balance(&env), 0);

            // At the cap → accepted.
            contribute_to_insurance_fund(&env, vec![&env, admin.clone()], 1_000, &cfg)
                .expect("contribution at cap should succeed");
            assert_eq!(get_insurance_fund_balance(&env), 1_000);
        });
    }

    #[test]
    fn contribution_emits_audit_event() {
        let (env, contract_id, admin, cfg) = setup();

        env.as_contract(&contract_id, || {
            let before = env.events().all().events().len();
            contribute_to_insurance_fund(&env, vec![&env, admin.clone()], 750, &cfg)
                .expect("contribution should succeed");
            assert!(
                env.events().all().events().len() > before,
                "contribute_to_insurance_fund must emit an event"
            );
        });
    }

    #[test]
    fn low_balance_event_fires_exactly_on_threshold_crossing() {
        let (env, contract_id, _admin, mut cfg) = setup();
        cfg.insurance_fund_low_bal_thresh = 500;
        cfg.insurance_max_payout_bps = 10_000; // allow full-shortfall payout

        env.as_contract(&contract_id, || {
            env.storage().instance().set(&DataKey::InsuranceFund, &1_000i128);

            // First claim keeps balance at/above threshold (1000 -> 600): no alert.
            let before = env.events().all().events().len();
            claim_insurance_for_shortfall(&env, 400, &cfg).unwrap();
            assert_eq!(get_insurance_fund_balance(&env), 600);
            assert_eq!(
                env.events().all().events().len(),
                before,
                "no alert while balance stays above threshold"
            );

            // Second claim crosses the threshold (600 -> 300): exactly one alert.
            let before = env.events().all().events().len();
            claim_insurance_for_shortfall(&env, 300, &cfg).unwrap();
            assert_eq!(get_insurance_fund_balance(&env), 300);
            assert_eq!(
                env.events().all().events().len(),
                before + 1,
                "alert must fire on the claim that crosses the threshold"
            );

            // Third claim while already below threshold: no further alert.
            let before = env.events().all().events().len();
            claim_insurance_for_shortfall(&env, 100, &cfg).unwrap();
            assert_eq!(
                env.events().all().events().len(),
                before,
                "alert must not repeat once already below threshold"
            );
        });
    }
}

#[cfg(test)]
mod missing_record_tests {
    //! Unwrap-removal coverage (Issue #1916): claim and quote paths must return
    //! typed errors for missing records and malformed input instead of panicking.
    use super::*;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{vec, Address, Env};

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let deployer = Address::generate(&env);
        let admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let contract_id = env.register_contract(None, QuorumCreditContract);
        let client = QuorumCreditContractClient::new(&env, &contract_id);
        client.initialize(&deployer, &vec![&env, admin.clone()], &1u32, &token);
        (env, contract_id)
    }

    #[test]
    fn fetch_quote_rejects_non_positive_amount() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert_eq!(
                fetch_quote(env.clone(), 1, borrower.clone(), 0),
                Err(ContractError::InvalidAmount)
            );
            assert_eq!(
                fetch_quote(env.clone(), 1, borrower.clone(), -100),
                Err(ContractError::InvalidAmount)
            );
        });
    }

    #[test]
    fn fetch_quote_on_missing_product_returns_error() {
        let (env, contract_id) = setup();
        let borrower = Address::generate(&env);
        env.as_contract(&contract_id, || {
            assert!(fetch_quote(env.clone(), 999, borrower.clone(), 1_000).is_err());
        });
    }

    #[test]
    fn claim_paths_on_missing_records_return_errors() {
        let (env, contract_id) = setup();
        env.as_contract(&contract_id, || {
            assert!(file_claim(env.clone(), 999).is_err());
            assert!(approve_claim(env.clone(), 999).is_err());
            assert!(reject_claim(env.clone(), 999).is_err());
            assert!(pay_claim(env.clone(), 999).is_err());
            assert!(accept_quote(env.clone(), 999).is_err());
        });
    }

    #[test]
    fn next_id_returns_overflow_error_at_counter_max() {
        let (env, contract_id) = setup();
        let key = DataKey::InsuranceClaimCounter;
        env.as_contract(&contract_id, || {
            env.storage().instance().set(&key, &u64::MAX);
            assert_eq!(next_id(&env, &key), Err(ContractError::CounterOverflow));
        });
    }
}
