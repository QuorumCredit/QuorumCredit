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
    LoanBelowMinAmount = 34,
    QuorumNotMet = 35,
    MaxVouchersPerBorrowerExceeded = 36,
    /// Voucher has insufficient balance to stake the requested amount.
    InsufficientVoucherBalance = 37,
    /// Voucher and borrower must be different addresses.
    SelfVouchNotAllowed = 38,
    DuplicateToken = 39,
    /// Admin threshold must be > 0 and <= number of admins.
    InvalidAdminThreshold = 40,
    /// Yield reserve is insufficient to cover promised yield.
    InsufficientYieldReserve = 41,
    /// Reminder already sent for this loan.
    ReminderAlreadySent = 42,
}
