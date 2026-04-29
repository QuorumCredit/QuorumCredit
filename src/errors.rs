use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ContractError {
    InsufficientFunds = 1,
    ActiveLoanExists = 2,
    StakeOverflow = 3,
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
    VoucherNotWhitelisted = 23,
    Blacklisted = 24,
    TimelockNotFound = 25,
    TimelockNotReady = 26,
    TimelockExpired = 27,
    NoVouchesForBorrower = 28,
    VoucherNotFound = 29,
    InvalidToken = 30,
    AlreadyVoted = 31,
    SlashVoteNotFound = 32,
    SlashAlreadyExecuted = 33,
    LoanBelowMinAmount = 34,
    QuorumNotMet = 35,
    MaxVouchersPerBorrowerExceeded = 36,
    InsufficientVoucherBalance = 37,
    SelfVouchNotAllowed = 38,
    DuplicateToken = 39,
    InvalidAdminThreshold = 40,
    InsufficientYieldReserve = 41,
    ReminderAlreadySent = 42,
    /// Insurance pool has no funds to cover the claim.
    InsurancePoolEmpty = 43,
    /// Insurance claim already made for this loan.
    InsuranceClaimAlreadyMade = 44,
    /// Basis points value is invalid (must be 0–10000).
    InvalidBps = 45,
}
