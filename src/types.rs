#![allow(unused)]

use soroban_sdk::{contracttype, Address, Vec};

// ── Constants ─────────────────────────────────────────────────────────────────

pub const DEFAULT_YIELD_BPS: i128 = 200;
pub const DEFAULT_SLASH_BPS: i128 = 5000;
pub const DEFAULT_MIN_YIELD_STAKE: i128 = 50;
pub const DEFAULT_REFERRAL_BONUS_BPS: u32 = 100; // 1% of loan amount
pub const MIN_VOUCH_AGE: u64 = 60; // 1 minute
pub const DEFAULT_MAX_VOUCHERS: u32 = 100;
pub const DEFAULT_MIN_LOAN_AMOUNT: i128 = 100_000;
pub const DEFAULT_LOAN_DURATION: u64 = 30 * 24 * 60 * 60;
pub const DEFAULT_MAX_LOAN_TO_STAKE_RATIO: u32 = 150;
pub const DEFAULT_VOUCH_COOLDOWN_SECS: u64 = 24 * 60 * 60; // 24 hours
pub const TIMELOCK_DELAY: u64 = 24 * 60 * 60;
pub const TIMELOCK_EXPIRY: u64 = 72 * 60 * 60;

// ── Loan Status ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoanStatus {
    None,
    Active,
    Repaid,
    Defaulted,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
pub enum DataKey {
    Loan(u64),                   // loan_id → LoanRecord
    ActiveLoan(Address),         // borrower → active loan_id
    LatestLoan(Address),         // borrower → latest loan_id
    Vouches(Address),            // borrower → Vec<VouchRecord>
    VoucherHistory(Address),     // voucher → Vec<Address> (borrowers backed)
    Config,                      // Config struct: all configurable protocol parameters
    Deployer,                    // Address that deployed the contract; guards initialize
    SlashTreasury,               // i128 accumulated slashed funds
    Paused,                      // bool: true when contract is paused
    BorrowerList,                // Vec<Address> of all borrowers who have ever requested a loan
    ReputationNft,               // Address of the ReputationNftContract
    MinStake,                    // i128 minimum stake amount per vouch
    MaxLoanAmount,               // i128 maximum individual loan size (0 = no cap)
    MinVouchers,     // u32 minimum number of distinct vouchers required (0 = no minimum)
    LoanCounter,     // u64: monotonically increasing loan ID counter
    LoanPool(u64),   // pool_id → LoanPoolRecord
    LoanPoolCounter, // u64: monotonically increasing pool ID counter
    PendingAdmin,    // Address of the pending admin (two-step transfer)
    RepaymentCount(Address), // borrower → u32 total successful repayments
    LoanCount(Address), // borrower → u32 total historical loans disbursed
    DefaultCount(Address), // borrower → u32 total defaults (slash + auto_slash + claim_expired)
    ProtocolFeeBps,  // u32: protocol fee in basis points
    FeeTreasury,     // Address: recipient of collected protocol fees
    LastVouchTimestamp(Address), // voucher → u64 last vouch timestamp
    Timelock(u64),   // proposal_id → TimelockProposal
    TimelockCounter, // u64 monotonically increasing proposal ID
    Blacklisted(Address), // borrower → bool permanently banned
    VoucherWhitelist(Address), // voucher → bool allowed to vouch
    ExtensionConsents(Address), // borrower → Vec<Address> vouchers who consented to extension
    SlashVote(Address),         // borrower → SlashVoteRecord
    SlashVoteQuorum,            // u32 quorum in basis points (e.g. 5000 = 50%)
    ReferredBy(Address),        // borrower → Address of referrer
    ReferralBonusBps,           // u32 referral bonus in basis points (default 100 = 1%)

    // ── Insurance Marketplace ─────────────────────────────────────────────────
    InsuranceProvider(u64),         // provider_id → InsuranceProvider
    InsuranceProviderCounter,       // u64 monotonically increasing provider ID
    InsuranceProduct(u64),          // product_id → InsuranceProduct
    InsuranceProductCounter,        // u64 monotonically increasing product ID
    InsuranceQuote(u64),            // quote_id → InsuranceQuote
    InsuranceQuoteCounter,          // u64 monotonically increasing quote ID
    InsuranceClaim(u64),            // claim_id → InsuranceClaim
    InsuranceClaimCounter,          // u64 monotonically increasing claim ID
}

// ── Governance ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct SlashVoteRecord {
    pub approve_stake: i128,    // total stake voting to approve slash
    pub reject_stake: i128,     // total stake voting to reject slash
    pub voters: Vec<Address>,   // addresses that have already voted
    pub executed: bool,         // true once slash has been auto-executed
}

// ── Config ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub admins: Vec<Address>,
    pub admin_threshold: u32,
    pub token: Address,
    pub allowed_tokens: Vec<Address>, // additional tokens accepted for loans/vouches
    pub yield_bps: i128,
    pub slash_bps: i128,
    pub max_vouchers: u32,
    pub min_loan_amount: i128,
    pub loan_duration: u64,
    pub max_loan_to_stake_ratio: u32,
    pub grace_period: u64,
}

// ── Data Types ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct LoanRecord {
    pub id: u64,
    pub borrower: Address,
    pub co_borrowers: Vec<Address>,
    pub amount: i128,        // total loan principal in stroops
    pub amount_repaid: i128, // cumulative repayments received so far (principal + yield)
    pub total_yield: i128,   // yield owed to vouchers, locked in at disbursement
    pub repaid: bool,
    pub defaulted: bool,
    pub created_at: u64,                  // ledger timestamp
    pub disbursement_timestamp: u64,      // ledger timestamp
    pub repayment_timestamp: Option<u64>, // set once the loan is fully repaid
    pub deadline: u64,                    // repayment deadline (ledger timestamp)
    pub loan_purpose: soroban_sdk::String, // borrower-supplied purpose string
    pub token_address: Address,           // token used for this loan
}

#[contracttype]
#[derive(Clone)]
pub struct VouchRecord {
    pub voucher: Address,
    pub stake: i128,          // in stroops
    pub vouch_timestamp: u64, // ledger timestamp when vouch was created; immutable after set
    pub token: Address,       // token this stake is denominated in
}

#[contracttype]
#[derive(Clone)]
pub struct LoanPoolRecord {
    pub pool_id: u64,
    pub borrowers: Vec<Address>,
    pub amounts: Vec<i128>,
    pub created_at: u64,
    pub total_disbursed: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct TimelockProposal {
    pub id: u64,
    pub action: TimelockAction,
    pub proposer: Address,
    pub eta: u64,
    pub executed: bool,
    pub cancelled: bool,
}

#[contracttype]
#[derive(Clone)]
pub enum TimelockAction {
    Slash(Address),
    SetConfig(Config),
}

// ── Insurance Marketplace ─────────────────────────────────────────────────────

/// Identifies a registered insurance provider.
///
/// `adapter_tag` is a short label (e.g. `b"static"`, `b"mock_a"`) that the
/// on-chain router uses to select the `QuoteProvider` implementation at
/// quote-fetch time.  In production this would be the handle of a real
/// off-chain adapter; for the bundled fallback it is `b"static"`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceProvider {
    pub id: u64,
    pub name: soroban_sdk::String,
    /// Short tag selecting the quote-computation strategy.
    /// `b"static"` → `StaticRateProvider` fallback (arithmetic only).
    /// Any other value → treated as an off-chain adapter key (future work).
    pub adapter_tag: soroban_sdk::Bytes,
    pub active: bool,
    pub registered_at: u64,
}

/// A coverage product offered by a specific provider.
///
/// `coverage_pct_bps` is the fraction of an insured loan amount that is paid
/// out on a successful claim, expressed in basis points (10 000 = 100 %).
///
/// `premium_bps` is the annual premium charged to the borrower, also in bps.
/// `StaticRateProvider` computes `premium = loan_amount * premium_bps / 10_000`.
/// A real adapter would override this with its own pricing model.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceProduct {
    pub id: u64,
    pub provider_id: u64,
    pub name: soroban_sdk::String,
    /// Coverage fraction in basis points (10 000 = 100 %).
    pub coverage_pct_bps: u32,
    /// Annual premium fraction in basis points.
    pub premium_bps: u32,
    pub active: bool,
}

/// A concrete quote issued to a borrower for a specific product.
///
/// Quotes are immutable after issuance and are stored in persistent storage so
/// they are visible to every contract instance behind the load-balancer.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceQuote {
    pub id: u64,
    pub product_id: u64,
    pub provider_id: u64,
    pub borrower: soroban_sdk::Address,
    /// The loan principal this quote covers (in stroops).
    pub loan_amount: i128,
    /// Calculated coverage payout ceiling (in stroops).
    pub coverage_amount: i128,
    /// Calculated one-time premium due (in stroops).
    pub premium_amount: i128,
    /// Whether the borrower has paid the premium and activated this policy.
    pub accepted: bool,
    pub issued_at: u64,
}

/// Status of an insurance claim.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClaimStatus {
    Pending,
    Approved,
    Rejected,
    Paid,
}

/// A filed insurance claim against an accepted quote.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceClaim {
    pub id: u64,
    pub quote_id: u64,
    pub borrower: soroban_sdk::Address,
    pub status: ClaimStatus,
    pub payout_amount: i128,
    pub filed_at: u64,
    pub resolved_at: Option<u64>,
}
