#![allow(unused)]

//! # Stroop Unit Convention
//!
//! **All monetary amounts in this contract are denominated in stroops.**
//!
//! | Unit  | Value                      |
//! |-------|----------------------------|
//! | 1 XLM | 10,000,000 stroops         |
//! | 1 stroop | 0.0000001 XLM           |
//!
//! This applies to every `i128` field or parameter that represents a token
//! amount (stakes, loan principals, yield, fees, minimums, etc.).
//! When displaying values to end-users, divide by `10_000_000` to convert
//! to XLM. When accepting user input in XLM, multiply by `10_000_000`
//! before passing to contract functions.

use soroban_sdk::{contracttype, Address, BytesN, String, Vec};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Yield earned by vouchers on full repayment, in basis points (200 = 2%).
pub const DEFAULT_YIELD_BPS: i128 = 200;
/// Fraction of stake burned when a borrower defaults, in basis points (5000 = 50%).
pub const DEFAULT_SLASH_BPS: i128 = 5000;
/// Basis-point denominator (10_000 = 100%).
pub const BPS_DENOMINATOR: i128 = 10_000;
/// Minimum stake amount, in stroops (50 stroops), required for non-zero yield at
/// the default 2% rate. Amounts below this truncate to zero yield.
/// 1 XLM = 10,000,000 stroops.
pub const DEFAULT_MIN_YIELD_STAKE: i128 = 50;
/// Referral bonus paid to the referrer on full repayment, in basis points (100 = 1% of loan amount).
pub const DEFAULT_REFERRAL_BONUS_BPS: u32 = 100; // 1% of loan amount
/// Minimum age of a vouch before it can be used for a loan, in seconds (60 = 1 minute).
pub const MIN_VOUCH_AGE: u64 = 60; // 1 minute
/// Default minimum vouch age before loan eligibility, in seconds (24 hours).
pub const DEFAULT_MIN_VOUCH_AGE_SECS: u64 = 24 * 60 * 60;
/// Default maximum number of distinct vouchers per borrower.
pub const DEFAULT_MAX_VOUCHERS: u32 = 100;
/// Default minimum loan amount, in stroops (100,000 stroops = 0.01 XLM).
/// 1 XLM = 10,000,000 stroops.
pub const DEFAULT_MIN_LOAN_AMOUNT: i128 = 100_000;
/// Default loan duration, in seconds (30 days).
pub const DEFAULT_LOAN_DURATION: u64 = 30 * 24 * 60 * 60;
/// Default payment grace period after a suspension, in seconds (3 days).
pub const PAYMENT_GRACE_PERIOD: u64 = 3 * 24 * 60 * 60;
/// Default maximum loan-to-stake ratio (150 = 150% — loan ≤ 1.5× total staked).
pub const DEFAULT_MAX_LOAN_TO_STAKE_RATIO: u32 = 150;
/// Default maximum loan-to-collateral ratio (50_000 = 50% — loan ≤ 0.5× total stake).
pub const DEFAULT_MAX_LOAN_TO_COLLATERAL_RATIO: u32 = 50_000;
/// Minimum elapsed time between vouch calls from the same address, in seconds (24 hours).
pub const DEFAULT_VOUCH_COOLDOWN_SECS: u64 = 24 * 60 * 60; // 24 hours
/// Default maximum number of vouchers that may back a single borrower.
pub const DEFAULT_MAX_VOUCHERS_PER_BORROWER: u32 = 50;
/// Default timelock delay for config changes, in seconds (7 days).
pub const CONFIG_TIMELOCK_SECONDS: u64 = 7 * 24 * 60 * 60;
/// Default governance voting period for slash-threshold proposals, in seconds (7 days).
pub const DEFAULT_VOTING_PERIOD_SECONDS: u64 = 7 * 24 * 60 * 60;
/// Minimum delay before a timelocked governance action may be executed, in seconds (24 hours).
pub const TIMELOCK_DELAY: u64 = 24 * 60 * 60;
/// Maximum window after `eta` within which a timelocked action must be executed, in seconds (72 hours).
pub const TIMELOCK_EXPIRY: u64 = 72 * 60 * 60;
/// Minimum lock period for a vouch before it can be withdrawn, in seconds (7 days).
/// Protects against flash-loan-style attacks where an attacker stakes, borrows, then
/// immediately withdraws.
pub const MIN_VOUCH_LOCK_PERIOD: u64 = 7 * 24 * 60 * 60;

/// Maximum reputation bonus for vouchers, in basis points (100 = 1%).
pub const REPUTATION_BONUS_MAX_BPS: i128 = 100;

/// Duration of slash escrow period before funds are burned or returned, in seconds (7 days).
pub const SLASH_APPEAL_PERIOD: u64 = 7 * 24 * 60 * 60;

/// Quorum required to overturn a slash appeal, in basis points (6667 = 2/3).
pub const APPEAL_OVERRIDE_QUORUM_BPS: u32 = 6_667;

/// Fraction of slashed funds routed to the insurance pool (2000 = 20%).
pub const SLASH_TO_INSURANCE_BPS: u32 = 2_000;
/// Default insurance fee on loan disbursement (50 = 0.5%).
pub const DEFAULT_INSURANCE_FEE_BPS: u32 = 50;
/// Default max insurance payout as % of slashed stake (2500 = 25%).
pub const DEFAULT_INSURANCE_COVERAGE_BPS: u32 = 2_500;

/// Extension fee charged when a borrower requests a loan extension, in basis points (100 = 1%).
pub const EXTENSION_FEE_BPS: i128 = 100;

/// Maximum number of extensions allowed per loan.
pub const MAX_EXTENSIONS_PER_LOAN: u32 = 2;

/// Default liquidity mining reward rate in basis points per epoch (50 = 0.5% per 7 days).
pub const DEFAULT_LIQUIDITY_MINING_RATE_BPS: u32 = 50;

/// Default dynamic slash threshold setting (false = disabled by default).
pub const DEFAULT_DYNAMIC_SLASH_THRESHOLD: bool = false;

/// Default loan-size-based slash scaling setting (false = disabled by default).
pub const DEFAULT_LOAN_SIZE_SLASH_ENABLED: bool = false;

/// Default maximum slash rate for the largest loans, in basis points (8000 = 80%).
/// When loan-size scaling is enabled, slash_bps is the floor (small loans) and
/// this is the ceiling (loans at or above the total staked collateral).
pub const DEFAULT_LOAN_SIZE_SLASH_MAX_BPS: i128 = 8_000;

/// Default borrower repayment confirmation requirement (false = disabled by default).
pub const DEFAULT_CONFIRMATION_REQUIRED: bool = false;

/// Default quorum for voucher-based slash votes, in basis points (6667 ≈ 66.67%).
/// Requires approximately 2/3 of total vouched stake to approve before a slash executes.
pub const DEFAULT_SLASH_VOTE_QUORUM_BPS: u32 = 6_667;

/// Minimum elapsed time between successive slash proposals for the same borrower, in seconds
/// (7 days). Prevents spam proposals and gives borrowers time to resolve issues between rounds.
pub const DEFAULT_SLASH_PROPOSAL_COOLDOWN_SECS: u64 = 7 * 24 * 60 * 60;

/// Default rate limit: 10 calls per window.
pub const DEFAULT_RATE_LIMIT_COUNT: u32 = 10;
/// Default rate limit window: 60 seconds.
pub const DEFAULT_RATE_LIMIT_WINDOW_SECS: u64 = 60;

/// Timelock delay for decrease_stake during an active loan, in seconds (7 days).
pub const DECREASE_STAKE_TIMELOCK: u64 = 7 * 24 * 60 * 60;

/// Withdrawal request timelock delay, in seconds (24 hours).
pub const WITHDRAWAL_TIMELOCK_DELAY: u64 = 24 * 60 * 60;

/// Maximum number of deferment periods allowed per loan.
pub const MAX_DEFERMENT_PERIODS: u32 = 3;

/// Duration of each deferment period, in seconds (30 days).
pub const DEFERMENT_PERIOD_SECS: u64 = 30 * 24 * 60 * 60;

/// Penalty applied to partial mid-loan withdrawals, in basis points (1000 = 10%).
pub const PARTIAL_WITHDRAWAL_PENALTY_BPS: i128 = 1_000;

/// Yield stream period in seconds (7 days).
pub const YIELD_STREAM_PERIOD_SECS: u64 = 7 * 24 * 60 * 60;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitConfig {
    pub window_secs: u64,
    pub max_calls: u32,
    pub enabled: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdminRole {
    SuperAdmin,
    Treasurer,
    Monitor,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdminPermission {
    Slash,
    Pause,
    UpdateConfig,
    ManageFees,
    ReadAnalytics,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionMatrix {
    pub role: AdminRole,
    pub permissions: Vec<AdminPermission>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Role {
    Admin,
    Voucher,
    Borrower,
    Governance,
    Oracle,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolePermissions {
    pub role: Role,
    pub can_vouch: bool,
    pub can_request_loan: bool,
    pub can_repay: bool,
    pub can_slash: bool,
    pub can_gov: bool,
}

/// Maximum fraction of stake that can be partially withdrawn during an active loan (50%).
pub const PARTIAL_WITHDRAWAL_MAX_BPS: i128 = 5_000;

/// Minimum slash threshold when protocol health is excellent, in basis points (2500 = 25%).
pub const MIN_DYNAMIC_SLASH_BPS: i128 = 2_500;

/// Maximum slash threshold when protocol health is poor, in basis points (7500 = 75%).
pub const MAX_DYNAMIC_SLASH_BPS: i128 = 7_500;

/// Health threshold below which slash penalty increases, in basis points (8000 = 80%).
pub const HEALTH_THRESHOLD_BPS: i128 = 8_000;

/// Default slash delay period to allow for disputes, in seconds (7 days).
pub const DEFAULT_SLASH_DELAY_SECONDS: u64 = 7 * 24 * 60 * 60;

/// Duration of one reporting month, in seconds (30 days).
pub const MONTHLY_PERIOD_SECS: u64 = 30 * 24 * 60 * 60;

/// Default premium rate for slashing insurance opt-in, in basis points (100 = 1%).
pub const DEFAULT_INSURANCE_PREMIUM_BPS: u32 = 100;

// ── Loan Extension ────────────────────────────────────────────────────────────

/// A pending loan extension request. Created by the borrower; approved by vouchers.
#[contracttype]
#[derive(Clone)]
pub struct LoanExtensionRequest {
    /// The borrower requesting the extension.
    pub borrower: Address,
    /// Loan ID being extended.
    pub loan_id: u64,
    /// Requested additional duration in seconds.
    pub extension_secs: u64,
    /// Timestamp when the request was created.
    pub requested_at: u64,
    /// Vouchers who have approved this extension.
    pub approvals: Vec<Address>,
    /// Extension fee paid (in stroops), deducted from borrower on approval.
    pub fee_paid: i128,
    /// How many times this loan has already been extended.
    pub extension_count: u32,
}
/// Slash escrow period before funds are permanently burned, in seconds (30 days).
pub const SLASH_ESCROW_PERIOD: u64 = 30 * 24 * 60 * 60;

// ── Escrow Status ─────────────────────────────────────────────────────────────

/// Status of a repayment held in oracle-verified escrow (#666/#667).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    /// No escrow — repayment released immediately (default).
    None,
    /// Repayment held pending oracle verification.
    Pending,
    /// Oracle approved — funds released to vouchers.
    Released,
    /// Oracle rejected — funds returned to borrower.
    Rejected,
}

// ── Loan Status ───────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoanStatus {
    None,
    Active,
    Repaid,
    /// #663: Borrower repaid some but less than partial_default_threshold_bps of total owed.
    PartialDefault,
    Defaulted,
    /// #664: Default was forgiven by admin.
    ForgivenDefault,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoanStatusEx {
    None,
    Active,
    Suspended,
    Repaid,
    PartialDefault,
    Defaulted,
    ForgivenDefault,
}

impl From<LoanStatus> for LoanStatusEx {
    fn from(status: LoanStatus) -> Self {
        match status {
            LoanStatus::None => LoanStatusEx::None,
            LoanStatus::Active => LoanStatusEx::Active,
            LoanStatus::Repaid => LoanStatusEx::Repaid,
            LoanStatus::PartialDefault => LoanStatusEx::PartialDefault,
            LoanStatus::Defaulted => LoanStatusEx::Defaulted,
            LoanStatus::ForgivenDefault => LoanStatusEx::ForgivenDefault,
        }
    }
}

/// Interest rate type for a loan.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RateType {
    /// Fixed rate locked at disbursement (yield_bps from Config).
    Fixed,
    /// Variable rate tied to an external index; recalculated on each repayment.
    Variable,
}

// ── Pause State Machine ───────────────────────────────────────────────────────

/// Contract pause state for the Normal → Paused → Thawing → Normal state machine.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PauseMode {
    /// Contract is operating normally.
    None,
    /// Contract is fully paused — all writes are blocked.
    Paused,
    /// Contract is thawing — only reads and withdrawals are allowed.
    /// Automatically transitions to `None` after `thaw_duration` seconds.
    Thawing,
}

/// Timestamps recorded when the contract enters or exits a thaw period.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThawState {
    /// Ledger timestamp when `pause()` was called.
    pub pause_timestamp: u64,
    /// Duration of the thaw window in seconds (default 24 h = 86_400).
    pub thaw_duration: u64,
    /// Ledger timestamp when `begin_thaw()` was called.
    pub thaw_start_timestamp: u64,
}

/// Duration of the thaw period in seconds (24 hours).
pub const THAW_DURATION_SECS: u64 = 24 * 60 * 60;

// ── Governance Proposal Status ─────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    /// Proposal is under active voting.
    Active,
    /// Proposal has passed and is executable.
    Passed,
    /// Proposal has been rejected.
    Rejected,
    /// Proposal voting period has expired.
    Expired,
    /// Proposal has been executed.
    Executed,
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
    PauseMode,                   // PauseMode enum: None, Paused, or Thawing
    ThawState,                   // ThawState: pause and thaw timestamps
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
    VouchCooldownSecs, // u64 cooldown between vouch calls (default 24 hours)
    Timelock(u64),   // proposal_id → TimelockProposal
    TimelockCounter, // u64 monotonically increasing proposal ID
    Blacklisted(Address), // borrower → bool permanently banned
    VoucherWhitelist(Address), // voucher → bool allowed to vouch
    WhitelistEnabled, // bool: true when voucher whitelist is enabled (opt-in)
    ExtensionConsents(Address), // borrower → Vec<Address> vouchers who consented to extension
    SlashVote(Address), // borrower → SlashVoteRecord
    SlashVoteQuorum, // u32 quorum in basis points (e.g. 5000 = 50%)
    ReferredBy(Address), // borrower → Address of referrer
    ReferralBonusBps, // u32 referral bonus in basis points (default 100 = 1%)
    MaxVouchersPerBorrower, // u32 maximum number of vouchers per borrower (default 50)
    BorrowerCollateral(Address), // borrower → i128 collateral amount deposited
    BorrowerCollateralToken(Address), // borrower → Address token used for collateral
    InsurancePool,           // i128 total funds contributed to the insurance pool
    InsuranceClaim(u64),     // loan_id → bool: has any claim been made (legacy single-claim guard)
    InsuranceFeeBps,         // u32: protocol fee routed to insurance pool per loan (default 50 = 0.5%)
    InsuranceCoverageBps,    // u32: max payout as % of slashed stake (default 2500 = 25%)
    InsuranceVoucherClaim(u64, Address), // (loan_id, voucher) → i128 amount already claimed
    VouchHistory(Address, Address, Address), // (borrower, voucher, token) → Vec<VouchHistoryEntry>
    VouchDelegation(Address, Address, Address), // (borrower, original_voucher, token) → Address (delegate)
    /// Issue #1069: Vote delegation - voucher → delegate address for governance votes
    VoteDelegation(Address),
    PendingSlashExecution(Address), // borrower → PendingSlashRecord
    YieldReserve,            // i128 balance of the yield reserve
    SlashEscrow(Address),    // borrower → (i128 amount, u64 release_timestamp)
    SlashAudit(Address),     // borrower → SlashRecord (latest slash for borrower)
    SlashRecord(u64),        // slash_id → SlashRecord
    SlashRecordCounter,      // u64 monotonic slash ID counter
    BorrowerRegistered(Address), // borrower → registration timestamp
    // Issue #598-601 additions
    PrepaymentPenaltyBps,    // u32: prepayment penalty in basis points
    YieldDistribution(u64),  // loan_id → Vec<YieldDistributionEntry>
    AdminAction(u64),        // action_id → AdminActionProposal
    AdminActionCounter,      // u64: monotonically increasing admin action ID
    SlashAppeal(Address, Address), // (borrower, voucher) → SlashAppealRecord (Issue #552)
    SlashEscrowAppeal(Address), // borrower → SlashAppealRecord (Issue #841: escrow-based appeal)
    /// Slash-threshold governance proposal id → proposal record.
    SlashThresholdProposal(u64),
    SlashThresholdProposalCounter,
    /// Per-borrower timestamp of the last successful slash.
    LastSlashedAt(Address),
    /// Per-borrower timestamp of the most recent slash proposal initiation.
    /// Used to enforce the 7-day cooldown between successive slash proposals.
    LastSlashProposalAt(Address),
    /// Cached total weighted stake per borrower per token: (borrower, token) → i128
    /// Used for O(1) eligibility checks; invalidated on vouch operations.
    TotalWeightedStakeCache(Address, Address),
    /// Archived loan records: archive_id → ArchivedLoanRecord
    /// Old completed or slashed loans are moved here to reduce persistent storage.
    ArchivedLoan(u64),
    /// Archive counter for generating unique archive IDs
    ArchiveCounter,
    /// Archived vouch history: (borrower, voucher, token, batch_id) → Vec<VouchHistoryEntry>
    /// Old vouch history entries are moved here when history grows beyond a threshold.
    ArchivedVouchHistory(Address, Address, Address, u32),
    /// IPFS archive reference for loans: archive_id → IpfsArchiveReference
    /// Maps archive IDs to their IPFS content hashes for off-chain storage.
    IpfsLoanArchive(u64),
    /// IPFS archive reference for vouch history: archive_id → IpfsArchiveReference
    IpfsVouchHistoryArchive(u64),
    /// Counter for IPFS archives created
    IpfsArchiveCounter,
    /// Flag indicating if an archive has been backed up to IPFS: archive_id → bool
    IpfsBackedArchive(u64),
    /// Admin config-update proposal id → proposal record.
    ConfigUpdateProposal(u64),
    ConfigUpdateProposalCounter,
    /// Issue #599/#600: (voucher, borrower) → WithdrawalRequest (pending timelock withdrawal)
    PendingWithdrawal(Address, Address),
    /// Issue #601: borrower → LoanExtensionRequest
    LoanExtension(Address),
    /// Issue #598: loan_id → Vec<PaymentRecord> (payment history)
    PaymentHistory(u64),
    /// Voucher cumulative reputation stats: voucher → VoucherStats
    VoucherStats(Address),
    /// Withdrawal queue: borrower → Vec<QueuedWithdrawal>
    WithdrawalQueue(Address),
    // #634: Liquidity Mining
    LastMiningClaim(Address),
    // #635: Vouch Snapshot for Governance
    VouchSnapshot(u32),
    // #636: Staking Derivatives
    StakingDerivative(Address, Address),
    // #637: Fraud Detection
    VoucherFraudScore(Address),
    /// Issue #637: on-demand fraud detection configuration
    FraudScoreConfig,
    /// Repayment dispute raised by a voucher: (borrower, voucher) -> DisputeRecord
    RepaymentDispute(Address, Address),
    // #667: Oracle address for repayment verification
    OracleAddress,
    // #667: External credit score per borrower
    ExternalCreditScore(Address),
    // #666: Escrowed repayment amount per borrower (held pending oracle verification)
    EscrowAmount(Address),
    /// Monthly slashing transparency report: month_id → SlashingReportRecord.
    /// month_id = unix_timestamp / MONTHLY_PERIOD_SECS
    SlashingReport(u64),
    /// Per-vouch insurance opt-in: (voucher, borrower) → bool (insured).
    VoucherInsurance(Address, Address),
    /// Cross-chain bridge validation status: (voucher, chain_id) → bool.
    BridgeValidated(Address, u32),
    /// Registered cross-chain bridges: Vec<BridgeRecord>
    Bridges,
    /// origin_chain → the Ed25519 public key trusted to sign attestations from it.
    BridgePublicKey(u32),
    /// (origin_chain, nonce) → true once an attestation with that nonce has been consumed.
    BridgeNonceUsed(u32, u64),
    /// (origin_chain, loan_id) → the mirrored CrossChainLoanMetadata for that origin loan.
    MirroredLoan(u32, u64),
    /// borrower → the latest cross-chain reputation snapshot mirrored in for them.
    CrossChainReputation(Address),
    /// Issue #687: admin removal proposal id → AdminRemovalProposal
    AdminRemovalProposal(u64),
    /// Issue #687: monotonically increasing admin removal proposal counter
    AdminRemovalProposalCounter,
    /// Issue #686: accumulated admin compensation pool balance (i128 stroops)
    AdminCompensation,
    /// Issue #686: last compensation claim timestamp per admin address
    AdminLastClaim(Address),
    RolePermissions(Address), // address -> RolePermissions
    RateLimit(Address),        // address -> (u64 last_call_window_start, u32 call_count)
    /// Issue #16: admin address -> AdminRole
    AdminRole(Address),
    /// Issue #742: current semantic contract version
    ContractVersion,
    /// Issue #742: version history entries by index
    ContractVersionHistory(u32),
    /// Issue #742: number of version history entries
    ContractVersionHistoryCount,
    /// Issue #743: deployment record by index
    DeploymentRecord(u32),
    /// Issue #743: total number of deployment records
    DeploymentRecordCount,
    /// Issue #744: rollback snapshot of config keyed by version index
    RollbackSnapshot(u32),
    /// Governance proposal id → GovernanceProposal
    GovernanceProposal(u64),
    /// Governance proposal counter (monotonically increasing)
    GovernanceProposalCounter,
    /// Governance queue configuration
    GovernanceQueueConfig,
    /// Credit score record for a borrower
    CreditScore(Address),
    /// Credit score configuration
    CreditScoreConfig,
    /// Loan syndication record
    LoanSyndication(u64),
    /// Syndication counter (monotonically increasing)
    SyndicationCounter,
    /// Syndication configuration
    SyndicationConfig,
    /// Syndication member index (syndication_id, member_address) → SyndicationMember
    SyndicationMember(u64, Address),
    /// Syndication repayment records
    SyndicationRepayment(u64, u64), // syndication_id, repayment_index
    /// Syndication repayment counter
    SyndicationRepaymentCounter(u64), // syndication_id → counter
    /// Reputation NFT badge for excellent credit tier: borrower → ReputationNFTRecord
    ReputationNFTBadge(Address),
    // ── Issue #863: Vouch Cooldown Bypass ────────────────────────────────────
    /// Per-voucher emergency bypass flag: voucher → bool
    EmergencyCooldownBypass(Address),
    /// Cooldown bypass request: (borrower, voucher) → CooldownBypassRequest
    CooldownBypass(Address, Address),
    // ── Issue #867: Cross-Collateral Vouch Pools ─────────────────────────────
    CollateralPool(u64),
    CollateralPoolCounter,
    BorrowerPool(Address, u64),
    // ── Issue #868: Gradual Unstaking ─────────────────────────────────────────
    GradualUnstake(Address, Address),
    // ── Issue #882: Loan Insurance Integration ───────────────────────────────
    /// loan_id → bool: whether insurance was collected at disbursement
    InsuranceLinked(u64),
    // ── Issue #884: Prepayment Bonus ─────────────────────────────────────────
    /// Configurable prepayment bonus rate in basis points
    PrepaymentBonusBps,
    // ── Issue #885: Loan Status Privacy ──────────────────────────────────────
    /// borrower → LoanPrivacyLevel
    LoanPrivacy(Address),
    // ── Issue #887: Loan Subordination and Cascading Debt Hierarchy ──────────
    /// (senior_loan_id, subordinate_loan_id) → SubordinationRecord
    SubordinationRelation(u64, u64),
    /// senior_loan_id → Vec<u64> (IDs of all subordinate loans ordered by priority)
    SubordinateLoansList(u64),
    /// subordinate_loan_id → u64 (ID of direct senior loan, if any)
    SeniorLoanOf(u64),
    /// senior_loan_id → CascadingDefault (tracks cascade triggered by default)
    CascadingDefaultRecord(u64),
    /// Waterfall distribution configuration for a borrower
    WaterfallConfig(Address),
    // ── Issue #934: Yield Calculation Caching ──────────────────────────────────
    /// (borrower, voucher) → CachedYieldRecord
    YieldCache(Address, Address),
    // ── Cache infrastructure (Issue #724) ──────────────────────────────────────
    /// LRU index counter for cache eviction
    LruIndex,
    /// Oldest loan cache entry ID for LRU eviction
    LruOldestLoanId,
    // ── Reentrancy Guard ──────────────────────────────────────────────────────
    /// bool: true when a state-mutating operation is in progress
    Locked,
    // ── Nonce tracking (Issue #64) ────────────────────────────────────────────
    /// Address → u64: last consumed nonce for replay protection
    Nonce(Address),
    // ── Oracle price (Issue #64) ──────────────────────────────────────────────
    /// Symbol → OraclePriceRecord
    OraclePrice(soroban_sdk::Symbol),
    // ── Graduated threat level (Issue #65) ────────────────────────────────────
    /// ThreatLevel enum value
    ThreatLevelKey,
    // ── Multi-tier admin thresholds (Issue #893) ──────────────────────────────
    /// MultiTierAdminThresholds configuration (stored in instance storage)
    MultiTierAdminThresholds,
    // ── Risk threshold governance (Issue #903) ────────────────────────────────
    /// proposal_id → RiskThresholdProposal
    RiskThresholdProposal(u64),
    /// monotonically increasing risk threshold proposal counter
    RiskThresholdCounter,
    /// (proposal_id, voter) → bool (has voted)
    RiskThresholdVote(u64, Address),
    // ── Fee structure governance (Issue #904) ─────────────────────────────────
    /// proposal_id → FeeStructureProposal
    FeeStructureProposal(u64),
    /// monotonically increasing fee structure proposal counter
    FeeStructureCounter,
    /// (proposal_id, voter) → bool (has voted)
    FeeStructureVote(u64, Address),
    // ── Withdrawal timelock (Issue #905) ──────────────────────────────────────
    /// (borrower, voucher) → withdrawal timelock record
    WithdrawalTimelock(u64),
    /// monotonically increasing withdrawal timelock counter
    WithdrawalTimelockCounter,
    // ── Cross-chain proposal sync (Issue #906) ─────────────────────────────────
    /// proposal_id → CrossChainProposalSync
    CrossChainProposalSync(u64),
    /// monotonically increasing cross-chain proposal sync counter
    CrossChainSyncCounter,
    // ── Yield stream (Issue #907) ──────────────────────────────────────────────
    /// loan_id → YieldStreamState
    YieldStreamState(u64),
    /// (loan_id, voucher) → VoucherYieldClaim
    VoucherYieldClaim(u64, Address),
    // ── Periodic payments (Issue #908) ────────────────────────────────────────
    /// loan_id → PeriodicPaymentConfig
    PeriodicPaymentConfig(u64),
    /// loan_id → PeriodicPaymentStatus
    PeriodicPaymentStatus(u64),
    // ── Vouch groups (Issue #909) ──────────────────────────────────────────────
    /// group_id → VouchGroup
    VouchGroup(u64),
    /// monotonically increasing vouch group counter
    VouchGroupCounter,
    /// voucher → Vec<u64> group IDs the voucher belongs to
    VoucherGroupIds(Address),
    // ── Vouch merkle root (Issue #910) ────────────────────────────────────────
    /// borrower → BytesN<32> merkle root of vouch set
    VouchMerkleRoot(Address),
    // ── Batch transfers (Issue #935) ──────────────────────────────────────────
    /// Vec<BatchTransfer> pending transfer queue
    PendingTransfers,
    // ── Lazy slash queue (Issue #937) ─────────────────────────────────────────
    /// Vec<LazySlashEntry> queued slash operations
    LazySlashQueue,
    // ── Custom attributes ────────────────────────────────────────────────────
    /// Address → Vec<AttributeEntry>
    CustomAttributes(Address),
    // ── Forbearance (Issue #878) ──────────────────────────────────────────────
    /// loan_id → ForbearanceRecord
    Forbearance(u64),
    // ── Refinance record (Issue #877) ─────────────────────────────────────────
    /// loan_id → RefinanceRecord
    RefinanceRecord(u64),
    // ── Dynamic rate (Issue #881) ──────────────────────────────────────────────
    /// DynamicRateConfig (global config)
    DynamicRateConfig,
    /// borrower → BorrowerDynamicRate
    BorrowerDynamicRate(Address),
    // ── API versioning ─────────────────────────────────────────────────────────
    /// Current API version string
    ApiVersion,
    // ── Emergency admin revocation ─────────────────────────────────────────────
    /// Emergency admin revocation record — Address → bool (true = revoked).
    /// Revoked admins are excluded from admin approval checks.
    RevokedAdmin(Address),
    // ── Repayment confirmation ─────────────────────────────────────────────────
    /// loan_id → bool (repayment confirmed by oracle)
    RepaymentConfirmation(u64),
}

/// Issue #867: Shared collateral pool backed by multiple vouchers.
#[contracttype]
#[derive(Clone)]
pub struct CollateralPool {
    pub pool_id: u64,
    pub members: Vec<Address>,
    /// Stake per member (parallel to `members`), in stroops.
    pub stakes: Vec<i128>,
    /// Origin chain per member (parallel to `members`). `0` is the native chain.
    pub chain_ids: Vec<u32>,
    pub token: Address,
    pub borrower: Option<Address>,
    pub active: bool,
    pub created_at: u64,
}

// ── Governance ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct SlashVoteRecord {
    /// Total stake (in stroops) that has voted to approve this slash.
    /// 1 XLM = 10,000,000 stroops.
    pub approve_stake: i128,
    /// Total stake (in stroops) that has voted to reject this slash.
    /// 1 XLM = 10,000,000 stroops.
    pub reject_stake: i128,
    /// Addresses that have already cast a vote on this proposal.
    pub voters: Vec<Address>,
    /// `true` once the slash has been auto-executed after quorum was reached.
    pub executed: bool,
}

/// Slash escrow record holding slashed funds in 7-day escrow pending appeal.
#[contracttype]
#[derive(Clone)]
pub struct SlashEscrow {
    pub borrower: Address,
    pub loan_id: u64,
    /// Slashed amount held in escrow (50% of total stake).
    pub escrow_amount: i128,
    /// Timestamp when escrow period expires (created_at + 7 days).
    pub release_timestamp: u64,
    /// Status: Pending, Approved, or Rejected.
    pub status: AppealStatus,
}

/// Status of a slash appeal.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AppealStatus {
    /// Appeal in progress.
    Pending,
    /// Appeal approved; slash is overturned and funds returned to vouchers.
    Approved,
    /// Appeal rejected; funds are burned after escrow period.
    Rejected,
}

/// Record of a slash appeal voted on by vouchers (Issue #841: escrow-based).
#[contracttype]
#[derive(Clone)]
pub struct SlashEscrowAppealRecord {
    pub borrower: Address,
    pub loan_id: u64,
    /// Total stake that voted to approve the appeal (overturn slash).
    pub approve_stake: i128,
    /// Total stake that voted to reject the appeal (keep slash).
    pub reject_stake: i128,
    /// Addresses that have already voted on this appeal.
    pub voters: Vec<Address>,
    /// Timestamp when appeal was created.
    pub appeal_timestamp: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct DisputeRecord {
    pub borrower: Address,
    pub voucher: Address,
    pub evidence_hash: soroban_sdk::BytesN<32>,
    pub disputed_at: u64,
    pub resolved: Option<bool>,
}

/// Governance proposal to change the protocol slash threshold (`Config.slash_bps`).
#[contracttype]
#[derive(Clone)]
pub struct SlashThresholdProposal {
    pub id: u64,
    pub proposer: Address,
    pub proposed_threshold: i128,
    pub proposed_at: u64,
    pub approve_votes: u32,
    pub reject_votes: u32,
    pub voters: Vec<Address>,
    pub finalized: bool,
}

/// Config field targeted by an admin config-update proposal.
#[contracttype]
#[derive(Clone)]
pub enum ConfigUpdateKey {
    AdminThreshold,
}

/// Multi-sig admin proposal to update a config field.
#[contracttype]
#[derive(Clone)]
pub struct ConfigUpdateProposal {
    pub id: u64,
    pub proposer: Address,
    pub key: ConfigUpdateKey,
    pub new_value: u32,
    pub approvals: Vec<Address>,
    pub executed: bool,
}

// ── Admin Governance Queue with Multi-Signature Confirmation ─────────────────────

/// Issue #893: Admin operation types for multi-tier approval thresholds.
/// Different operations can require different numbers of admin approvals based on criticality.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdminOperationType {
    /// Low-risk operations (e.g., setting parameters like min_stake)
    Standard,
    /// Medium-risk operations (e.g., adding/removing tokens, admin changes)
    HighRisk,
    /// Critical operations (e.g., contract upgrade, pause, emergency actions)
    Critical,
}

/// Issue #893: Multi-tier admin approval thresholds for different operation types.
/// Allows different admin operations to require different numbers of approvals.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiTierAdminThresholds {
    /// Approvals required for standard operations (default: same as admin_threshold)
    pub standard_threshold: u32,
    /// Approvals required for high-risk operations (default: 2x standard)
    pub high_risk_threshold: u32,
    /// Approvals required for critical operations (default: all admins)
    pub critical_threshold: u32,
}

impl MultiTierAdminThresholds {
    /// Create default thresholds based on total admin count.
    /// Standard = 1, HighRisk = (total/2)+1, Critical = total
    pub fn default_for_admin_count(admin_count: u32) -> Self {
        let high_risk = if admin_count > 1 { (admin_count / 2) + 1 } else { 1 };
        let critical = admin_count;
        MultiTierAdminThresholds {
            standard_threshold: 1,
            high_risk_threshold: high_risk,
            critical_threshold: critical,
        }
    }

    /// Get the threshold for a specific operation type
    pub fn get_threshold(&self, operation_type: AdminOperationType) -> u32 {
        match operation_type {
            AdminOperationType::Standard => self.standard_threshold,
            AdminOperationType::HighRisk => self.high_risk_threshold,
            AdminOperationType::Critical => self.critical_threshold,
        }
    }
}

/// Types of governance actions that can be proposed in the admin governance queue.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GovernanceAction {
    /// Pause the contract
    Pause,
    /// Unpause the contract
    Unpause,
    /// Upgrade the contract to a new WASM hash
    Upgrade(BytesN<32>),
    /// Set protocol fee in basis points
    SetProtocolFee(u32),
    /// Set fee treasury address
    SetFeeTreasury(Address),
    /// Add an allowed token
    AddAllowedToken(Address),
    /// Remove an allowed token
    RemoveAllowedToken(Address),
    /// Set minimum stake amount
    SetMinStake(i128),
    /// Set maximum loan amount
    SetMaxLoanAmount(i128),
    /// Set minimum vouchers required
    SetMinVouchers(u32),
    /// Set maximum vouchers per borrower
    SetMaxVouchersPerBorrower(u32),
    /// Set max loan to stake ratio
    SetMaxLoanToStakeRatio(u32),
    /// Set grace period
    SetGracePeriod(u64),
    /// Set yield basis points
    SetYieldBps(i128),
    /// Set slash basis points
    SetSlashBps(i128),
    /// Set admin threshold
    SetAdminThreshold(u32),
    /// Add an admin
    AddAdmin(Address),
    /// Remove an admin
    RemoveAdmin(Address),
    /// Rotate an admin
    RotateAdmin(Address, Address),
    /// Set reputation NFT contract
    SetReputationNft(Address),
    /// Set whitelist enabled
    SetWhitelistEnabled(bool),
    /// Blacklist a borrower
    BlacklistBorrower(Address),
    /// Set prepayment penalty basis points
    SetPrepaymentPenaltyBps(u32),
    /// Set dynamic slash threshold enabled
    SetDynamicSlashThreshold(bool),
    /// Set loan size slash enabled
    SetLoanSizeSlashEnabled(bool),
    /// Set loan size slash max basis points
    SetLoanSizeSlashMaxBps(i128),
    /// Set successor admin
    SetSuccessorAdmin(Option<Address>),
    /// Set confirmation required
    SetConfirmationRequired(bool),
    /// Set admin compensation basis points
    SetAdminCompensationBps(u32),
    /// Set removal vote threshold
    SetRemovalVoteThreshold(u32),
    /// Set rate limit config
    SetRateLimitConfig(RateLimitConfig),
}

/// Status of a governance proposal in the queue.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GovernanceProposalStatus {
    /// Proposal is pending approval
    Pending,
    /// Proposal has been approved and can be executed
    Approved,
    /// Proposal has been executed
    Executed,
    /// Proposal has been cancelled
    Cancelled,
    /// Proposal has expired
    Expired,
}

/// A governance proposal in the admin governance queue with multi-signature confirmation.
#[contracttype]
#[derive(Clone)]
pub struct GovernanceProposal {
    /// Unique proposal ID
    pub id: u64,
    /// The governance action to be executed
    pub action: GovernanceAction,
    /// Address that proposed the action
    pub proposer: Address,
    /// Addresses that have approved this proposal
    pub approvals: Vec<Address>,
    /// Addresses that have rejected this proposal
    pub rejections: Vec<Address>,
    /// Current status of the proposal
    pub status: GovernanceProposalStatus,
    /// Ledger timestamp when the proposal was created
    pub created_at: u64,
    /// Ledger timestamp when the proposal can be executed (timelock)
    pub executable_at: u64,
    /// Ledger timestamp when the proposal expires (if not executed)
    pub expires_at: u64,
    /// Optional description or justification for the proposal
    pub description: soroban_sdk::String,
    /// Ledger timestamp when the proposal was executed (if applicable)
    pub executed_at: Option<u64>,
}

/// Governance queue configuration parameters.
#[contracttype]
#[derive(Clone)]
pub struct GovernanceQueueConfig {
    /// Minimum delay before a proposal can be executed (in seconds)
    pub timelock_delay: u64,
    /// Time window after executable_at during which a proposal can be executed (in seconds)
    pub execution_window: u64,
    /// Whether proposals require multi-sig approval (true) or can be executed by proposer (false)
    pub require_multisig: bool,
}

/// Default timelock delay for governance proposals (24 hours).
pub const DEFAULT_GOVERNANCE_TIMELOCK_DELAY: u64 = 24 * 60 * 60;

/// Default execution window for governance proposals (7 days).
pub const DEFAULT_GOVERNANCE_EXECUTION_WINDOW: u64 = 7 * 24 * 60 * 60;

// ── On-Chain Credit Score with Tiered Rewards ─────────────────────────────────────

/// Credit score tier levels.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreditTier {
    /// Tier 1: Poor (0-349)
    Poor,
    /// Tier 2: Fair (350-549)
    Fair,
    /// Tier 3: Good (550-699)
    Good,
    /// Tier 4: Very Good (700-849)
    VeryGood,
    /// Tier 5: Excellent (850-1000)
    Excellent,
}

/// Comprehensive credit score record for a borrower.
#[contracttype]
#[derive(Clone)]
pub struct CreditScore {
    /// Overall credit score (0-1000)
    pub score: u32,
    /// Current credit tier
    pub tier: CreditTier,
    /// Ledger timestamp when the score was last updated
    pub last_updated: u64,
    /// Total number of loans taken
    pub total_loans: u32,
    /// Number of successfully repaid loans
    pub successful_repayments: u32,
    /// Number of defaults
    pub defaults: u32,
    /// Total amount borrowed (in stroops)
    pub total_borrowed: i128,
    /// Total amount repaid (in stroops)
    pub total_repaid: i128,
    /// Account age in seconds
    pub account_age: u64,
    /// Number of times as a voucher
    pub voucher_count: u32,
    /// Average repayment time (in seconds before deadline, negative if late)
    pub avg_repayment_time: i64,
}

// ── zk-SNARK Confidentiality Types ─────────────────────────────────────────────

/// A zk-SNARK proof for confidential operations
#[contracttype]
#[derive(Clone)]
pub struct ZkProof {
    /// Proof points (compressed representation)
    pub proof_bytes: soroban_sdk::Bytes,
    /// Public inputs for the proof
    pub public_inputs: soroban_sdk::Vec<soroban_sdk::BytesN<32>>,
    /// Proof type identifier
    pub proof_type: u32,
}

/// A commitment to a confidential value using hash-based commitment
#[contracttype]
#[derive(Clone)]
pub struct ConfidentialCommitment {
    /// The commitment value (hash of blinded amount)
    pub commitment: soroban_sdk::BytesN<32>,
    /// The blinding factor (revealed only to trusted parties)
    pub blinding: soroban_sdk::BytesN<32>,
}

/// Public parameters for the zk-SNARK system
#[contracttype]
#[derive(Clone)]
pub struct ZkPublicParams {
    /// Verifying key hash (for on-chain verification)
    pub vk_hash: soroban_sdk::BytesN<32>,
    /// Circuit identifier
    pub circuit_id: u32,
}

/// Audit record for a zk-SNARK proof
#[contracttype]
#[derive(Clone)]
pub struct ZkProofRecord {
    /// Unique proof ID
    pub proof_id: u64,
    /// The proof that was verified
    pub proof: ZkProof,
    /// Operation type (vouch, loan_request, repayment)
    pub operation_type: u32,
    /// Address that submitted the proof
    pub submitter: Address,
    /// Whether verification succeeded
    pub verified: bool,
    /// Ledger timestamp when proof was submitted
    pub submitted_at: u64,
}

/// Proof types for different operations
pub const PROOF_TYPE_VOUCH: u32 = 1;
pub const PROOF_TYPE_LOAN_REQUEST: u32 = 2;
pub const PROOF_TYPE_REPAYMENT: u32 = 3;

/// Error for invalid proof type
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ZkError {
    InvalidProofType,
    ProofVerificationFailed,
    InvalidCommitment,
}

/// Reputation NFT badge record for borrowers reaching Excellent tier.
#[contracttype]
#[derive(Clone)]
pub struct ReputationNFTRecord {
    /// Address of the borrower who minted the badge
    pub borrower: Address,
    /// Ledger timestamp when the NFT badge was minted
    pub minted_at: u64,
}

/// Credit score calculation factors.
#[contracttype]
#[derive(Clone)]
pub struct CreditFactors {
    /// Weight for repayment history (0-10000 basis points)
    pub repayment_history_weight: u32,
    /// Weight for loan count (0-10000 basis points)
    pub loan_count_weight: u32,
    /// Weight for account age (0-10000 basis points)
    pub account_age_weight: u32,
    /// Weight for vouching activity (0-10000 basis points)
    pub vouching_weight: u32,
    /// Weight for repayment timeliness (0-10000 basis points)
    pub timeliness_weight: u32,
}

/// Tiered reward benefits for each credit tier.
#[contracttype]
#[derive(Clone)]
pub struct TierRewards {
    /// Yield basis points bonus (added to base yield)
    pub yield_bonus_bps: i32,
    /// Maximum loan amount multiplier (e.g., 150 = 1.5x)
    pub max_loan_multiplier: u32,
    /// Minimum stake reduction in basis points (e.g., 1000 = 10% reduction)
    pub min_stake_reduction_bps: u32,
    /// Loan duration extension in seconds (e.g., 7 days = 604800)
    pub duration_extension: u64,
    /// Fee discount in basis points (e.g., 500 = 5% discount)
    pub fee_discount_bps: u32,
}

/// Credit score configuration parameters.
#[contracttype]
#[derive(Clone)]
pub struct CreditScoreConfig {
    /// Whether credit scoring is enabled
    pub enabled: bool,
    /// Credit score calculation factors
    pub factors: CreditFactors,
    /// Rewards for each tier
    pub poor_rewards: TierRewards,
    pub fair_rewards: TierRewards,
    pub good_rewards: TierRewards,
    pub very_good_rewards: TierRewards,
    pub excellent_rewards: TierRewards,
}

/// Default credit score factors.
pub const DEFAULT_CREDIT_FACTORS: CreditFactors = CreditFactors {
    repayment_history_weight: 4000,  // 40%
    loan_count_weight: 1500,         // 15%
    account_age_weight: 1000,         // 10%
    vouching_weight: 1500,            // 15%
    timeliness_weight: 2000,          // 20%
};

/// Default tier rewards configuration.
pub const DEFAULT_POOR_REWARDS: TierRewards = TierRewards {
    yield_bonus_bps: 0,
    max_loan_multiplier: 100,
    min_stake_reduction_bps: 0,
    duration_extension: 0,
    fee_discount_bps: 0,
};

pub const DEFAULT_FAIR_REWARDS: TierRewards = TierRewards {
    yield_bonus_bps: 50,
    max_loan_multiplier: 110,
    min_stake_reduction_bps: 500,
    duration_extension: 86400,      // 1 day
    fee_discount_bps: 100,
};

pub const DEFAULT_GOOD_REWARDS: TierRewards = TierRewards {
    yield_bonus_bps: 100,
    max_loan_multiplier: 125,
    min_stake_reduction_bps: 1000,
    duration_extension: 172800,     // 2 days
    fee_discount_bps: 250,
};

pub const DEFAULT_VERY_GOOD_REWARDS: TierRewards = TierRewards {
    yield_bonus_bps: 150,
    max_loan_multiplier: 150,
    min_stake_reduction_bps: 1500,
    duration_extension: 345600,     // 4 days
    fee_discount_bps: 500,
};

pub const DEFAULT_EXCELLENT_REWARDS: TierRewards = TierRewards {
    yield_bonus_bps: 200,
    max_loan_multiplier: 200,
    min_stake_reduction_bps: 2000,
    duration_extension: 604800,     // 7 days
    fee_discount_bps: 1000,
};

/// Default credit score configuration.
pub const DEFAULT_CREDIT_SCORE_CONFIG: CreditScoreConfig = CreditScoreConfig {
    enabled: true,
    factors: DEFAULT_CREDIT_FACTORS,
    poor_rewards: DEFAULT_POOR_REWARDS,
    fair_rewards: DEFAULT_FAIR_REWARDS,
    good_rewards: DEFAULT_GOOD_REWARDS,
    very_good_rewards: DEFAULT_VERY_GOOD_REWARDS,
    excellent_rewards: DEFAULT_EXCELLENT_REWARDS,
};

// ── Loan Pool Syndication for Multi-Borrower Loans ───────────────────────────────

/// Syndication role for a member in a loan syndicate.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyndicationRole {
    /// Lead borrower - primary contact and decision maker
    LeadBorrower,
    /// Co-borrower - shares loan responsibility
    CoBorrower,
    /// Guarantor - provides additional collateral but not a borrower
    Guarantor,
}

/// Syndication member information.
#[contracttype]
#[derive(Clone)]
pub struct SyndicationMember {
    /// Member address
    pub address: Address,
    /// Role in the syndication
    pub role: SyndicationRole,
    /// Share of the loan (in basis points, e.g., 5000 = 50%)
    pub share_bps: u32,
    /// Collateral contributed (in stroops)
    pub collateral: i128,
    /// Vouches contributed (stake amount in stroops)
    pub vouch_stake: i128,
    /// Whether the member has approved the syndication
    pub approved: bool,
    /// Ledger timestamp when the member joined
    pub joined_at: u64,
}

/// Loan syndication record for multi-borrower loans.
#[contracttype]
#[derive(Clone)]
pub struct LoanSyndication {
    /// Unique syndication ID
    pub syndication_id: u64,
    /// Associated loan ID (if loan has been disbursed)
    pub loan_id: Option<u64>,
    /// Syndication members
    pub members: Vec<SyndicationMember>,
    /// Total loan amount requested (in stroops)
    pub total_amount: i128,
    /// Total collateral contributed (in stroops)
    pub total_collateral: i128,
    /// Total vouch stake (in stroops)
    pub total_vouch_stake: i128,
    /// Loan purpose description
    pub loan_purpose: soroban_sdk::String,
    /// Token address for the loan
    pub token_address: Address,
    /// Ledger timestamp when syndication was created
    pub created_at: u64,
    /// Ledger timestamp when syndication was disbursed (if applicable)
    pub disbursed_at: Option<u64>,
    /// Syndication status
    pub status: SyndicationStatus,
    /// Minimum number of approvals required
    pub min_approvals: u32,
    /// Current number of approvals
    pub approval_count: u32,
}

/// Syndication status.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SyndicationStatus {
    /// Syndication is being formed
    Forming,
    /// Syndication is ready for loan disbursement
    Ready,
    /// Loan has been disbursed
    Active,
    /// Loan has been fully repaid
    Repaid,
    /// Syndication has been cancelled
    Cancelled,
    /// Syndication has defaulted
    Defaulted,
}

/// Syndication repayment record.
#[contracttype]
#[derive(Clone)]
pub struct SyndicationRepayment {
    /// Syndication ID
    pub syndication_id: u64,
    /// Member who made the repayment
    pub repayer: Address,
    /// Amount repaid (in stroops)
    pub amount: i128,
    /// Ledger timestamp of repayment
    pub timestamp: u64,
}

/// Syndication configuration parameters.
#[contracttype]
#[derive(Clone)]
pub struct SyndicationConfig {
    /// Maximum number of members in a syndication
    pub max_members: u32,
    /// Minimum number of members required
    pub min_members: u32,
    /// Minimum approvals required (as percentage of members, e.g., 5000 = 50%)
    pub min_approval_percentage: u32,
    /// Maximum loan amount for syndication (in stroops)
    pub max_loan_amount: i128,
    /// Syndication fee in basis points (e.g., 100 = 1%)
    pub syndication_fee_bps: u32,
}

/// Default syndication configuration.
pub const DEFAULT_SYNDICATION_CONFIG: SyndicationConfig = SyndicationConfig {
    max_members: 10,
    min_members: 2,
    min_approval_percentage: 7500, // 75%
    max_loan_amount: 1_000_000_000_000, // 10 million XLM
    syndication_fee_bps: 100, // 1%
};

// ── Config ────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct Config {
    pub admins: Vec<Address>,
    pub admin_threshold: u32,
    /// Admin addresses that are permitted to be configured as admins.
    /// If empty, any valid admin address may be used.
    pub admin_whitelist: Vec<Address>,
    /// Admin addresses that are explicitly forbidden from being configured as admins.
    pub admin_blacklist: Vec<Address>,
    /// Primary token contract address used for loans and vouches.
    pub token: Address,
    /// Additional token contract addresses accepted for loans/vouches.
    pub allowed_tokens: Vec<Address>,
    /// Yield rate in basis points (e.g. 200 = 2%). Applied to loan principal (in stroops)
    /// at repayment: `yield = principal_stroops * yield_bps / 10_000`.
    pub yield_bps: i128,
    /// Slash fraction in basis points (e.g. 5000 = 50%). Applied to voucher stake (in stroops)
    /// on borrower default: `slashed = stake_stroops * slash_bps / 10_000`.
    pub slash_bps: i128,
    pub max_vouchers: u32,
    /// Minimum loan amount, in stroops. 1 XLM = 10,000,000 stroops.
    pub min_loan_amount: i128,
    /// Maximum loan duration, in seconds.
    pub loan_duration: u64,
    /// Maximum ratio of loan amount to total staked collateral, expressed as a percentage
    /// (e.g. 150 means loan ≤ 1.5 × total stake in stroops).
    pub max_loan_to_stake_ratio: u32,
    pub max_loan_to_collateral_ratio: u32,
    /// Grace period after loan deadline before the loan can be slashed, in seconds.
    pub grace_period: u64,
    /// Minimum age of a vouch before it can be used for loan eligibility, in seconds (default 24 hours).
    pub min_vouch_age_secs: u64,
    /// Prepayment penalty in basis points (e.g. 100 = 1%). Applied to remaining principal
    /// when a borrower repays early. 0 means no penalty.
    pub prepayment_penalty_bps: u32,
    /// #634: Liquidity mining reward rate in basis points per epoch (e.g. 50 = 0.5% per 7 days).
    pub liquidity_mining_rate_bps: u32,
    /// Voting period for slash-threshold governance proposals, in seconds.
    pub voting_period_seconds: u64,
    /// Minimum seconds between slashes for the same borrower (0 = disabled).
    pub slash_cooldown_seconds: u64,
        /// When true, critical write paths are blocked until multi-sig emergency unpause.
    pub emergency_pause_enabled: bool,
    /// Issue #668: Discount applied to yield on early repayment, in basis points (0 = no discount).
    pub early_repayment_discount_bps: u32,
    /// Issue #666/#667: Optional oracle contract address for repayment verification.
    pub oracle_address: Option<soroban_sdk::Address>,
    /// Delay (in seconds) after a slash vote reaches quorum before it can be executed (0 = immediate).
    pub slash_delay_seconds: u64,
    /// Designated successor admin address that can claim admin rights without multi-sig approval
    /// when current admins are unavailable.
    pub successor_admin: Option<Address>,
    pub rate_limit_config: RateLimitConfig,
    /// Issue #893: Multi-tier admin approval thresholds for different operation types.
    /// Empty means "not set" -- falls back to single admin_threshold for all operations.
    /// (0 or 1 elements; a `Vec` rather than `Option` because a custom struct nested in
    /// an `Option` field of a `#[contracttype]` isn't XDR-derivable in this SDK version.)
    pub multi_tier_thresholds: Vec<MultiTierAdminThresholds>,
    /// Recovery percentage for defaulted loans (in basis points, e.g. 5000 = 50%).
    pub recovery_percentage: u32,
    /// When true, the slash threshold is calculated dynamically based on pool health.
    pub dynamic_slash_threshold: bool,
    /// When true, loan size affects the maximum slash basis points.
    pub loan_size_slash_enabled: bool,
    /// Maximum slash in basis points when loan size slash is enabled (e.g. 8000 = 80%).
    pub loan_size_slash_max_bps: i128,
    /// When true, loans require admin confirmation before being executed.
    pub confirmation_required: bool,
    /// Admin compensation rate in basis points (e.g. 100 = 1%).
    pub admin_compensation_bps: u32,
    /// Minimum votes required to remove an admin via governance (0 = disabled).
    pub removal_vote_threshold: u32,
    /// Insurance premium rate in basis points collected at loan disbursement (e.g. 100 = 1%).
    /// Controls where redistributable slash funds flow after insurance allocation.
    pub redistribution_rule: RedistributionRule,
    /// Seconds after repayment during which a borrower is immune from slash votes (0 = disabled).
    pub immunity_period_seconds: u64,
    pub insurance_premium_bps: u32,
}

// ── Data Types ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct AmortizationEntry {
    pub due_date: u64,
    pub payment_due: i128,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MilestoneStatus {
    Pending,
    Submitted,
    Approved,
    Rejected,
    Expired,
}

#[contracttype]
#[derive(Clone)]
pub struct MilestoneRecord {
    pub milestone_id: u32,
    pub loan_id: u64,
    pub tranche_id: u32,
    pub status: MilestoneStatus,
    pub deadline: u64,
    pub description: soroban_sdk::String,
    pub submitted_at: Option<u64>,
    pub evidence_hash: Option<soroban_sdk::BytesN<32>>,
    pub proof_uri: Option<soroban_sdk::String>,
    pub approved_at: Option<u64>,
    pub approvers: Vec<Address>,
    pub rejection_reason: Option<soroban_sdk::String>,
    pub tranche_released: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct LoanRecord {
    pub id: u64,
    pub borrower: Address,
    /// #656: Third-party guarantor for this loan (None if no guarantor).
    pub guarantor: Option<Address>,
    /// #657: Buyback price set by borrower for vouchers to buy back stake (0 = not available).
    pub buyback_price: i128,
    /// #658: Whether automatic repayments are enabled for this loan.
    pub auto_repay_enabled: bool,
    /// #658: Number of repayment attempts made (for tracking auto-repay retries).
    pub auto_repay_attempts: u32,
    /// #666/#667: Escrow status for oracle-verified repayments.
    pub escrow_status: EscrowStatus,
    pub co_borrowers: Vec<Address>,
    /// Total loan principal disbursed, in stroops. 1 XLM = 10,000,000 stroops.
    pub amount: i128,
    /// Cumulative repayments received so far (principal + yield), in stroops.
    /// 1 XLM = 10,000,000 stroops.
    pub amount_repaid: i128,
    /// Yield owed to vouchers, locked in at disbursement time, in stroops.
    /// Computed as `amount * yield_bps / 10_000`. 1 XLM = 10,000,000 stroops.
    pub total_yield: i128,
        pub status: LoanStatus,
    pub repaid: bool,
    pub defaulted: bool,
    /// Ledger timestamp when the loan record was created.
    pub created_at: u64,
    /// Ledger timestamp when the loan was disbursed to the borrower.
    pub disbursement_timestamp: u64,
    /// Ledger timestamp when the loan was fully repaid; `None` if not yet repaid.
    pub repayment_timestamp: Option<u64>,
    /// Repayment deadline as a ledger timestamp.
    pub deadline: u64,
    /// Borrower-supplied description of the loan purpose.
    pub loan_purpose: soroban_sdk::String,
    /// Address of the token contract used for this loan.
    pub token_address: Address,
    /// Amortization schedule for partial repayments.
    pub amortization_schedule: Vec<AmortizationEntry>,
    /// Whether a repayment reminder has been sent for this loan.
    pub reminder_sent: bool,
    /// Risk score for the borrower (0-100), used for dynamic yield calculation.
    pub risk_score: u32,
    /// Number of payment deferment periods used on this loan.
    pub deferment_periods: u32,
    /// Optional custom maturity date (ledger timestamp). When set, overrides the
    /// default `deadline` computed from `loan_duration`. `None` means use `deadline`.
    pub maturity_date: Option<u64>,
    /// Interest rate type for this loan.
    pub rate_type: RateType,
    /// For variable-rate loans: the oracle key or index name used to look up the
    /// current rate (e.g. `"SOFR"`, `"PRIME"`). `None` for fixed-rate loans.
    pub index_reference: Option<soroban_sdk::String>,
    /// Issue #838: Timestamp of last compound interest calculation (for daily compounding).
    pub last_interest_calc: u64,
    /// Issue #838: Accrued compound interest from partial repayments (in stroops).
    pub accrued_interest: i128,
    /// Issue #838: Milestone bonus applied (50% repaid threshold).
    pub milestone_bonus_applied: bool,
    /// Issue #669: Retry count for failed repayments (max 3).
    pub retry_count: u32,
    /// Timestamp when the loan was suspended due to missed payment.
    pub suspension_timestamp: Option<u64>,
    /// Amount repaid when the loan entered suspension.
    pub suspension_amount_repaid: i128,
}

/// An archived loan record, stored separately to reduce active persistent storage.
/// Created when a loan reaches a terminal state (Repaid or Defaulted) and is moved
/// from active storage to archive to preserve history while reducing bloat.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefinanceRecord {
    pub old_loan_id: u64,
    pub new_loan_id: u64,
    pub borrower: Address,
    pub old_amount: i128,
    pub new_amount: i128,
    pub old_rate_bps: i128,
    pub new_rate_bps: i128,
    pub refinanced_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct ArchivedLoanRecord {
    /// Unique archive ID (monotonically increasing).
    pub archive_id: u64,
    /// Original loan ID before archival.
    pub original_loan_id: u64,
    /// Borrower address for historical audit trail.
    pub borrower: Address,
    /// Total principal in stroops.
    pub amount: i128,
    /// Cumulative repayments in stroops.
    pub amount_repaid: i128,
    /// Total yield locked in stroops.
    pub total_yield: i128,
    /// Final loan status before archival (should be Repaid or Defaulted).
    pub final_status: LoanStatus,
    /// Timestamp when the loan was originally created.
    pub created_at: u64,
    /// Timestamp when the loan was archived (terminal state reached).
    pub archived_at: u64,
    /// Original loan purpose for audit trail.
    pub loan_purpose: soroban_sdk::String,
    /// Token used for this loan.
    pub token_address: Address,
}

/// A reference to archived data stored on IPFS.
/// The actual data blob is stored on IPFS, and this contract maintains the hash for retrieval.
#[contracttype]
#[derive(Clone)]
pub struct IpfsArchiveReference {
    /// IPFS content hash (e.g., "Qm..." for v0 IPFS, "baf..." for v1 CIDv1)
    pub ipfs_hash: soroban_sdk::String,
    /// Timestamp when this archive was created
    pub archived_at: u64,
    /// Type of archive: "loan", "vouch_history", etc.
    pub archive_type: soroban_sdk::String,
}

/// #645: Pending loan restructure request — borrower requests, vouchers approve.
#[contracttype]
#[derive(Clone)]
pub struct RestructureRequest {
    pub borrower: Address,
    /// New deadline (must be after current deadline).
    pub new_deadline: u64,
    /// Reduced outstanding amount (0 = no change to amount).
    pub new_amount: i128,
    /// Ledger timestamp when the request was created.
    pub requested_at: u64,
    /// Voucher addresses that have approved this request.
    pub approvals: Vec<Address>,
}

/// A single payment event recorded against a loan.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRecord {
    /// Amount paid in this transaction, in stroops.
    pub amount: i128,
    /// Ledger timestamp of this payment.
    pub timestamp: u64,
    /// Cumulative amount repaid after this payment, in stroops.
    pub cumulative_repaid: i128,
}

#[contracttype]
#[derive(Clone)]
pub struct VouchRecord {
    pub voucher: Address,
    /// Amount staked by the voucher, in stroops. 1 XLM = 10,000,000 stroops.
    pub stake: i128,
    /// Ledger timestamp when this vouch was created; immutable after set.
    pub vouch_timestamp: u64,
    /// Token contract address that this stake is denominated in.
    pub token: Address,
    /// Optional expiry timestamp; if set and current time > expiry, vouch is expired.
    pub expiry_timestamp: Option<u64>,
    /// Optional delegate address; if set, this address can manage the vouch.
    pub delegate: Option<Address>,
    /// Optional chain ID for cross-chain vouches. `None` means native Stellar.
    /// When set, the token must originate from a registered bridge for that chain.
    pub chain_id: Option<u32>,
}

/// Metadata for a registered cross-chain bridge.
#[contracttype]
#[derive(Clone)]
pub struct BridgeRecord {
    /// Numeric chain identifier (e.g. 1 = Ethereum mainnet, 137 = Polygon).
    pub chain_id: u32,
    /// Human-readable chain name (e.g. "ethereum", "polygon").
    pub chain_name: soroban_sdk::String,
    /// The Stellar-side bridge contract address that wraps/unwraps tokens.
    pub bridge_address: Address,
    /// Whether this bridge is currently active and accepted for new vouches.
    pub active: bool,
}

#[contracttype]
#[derive(Clone)]
pub struct VouchHistoryEntry {
    /// Timestamp of the modification.
    pub timestamp: u64,
    /// Type of modification: "created", "increased", "decreased", "withdrawn", "delegated".
    pub modification_type: soroban_sdk::String,
    /// Stake amount involved in the modification, in stroops.
    pub stake_amount: i128,
    /// Optional delegate address if this is a delegation event.
    pub delegate: Option<Address>,
}

#[contracttype]
#[derive(Clone)]
pub struct LoanPoolRecord {
    pub pool_id: u64,
    pub borrowers: Vec<Address>,
    /// Per-borrower loan amounts in this pool, in stroops. 1 XLM = 10,000,000 stroops.
    pub amounts: Vec<i128>,
    /// Ledger timestamp when this pool was created.
    pub created_at: u64,
    /// Total amount disbursed from this pool across all borrowers, in stroops.
    /// 1 XLM = 10,000,000 stroops.
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

/// A pending slash awaiting execution after the mandatory delay period.
/// Created when a slash vote reaches quorum; executed via `execute_pending_slash`.
#[contracttype]
#[derive(Clone)]
pub struct PendingSlashRecord {
    pub borrower: Address,
    pub approved_at: u64,
    pub executable_at: u64,
    pub executed: bool,
}

/// Controls where redistributable slash funds flow after insurance allocation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RedistributionRule {
    /// Route to the slash treasury (default).
    Treasury,
    /// Redistribute pro-rata to remaining active vouchers of the borrower.
    Vouchers,
}

#[contracttype]
#[derive(Clone)]
pub struct SlashRecord {
    pub slash_id: u64,
    pub borrower: Address,
    pub loan_id: u64,
    pub loan_amount: i128,
    pub total_slashed: i128,
    pub slash_timestamp: u64,
    /// Amount returned to borrower from treasury on full repay (0 until recovered).
    pub recovery_amount: i128,
    /// Set by admin on reversal; None when not reversed.
    pub reversal_reason: Option<soroban_sdk::String>,
    /// True once an admin has reversed this slash.
    pub reversed: bool,
}

/// Monthly aggregated report of all slashing events.
#[contracttype]
#[derive(Clone)]
pub struct SlashingReportRecord {
    /// Month identifier: unix_timestamp / MONTHLY_PERIOD_SECS.
    pub month_id: u64,
    /// Total number of slash events in this month.
    pub total_slashes: u32,
    /// Total amount slashed across all events, in stroops.
    pub total_slashed: i128,
    /// Number of slashes subsequently reversed by admins.
    pub total_reversed: u32,
    /// Slash IDs recorded during this month.
    pub slash_ids: Vec<u64>,
}

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalRequest {
    pub voucher: Address,
    pub borrower: Address,
    pub token: Address,
    pub requested_at: u64,
}

/// A queued withdrawal request submitted during an active loan.
/// Processed automatically when the loan is repaid or slashed.
#[contracttype]
#[derive(Clone)]
pub struct QueuedWithdrawal {
    /// The voucher requesting withdrawal.
    pub voucher: Address,
    /// Token the stake is denominated in.
    pub token: Address,
    /// Ledger timestamp when the request was submitted.
    pub requested_at: u64,
    /// Whether this is a partial withdrawal (up to 50% of stake with penalty).
    pub partial: bool,
    /// Priority fee paid by the voucher (in stroops), distributed to remaining vouchers.
    pub priority_fee: i128,
}

/// Per-vouch yield allocation, locked at loan disbursement.
#[contracttype]
#[derive(Clone)]
pub struct YieldDistributionEntry {
    pub voucher: Address,
    pub yield_amount: i128,
}

/// Cumulative vouch reputation statistics for a voucher address.
#[contracttype]
#[derive(Clone)]
pub struct VoucherStats {
    pub successful_vouches: u32,
    pub total_vouches_slashed: u32,
    pub total_yield_earned: i128,
    pub total_slashed: i128,
}

/// Maximum number of loan entries to keep in the LRU cache.
pub const CACHE_LRU_MAX_ENTRIES: u32 = 100;
/// TTL for general cached records, in seconds (5 minutes).
pub const CACHE_TTL_SECS: u64 = 5 * 60;
/// TTL for cached yield-bps values, in seconds (5 minutes).
pub const YIELD_CACHE_TTL_SECS: u64 = 5 * 60;

/// Cached per-vouch yield rate for a (borrower, voucher) pair.
#[contracttype]
#[derive(Clone)]
pub struct CachedYieldRecord {
    /// The cached yield rate in basis points.
    pub yield_bps: i128,
    /// Ledger timestamp when this value was cached.
    pub cached_at: u64,
    /// The base yield_bps from config at cache time (for stale-config detection).
    pub base_yield_bps: i128,
}

/// Current API version of the contract.
pub const API_VERSION: u32 = 1;

#[contracttype]
#[derive(Clone)]
pub struct ApiVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

// ── Contract Versioning (Issue #742) ─────────────────────────────────────────

/// Semantic version record stored on-chain for the contract itself.
#[contracttype]
#[derive(Clone)]
pub struct ContractSemVer {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    /// Ledger timestamp when this version was set.
    pub updated_at: u64,
    /// Short human-readable change note (max 64 chars).
    pub note: soroban_sdk::String,
}

/// A single entry in the on-chain version history log.
#[contracttype]
#[derive(Clone)]
pub struct VersionHistoryEntry {
    pub version: ContractSemVer,
    /// Sequential index of this entry (0-based).
    pub index: u32,
}

// ── Deployment Records (Issue #743) ──────────────────────────────────────────

/// On-chain record of a single contract deployment or upgrade.
#[contracttype]
#[derive(Clone)]
pub struct DeploymentRecord {
    /// Sequential deployment index (0-based).
    pub index: u32,
    /// Deployer address that signed the transaction.
    pub deployer: Address,
    /// Ledger timestamp of the deployment.
    pub deployed_at: u64,
    /// Semantic version active at time of deployment.
    pub version: ContractSemVer,
    /// Network identifier ("testnet" | "mainnet").
    pub network: soroban_sdk::String,
}

// ── Rollback Snapshots (Issue #744) ──────────────────────────────────────────

/// Snapshot of critical config fields saved before an upgrade, used for rollback.
#[contracttype]
#[derive(Clone)]
pub struct RollbackSnapshot {
    /// Deployment index this snapshot corresponds to.
    pub deployment_index: u32,
    /// Ledger timestamp when the snapshot was taken.
    pub snapshot_at: u64,
    /// Semantic version at snapshot time.
    pub version: ContractSemVer,
    /// Serialised config — stores yield_bps, slash_bps, max_vouchers, and
    /// admin_threshold so a rollback can restore these critical parameters.
    pub yield_bps: i128,
    pub slash_bps: i128,
    pub max_vouchers: u32,
    pub admin_threshold: u32,
}

// ── API Caching (Issue #724) ──────────────────────────────────────────────────

/// Issue #687: Governance proposal to remove a compromised admin address.
/// Passes when `approve_votes >= Config.removal_vote_threshold`.
#[contracttype]
#[derive(Clone)]
pub struct AdminRemovalProposal {
    pub id: u64,
    /// Admin address to be removed if the proposal passes.
    pub admin_to_remove: Address,
    /// Address that created the proposal (must be a governance participant).
    pub proposer: Address,
    /// Number of approve votes cast so far.
    pub approve_votes: u32,
    /// Number of reject votes cast so far.
    pub reject_votes: u32,
    /// Addresses that have already voted (prevent double-voting).
    pub voters: Vec<Address>,
    /// Ledger timestamp when the proposal was created.
    pub proposed_at: u64,
    /// True once the proposal has been finalized (admin removed or rejected).
    pub finalized: bool,
}

// ── Pagination ────────────────────────────────────────────────────────────────

#[contracttype]
pub enum CacheKey {
    LoanCache(u64),           // loan_id → CachedLoanRecord
    VouchesCache(Address),    // borrower → CachedVouchesRecord
    ConfigCache,              // CachedConfigRecord
}

#[contracttype]
#[derive(Clone)]
pub struct CachedLoanRecord {
    pub data: LoanRecord,
    pub cached_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct CachedVouchesRecord {
    pub data: Vec<VouchRecord>,
    pub cached_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct CachedConfigRecord {
    pub data: Config,
    pub cached_at: u64,
}

// ── Risk Assessment Voting (Issue #903) ──────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct RiskThresholdProposal {
    pub id: u64,
    pub proposer: Address,
    pub min_risk_threshold: u32,  // basis points (e.g., 5000 = 50%)
    pub max_risk_threshold: u32,  // basis points
    pub votes_for: i128,
    pub votes_against: i128,
    pub status: GovernanceProposalStatus,
    pub created_at: u64,
    pub eta: u64,
}

// ── Fee Structure Voting (Issue #904) ──────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct FeeStructureProposal {
    pub id: u64,
    pub proposer: Address,
    pub origination_fee_bps: u32,
    pub repayment_fee_bps: u32,
    pub late_fee_bps: u32,
    pub votes_for: i128,
    pub votes_against: i128,
    pub status: GovernanceProposalStatus,
    pub created_at: u64,
    pub eta: u64,
}

// ── Withdrawal Timelock (Issue #905) ───────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct WithdrawalTimelock {
    pub id: u64,
    pub voucher: Address,
    pub borrower: Address,
    pub amount: i128,
    pub token: Address,
    pub eta: u64,
    pub executed: bool,
    pub cancelled: bool,
}

// ── Cross-Chain Proposal Sync (Issue #906) ────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct CrossChainProposalSync {
    pub id: u64,
    pub source_chain: String,
    pub target_chains: Vec<String>,
    pub proposal_type: String,  // "risk", "fee", "timelock"
    pub proposal_data: soroban_sdk::Bytes,
    pub votes_required: u32,
    pub votes_received: u32,
    pub status: GovernanceProposalStatus,
    pub created_at: u64,
    pub eta: u64,
}

// ── Error Standardization (Issue #725) ────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub struct ErrorResponse {
    /// Numeric error code matching ContractError enum.
    pub code: u32,
    /// Human-readable error message.
    pub message: soroban_sdk::String,
    /// Optional additional context or details.
    pub details: Option<soroban_sdk::String>,
    /// Timestamp when the error occurred.
    pub timestamp: u64,
}

// ── Issue #868: Gradual Unstaking ────────────────────────────────────────────

/// Default number of equal instalments for gradual unstaking (4 tranches).
pub const DEFAULT_GRADUAL_UNSTAKE_INSTALMENTS: u32 = 4;
/// Default interval between instalments, in seconds (7 days).
pub const DEFAULT_GRADUAL_UNSTAKE_INTERVAL_SECS: u64 = 7 * 24 * 60 * 60;

/// Progressive vouch-revocation schedule: stake released in equal instalments.
#[contracttype]
#[derive(Clone)]
pub struct GradualUnstakeSchedule {
    pub voucher: Address,
    pub borrower: Address,
    pub token: Address,
    /// Total stake to release across all instalments, in stroops.
    pub total_amount: i128,
    /// Amount per instalment, in stroops.
    pub instalment_amount: i128,
    pub instalments_paid: u32,
    pub total_instalments: u32,
    pub interval_secs: u64,
    pub created_at: u64,
    /// Ledger timestamp when the next instalment becomes claimable.
    pub next_release_at: u64,
}

// ── Issue #884: Prepayment Bonus ────────────────────────────────────────────

/// Default prepayment bonus rate in basis points (50 = 0.5% of loan amount).
pub const DEFAULT_PREPAYMENT_BONUS_BPS: u32 = 50;

// ── Issue #885: Loan Status Privacy ─────────────────────────────────────────

/// Privacy level for loan status visibility.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LoanPrivacyLevel {
    /// Anyone can view loan details (default).
    Public,
    /// Only the borrower and their vouchers can view loan details.
    VouchersOnly,
    /// Only the borrower can view loan details.
    Private,
}

// ── Issue #887: Loan Subordination and Cascading Debt Hierarchy ──────────────

/// Issue #887: Subordination level in the debt hierarchy.
/// Determines priority order for repayment and default cascading.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SubordinationLevel {
    /// Senior (Priority 0): Highest priority. Must be fully repaid first.
    /// Default of senior loan blocks all subordinate loans.
    Senior = 0,
    /// Mezzanine (Priority 1): Intermediate level.
    /// Can have both senior and subordinate loans.
    Mezzanine = 1,
    /// Subordinate (Priority 2+): Lowest priority.
    /// Repaid after seniors. Affected by senior defaults (cascading).
    Subordinate = 2,
}

/// Issue #887: Represents a subordination relationship between two loans.
/// Links a subordinate (junior) loan to its senior (creditor priority) loan.
#[contracttype]
#[derive(Clone)]
pub struct SubordinationRecord {
    /// ID of the senior (higher priority) loan
    pub senior_loan_id: u64,
    /// ID of the subordinate (lower priority) loan
    pub subordinate_loan_id: u64,
    /// The subordination level relative to the senior loan
    pub subordination_level: SubordinationLevel,
    /// Ledger timestamp when this subordination relationship was created
    pub created_at: u64,
    /// Whether this subordination is currently active (true) or waived (false)
    pub is_active: bool,
    /// Priority order index if senior loan has multiple subordinates (0 = highest priority)
    pub priority_index: u32,
}

/// Issue #887: Represents cascading default information.
/// Tracks which loans are affected when a senior loan defaults.
#[contracttype]
#[derive(Clone)]
pub struct CascadingDefault {
    /// ID of the senior loan that defaulted and triggered the cascade
    pub triggering_senior_loan_id: u64,
    /// IDs of all subordinate loans affected by this default
    pub affected_subordinate_ids: Vec<u64>,
    /// Ledger timestamp when the cascade was triggered
    pub triggered_at: u64,
    /// Whether the cascade has been fully resolved (all affected loans handled)
    pub is_resolved: bool,
}

/// Issue #887: Waterfall repayment distribution result.
/// Specifies how a repayment should be split between senior and subordinate loans.
#[contracttype]
#[derive(Clone)]
pub struct WaterfallDistribution {
    /// Amount to apply to the senior loan in stroops
    pub senior_amount: i128,
    /// Amount to apply to subordinate loans in stroops
    pub subordinate_amount: i128,
    /// Total amount distributed across all tiers
    pub total_distributed: i128,
}

/// Issue #887: DataKey for subordination relationships
/// Added to DataKey enum for storage:
/// `SubordinationRelation(u64, u64)` => (senior_loan_id, subordinate_loan_id) -> SubordinationRecord
/// `SubordinateLoansList(u64)` => senior_loan_id -> Vec<u64> (IDs of all subordinate loans)
/// `SeniorLoanOf(u64)` => subordinate_loan_id -> u64 (ID of direct senior loan)
/// `CascadingDefaultRecord(u64)` => senior_loan_id -> CascadingDefault
pub const MAX_SUBORDINATION_DEPTH: u32 = 10; // Prevent deeply nested hierarchies
pub const MAX_SUBORDINATES_PER_LOAN: u32 = 50; // Prevent excessive branching

/// Result for a single entry in `batch_vouch` with selective rollback semantics (Issue #1055).
/// Successful entries are committed; failed entries are skipped with an error code.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchVouchResult {
    /// The borrower address for this entry.
    pub borrower: Address,
    /// The stake amount attempted for this entry.
    pub stake: i128,
    /// `true` if the vouch was committed successfully; `false` if it was skipped.
    pub success: bool,
    /// Error code if `success == false`; `None` when successful.
    pub error_code: Option<u32>,
}

// ── Issue #64: Oracle Price Staleness ─────────────────────────────────────────

/// Staleness window for oracle price records, in seconds (1 hour).
pub const ORACLE_PRICE_MAX_AGE_SECS: u64 = 60 * 60;

/// An oracle price record with a value and timestamp.
#[contracttype]
#[derive(Clone)]
pub struct OraclePriceRecord {
    pub price: i128,
    pub recorded_at: u64,
}

// ── Issue #65: Graduated Response / Tiered Lockdown ──────────────────────────

/// Protocol threat level for graduated response.
#[contracttype]
#[derive(Clone, PartialEq)]
pub enum ThreatLevel {
    Normal,
    Elevated,
    Critical,
    Lockdown,
}

// ── Issue #552 / #841: Slash Appeal ───────────────────────────────────────────

/// A slash appeal record submitted by a voucher on behalf of a defaulted borrower.
#[contracttype]
#[derive(Clone)]
pub struct SlashAppealRecord {
    pub borrower: Address,
    pub voucher: Address,
    pub evidence_hash: soroban_sdk::BytesN<32>,
    pub appeal_timestamp: u64,
    pub approved: Option<bool>,
    pub admin_votes: Vec<Address>,
}

#[contracttype]
#[derive(Clone)]
pub struct AdminActionProposal {
    pub id: u64,
    pub action_type: soroban_sdk::String,
    pub proposer: Address,
    pub approvals: Vec<Address>,
    pub created_at: u64,
    pub executed: bool,
}

// ── Issue #910: Vouch Merkle Root ─────────────────────────────────────────────

/// Vouch Merkle root record stored per borrower.
#[contracttype]
#[derive(Clone)]
pub struct VouchMerkleRoot {
    pub root: BytesN<32>,
    pub vouch_count: u32,
    pub computed_at: u64,
}

// ── Issue #881: Dynamic Interest Rate based on Risk Score ───────────────────

/// Default base interest rate in basis points (200 = 2%).
pub const DEFAULT_DYNAMIC_BASE_RATE_BPS: u32 = 200;
/// Maximum rate adjustment per risk point in basis points (50 = 0.5% per risk point).
pub const DEFAULT_RISK_RATE_ADJUSTMENT_BPS: u32 = 50;
/// Maximum dynamic rate cap in basis points (5000 = 50%).
pub const DEFAULT_DYNAMIC_RATE_CAP_BPS: u32 = 5000;
/// Minimum dynamic rate floor in basis points (50 = 0.5%).
pub const DEFAULT_DYNAMIC_RATE_FLOOR_BPS: u32 = 50;

/// Configuration for dynamic interest rate calculation.
#[contracttype]
#[derive(Clone)]
pub struct DynamicRateConfig {
    pub enabled: bool,
    /// Base rate in basis points before risk adjustment.
    pub base_rate_bps: u32,
    /// Rate increase per risk score point (0-100 scale), in basis points.
    pub risk_adjustment_bps: u32,
    /// Maximum allowed rate in basis points.
    pub rate_cap_bps: u32,
    /// Minimum allowed rate in basis points.
    pub rate_floor_bps: u32,
    /// Oracle price key to consult as a risk input (e.g. the collateral token's
    /// price feed). `None` disables oracle-informed pricing entirely.
    pub oracle_price_symbol: Option<soroban_sdk::Symbol>,
    /// If the fresh oracle price is below this level, `oracle_risk_premium_bps`
    /// is added to the rate (the market has moved against the collateral).
    pub oracle_risk_threshold: i128,
    /// Risk premium applied when the oracle price is fresh but below threshold.
    pub oracle_risk_premium_bps: u32,
    /// Conservative premium applied when the oracle price is missing or stale
    /// (past `ORACLE_PRICE_MAX_AGE_SECS`) — we can't confirm the market is safe,
    /// so we price as if it were not.
    pub oracle_stale_premium_bps: u32,
}

/// Default dynamic rate configuration.
pub const DEFAULT_DYNAMIC_RATE_CONFIG: DynamicRateConfig = DynamicRateConfig {
    enabled: false,
    base_rate_bps: DEFAULT_DYNAMIC_BASE_RATE_BPS,
    risk_adjustment_bps: DEFAULT_RISK_RATE_ADJUSTMENT_BPS,
    rate_cap_bps: DEFAULT_DYNAMIC_RATE_CAP_BPS,
    rate_floor_bps: DEFAULT_DYNAMIC_RATE_FLOOR_BPS,
    oracle_price_symbol: None,
    oracle_risk_threshold: 0,
    oracle_risk_premium_bps: 0,
    oracle_stale_premium_bps: 0,
};

/// Snapshot of the dynamic rate applied to a specific borrower's loan.
#[contracttype]
#[derive(Clone)]
pub struct BorrowerDynamicRate {
    pub borrower: Address,
    pub loan_id: u64,
    /// Computed rate in basis points at time of disbursement or recalculation.
    pub effective_rate_bps: u32,
    /// Risk score used for computation (0-100).
    pub risk_score: u32,
    /// Credit tier at time of computation.
    pub credit_tier: CreditTier,
    /// Ledger timestamp when rate was last computed.
    pub computed_at: u64,
}

// ── Issue #878: Loan Forbearance ────────────────────────────────────────────

/// Maximum number of forbearance periods allowed per loan.
pub const MAX_FORBEARANCE_PERIODS: u32 = 2;
/// Default forbearance duration in seconds (30 days).
pub const DEFAULT_FORBEARANCE_DURATION_SECS: u64 = 30 * 24 * 60 * 60;

/// Forbearance status.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ForbearanceStatus {
    Active,
    Ended,
    Expired,
}

/// Record of a forbearance period on a loan.
#[contracttype]
#[derive(Clone)]
pub struct ForbearanceRecord {
    pub loan_id: u64,
    pub borrower: Address,
    /// Timestamp when forbearance was granted.
    pub started_at: u64,
    /// Duration of the forbearance in seconds.
    pub duration_secs: u64,
    /// Timestamp when forbearance ends.
    pub ends_at: u64,
    /// Original deadline before forbearance extension.
    pub original_deadline: u64,
    /// How many forbearance periods have been used on this loan (including this one).
    pub period_number: u32,
    pub status: ForbearanceStatus,
}
