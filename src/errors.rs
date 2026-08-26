use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    InsufficientFunds = 1,
    /// Borrower already has an active (non-repaid, non-defaulted) loan.
    ActiveLoanExists = 2,
    /// Total vouched stake overflowed i128.
    StakeOverflow = 3,
    /// admin or token address must not be the zero address.
    ZeroAddress = 4,
    DuplicateVouch = 5,
    NoActiveLoan = 6,
    ContractPaused = 7,
    LoanPastDeadline = 8,
    PoolLengthMismatch = 9,
    PoolEmpty = 10,
    PoolBorrowerActiveLoan = 11,
    PoolInsufficientFunds = 12,
    MinStakeNotMet = 13,
    LoanExceedsMaxAmount = 14,
    InsufficientVouchers = 15,
    UnauthorizedCaller = 16,
    InvalidAmount = 17,
    InvalidStateTransition = 18,
    AlreadyInitialized = 19,
    VouchTooRecent = 20,
    VouchCooldownActive = 21,
    BorrowerHasActiveLoan = 22,
    VoucherNotWhitelisted = 23,
    Blacklisted = 24,
    TimelockNotFound = 25,
    TimelockNotReady = 26,
    TimelockExpired = 27,
    NoVouchesForBorrower = 28,
    VoucherNotFound = 29,
    /// Token address does not implement the SEP-41 token interface.
    InvalidToken = 30,
    AlreadyVoted = 31,
    SlashVoteNotFound = 32,
    SlashAlreadyExecuted = 33,

    // ── Insurance Marketplace ─────────────────────────────────────────────────
    /// Requested insurance provider does not exist.
    ProviderNotFound = 34,
    /// Requested insurance product does not exist.
    ProductNotFound = 35,
    /// Requested insurance quote does not exist.
    QuoteNotFound = 36,
    /// Requested insurance claim does not exist.
    ClaimNotFound = 37,
    /// Provider or product is marked inactive and cannot be used.
    ProviderInactive = 38,
    /// Quote has already been accepted (premium paid); cannot accept again.
    QuoteAlreadyAccepted = 39,
    /// Claim can only be filed against an accepted (active) quote.
    QuoteNotAccepted = 40,
    /// A claim has already been filed for this quote.
    ClaimAlreadyFiled = 41,
    /// Claim is not in a state that allows the requested transition.
    InvalidClaimStatus = 42,
}
