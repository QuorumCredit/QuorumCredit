use soroban_sdk::{contracterror, contracttype, String, Env};

// ── Issue #109: Structured ApiError shape ────────────────────────────────────

/// A structured error payload that can be surfaced to off-chain clients.
///
/// When a contract function returns a `ContractError`, integrators can convert
/// it into an `ApiError` for consistent JSON/API response shaping:
///
/// ```json
/// {
///   "code": 1,
///   "message": "InsufficientFunds",
///   "details": "Total vouched stake is below the requested loan threshold."
/// }
/// ```
///
/// # Fields
/// - `code`    — The stable numeric error code matching `ContractError` discriminants.
/// - `message` — Short, human-readable error name (matches the enum variant name).
/// - `details` — Optional extended description, may contain context-specific hints.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ApiError {
    /// Stable numeric code — identical to the `ContractError` u32 discriminant.
    pub code: u32,
    /// Short human-readable error label (e.g. `"InsufficientFunds"`).
    pub message: String,
    /// Optional extended description with resolution hints.
    /// Empty string means no extra detail is available.
    pub details: String,
}

impl ApiError {
    /// Convert a `ContractError` into a structured `ApiError` with a stable
    /// code, a canonical message string, and an optional detail hint.
    pub fn from_contract_error(env: &Env, err: ContractError) -> Self {
        let (message, details) = contract_error_meta(env, err);
        ApiError {
            code: err as u32,
            message,
            details,
        }
    }
}

/// Returns `(message, details)` strings for each known `ContractError` variant.
pub fn contract_error_meta(env: &Env, err: ContractError) -> (String, String) {
    match err {
        ContractError::InsufficientFunds => (
            String::from_str(env, "InsufficientFunds"),
            String::from_str(env, "Stake or amount is zero/negative; or contract balance is insufficient for disbursement."),
        ),
        ContractError::ActiveLoanExists => (
            String::from_str(env, "ActiveLoanExists"),
            String::from_str(env, "Borrower already has an active loan. Wait for repayment or default resolution."),
        ),
        ContractError::StakeOverflow => (
            String::from_str(env, "StakeOverflow"),
            String::from_str(env, "Summing vouched stakes would overflow i128. Reduce stake amounts."),
        ),
        ContractError::ZeroAddress => (
            String::from_str(env, "ZeroAddress"),
            String::from_str(env, "Admin or token address is the zero address. Provide a valid address."),
        ),
        ContractError::DuplicateVouch => (
            String::from_str(env, "DuplicateVouch"),
            String::from_str(env, "Voucher already has an active vouch for this borrower. Use increase_stake() instead."),
        ),
        ContractError::NoActiveLoan => (
            String::from_str(env, "NoActiveLoan"),
            String::from_str(env, "No active loan found for borrower. Verify the borrower address."),
        ),
        ContractError::ContractPaused => (
            String::from_str(env, "ContractPaused"),
            String::from_str(env, "Contract is paused. Wait for an admin to call unpause()."),
        ),
        ContractError::LoanPastDeadline => (
            String::from_str(env, "LoanPastDeadline"),
            String::from_str(env, "Repayment deadline has passed. Use slash() to mark default."),
        ),
        ContractError::PoolLengthMismatch => (
            String::from_str(env, "PoolLengthMismatch"),
            String::from_str(env, "Pool borrower/stake arrays have different lengths."),
        ),
        ContractError::PoolEmpty => (
            String::from_str(env, "PoolEmpty"),
            String::from_str(env, "Pool has no members."),
        ),
        ContractError::PoolBorrowerActiveLoan => (
            String::from_str(env, "PoolBorrowerActiveLoan"),
            String::from_str(env, "A pool member already has an active loan."),
        ),
        ContractError::PoolInsufficientFunds => (
            String::from_str(env, "PoolInsufficientFunds"),
            String::from_str(env, "Pool has insufficient funds for the requested operation."),
        ),
        ContractError::MinStakeNotMet => (
            String::from_str(env, "MinStakeNotMet"),
            String::from_str(env, "Vouch stake is below the admin-configured minimum. Increase stake."),
        ),
        ContractError::LoanExceedsMaxAmount => (
            String::from_str(env, "LoanExceedsMaxAmount"),
            String::from_str(env, "Requested loan exceeds admin-configured maximum. Request a smaller amount."),
        ),
        ContractError::InsufficientVouchers => (
            String::from_str(env, "InsufficientVouchers"),
            String::from_str(env, "Number of vouchers is below admin-configured minimum. Recruit more vouchers."),
        ),
        ContractError::UnauthorizedCaller => (
            String::from_str(env, "UnauthorizedCaller"),
            String::from_str(env, "Caller is not authorized for this operation."),
        ),
        ContractError::InvalidAmount => (
            String::from_str(env, "InvalidAmount"),
            String::from_str(env, "Numeric parameter fails validity check. Pass a value in the documented valid range."),
        ),
        ContractError::InvalidStateTransition => (
            String::from_str(env, "InvalidStateTransition"),
            String::from_str(env, "Operation is not valid for the current loan status. Check loan_status() first."),
        ),
        ContractError::AlreadyInitialized => (
            String::from_str(env, "AlreadyInitialized"),
            String::from_str(env, "Contract is already initialized. initialize() is one-time only."),
        ),
        ContractError::VouchTooRecent => (
            String::from_str(env, "VouchTooRecent"),
            String::from_str(env, "Vouch was added too recently. Wait for the vouch age requirement to pass."),
        ),
        ContractError::VouchCooldownActive => (
            String::from_str(env, "VouchCooldownActive"),
            String::from_str(env, "Vouch cooldown is still active. Wait before vouching again."),
        ),
        ContractError::BorrowerHasActiveLoan => (
            String::from_str(env, "BorrowerHasActiveLoan"),
            String::from_str(env, "Borrower currently has an active loan. Operation is not permitted."),
        ),
        ContractError::VoucherNotWhitelisted => (
            String::from_str(env, "VoucherNotWhitelisted"),
            String::from_str(env, "Voucher address is not in the whitelist. Contact the admin."),
        ),
        ContractError::Blacklisted => (
            String::from_str(env, "Blacklisted"),
            String::from_str(env, "Address is blacklisted. Contact the protocol admin."),
        ),
        ContractError::TimelockNotFound => (
            String::from_str(env, "TimelockNotFound"),
            String::from_str(env, "Timelock operation ID not found. Verify the ID returned when operation was queued."),
        ),
        ContractError::TimelockNotReady => (
            String::from_str(env, "TimelockNotReady"),
            String::from_str(env, "Timelock delay has not elapsed. Wait and retry."),
        ),
        ContractError::TimelockExpired => (
            String::from_str(env, "TimelockExpired"),
            String::from_str(env, "Timelock operation has expired. Re-queue the operation."),
        ),
        ContractError::NoVouchesForBorrower => (
            String::from_str(env, "NoVouchesForBorrower"),
            String::from_str(env, "No vouches found for this borrower. Verify the borrower address."),
        ),
        ContractError::VoucherNotFound => (
            String::from_str(env, "VoucherNotFound"),
            String::from_str(env, "Voucher address not found in borrower's vouch list."),
        ),
        ContractError::InvalidToken => (
            String::from_str(env, "InvalidToken"),
            String::from_str(env, "Token is not allowed or does not implement SEP-41. Use get_config() to list allowed tokens."),
        ),
        ContractError::AlreadyVoted => (
            String::from_str(env, "AlreadyVoted"),
            String::from_str(env, "Voucher has already voted on this slash proposal. Each voucher votes once."),
        ),
        ContractError::SlashVoteNotFound => (
            String::from_str(env, "SlashVoteNotFound"),
            String::from_str(env, "No open slash proposal for this borrower. Initiate a slash vote first."),
        ),
        ContractError::SlashAlreadyExecuted => (
            String::from_str(env, "SlashAlreadyExecuted"),
            String::from_str(env, "Slash already executed for this borrower. No further action needed."),
        ),
        ContractError::QuorumNotMet => (
            String::from_str(env, "QuorumNotMet"),
            String::from_str(env, "Slash vote quorum not reached. Recruit more voucher votes."),
        ),
        ContractError::AlreadyRepaid => (
            String::from_str(env, "AlreadyRepaid"),
            String::from_str(env, "Loan has already been fully repaid. No further action needed."),
        ),
        ContractError::PermissionNotDelegated => (
            String::from_str(env, "PermissionNotDelegated"),
            String::from_str(env, "Admin has not delegated this permission. Delegation required."),
        ),
        ContractError::ProposalVetoed => (
            String::from_str(env, "ProposalVetoed"),
            String::from_str(env, "Governance proposal was vetoed by an admin."),
        ),
        ContractError::SelfVouchNotAllowed => (
            String::from_str(env, "SelfVouchNotAllowed"),
            String::from_str(env, "Voucher and borrower cannot be the same address."),
        ),
        ContractError::InvalidBps => (
            String::from_str(env, "InvalidBps"),
            String::from_str(env, "Basis points value is invalid. Must be in range 0–10000."),
        ),
        ContractError::DuplicateToken => (
            String::from_str(env, "DuplicateToken"),
            String::from_str(env, "Token is already in the allowed tokens list."),
        ),
        ContractError::LoanNotCancellable => (
            String::from_str(env, "LoanNotCancellable"),
            String::from_str(env, "Loan is not in a cancellable state."),
        ),
        ContractError::CancellationWindowExpired => (
            String::from_str(env, "CancellationWindowExpired"),
            String::from_str(env, "Cancellation window has expired for this loan."),
        ),
        ContractError::LoanTooLarge => (
            String::from_str(env, "LoanTooLarge"),
            String::from_str(env, "Loan amount exceeds the large-loan threshold and requires multi-sig approval."),
        ),
        ContractError::LargeLoanPendingApproval => (
            String::from_str(env, "LargeLoanPendingApproval"),
            String::from_str(env, "Large loan is pending admin approval."),
        ),
        ContractError::LargeLoanNotApproved => (
            String::from_str(env, "LargeLoanNotApproved"),
            String::from_str(env, "Large loan has not been approved by the required admins."),
        ),
        ContractError::LargeLoanDelayNotElapsed => (
            String::from_str(env, "LargeLoanDelayNotElapsed"),
            String::from_str(env, "Required delay after large-loan approval has not elapsed."),
        ),
        ContractError::LargeLoanAlreadyExecuted => (
            String::from_str(env, "LargeLoanAlreadyExecuted"),
            String::from_str(env, "Large loan approval has already been executed."),
        ),
        ContractError::CircularVouchDetected => (
            String::from_str(env, "CircularVouchDetected"),
            String::from_str(env, "Circular vouch relationship detected. Vouch graph must be acyclic."),
        ),
        ContractError::VouchDepthExceeded => (
            String::from_str(env, "VouchDepthExceeded"),
            String::from_str(env, "Vouch chain depth exceeds the maximum allowed depth."),
        ),
        ContractError::InvalidLoanCategory => (
            String::from_str(env, "InvalidLoanCategory"),
            String::from_str(env, "Loan category is not valid or not allowed by protocol config."),
        ),
        ContractError::SectorConcentrationTooHigh => (
            String::from_str(env, "SectorConcentrationTooHigh"),
            String::from_str(env, "Collateral sector concentration exceeds the diversification cap."),
        ),
        ContractError::LoanPurposeNotAllowed => (
            String::from_str(env, "LoanPurposeNotAllowed"),
            String::from_str(env, "Loan purpose is not on the approved list."),
        ),
        ContractError::RestructureRequestNotFound => (
            String::from_str(env, "RestructureRequestNotFound"),
            String::from_str(env, "Loan restructure request not found. Verify the request ID."),
        ),
        ContractError::RestructureAlreadyPending => (
            String::from_str(env, "RestructureAlreadyPending"),
            String::from_str(env, "A restructure request is already pending for this loan."),
        ),
        ContractError::DisputeNotFound => (
            String::from_str(env, "DisputeNotFound"),
            String::from_str(env, "Dispute record not found. Verify the dispute ID."),
        ),
        ContractError::DisputeAlreadyResolved => (
            String::from_str(env, "DisputeAlreadyResolved"),
            String::from_str(env, "Dispute has already been resolved."),
        ),
        ContractError::DisputeWindowExpired => (
            String::from_str(env, "DisputeWindowExpired"),
            String::from_str(env, "The dispute filing window has expired for this loan."),
        ),
        ContractError::FunctionPaused => (
            String::from_str(env, "FunctionPaused"),
            String::from_str(env, "This specific function has been paused by an admin."),
        ),
        ContractError::InvalidAdminThreshold => (
            String::from_str(env, "InvalidAdminThreshold"),
            String::from_str(env, "Admin threshold is 0 or exceeds the number of admins. Set between 1 and len(admins)."),
        ),
        ContractError::StakeLimitExceeded => (
            String::from_str(env, "StakeLimitExceeded"),
            String::from_str(env, "Voucher's total staked amount would exceed the per-voucher stake limit."),
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────

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
    QuorumNotMet = 34,
    AlreadyRepaid = 35,
    // #684: Admin Delegation
    PermissionNotDelegated = 36,
    // #685: Admin Veto Power
    ProposalVetoed = 37,
    /// Voucher and borrower must be different addresses.
    SelfVouchNotAllowed = 38,
    InvalidBps = 39,
    DuplicateToken = 40,
    // Task 1: Loan Cancellation
    LoanNotCancellable = 41,
    CancellationWindowExpired = 42,
    // Task 2: Large Loan Multi-Signature
    LoanTooLarge = 43,
    LargeLoanPendingApproval = 44,
    LargeLoanNotApproved = 45,
    LargeLoanDelayNotElapsed = 46,
    LargeLoanAlreadyExecuted = 47,
    // Task 3: Circular Vouch Detection
    CircularVouchDetected = 48,
    VouchDepthExceeded = 49,
    // Task 4: Loan Category
    InvalidLoanCategory = 50,
    // #642: Collateral Diversification
    SectorConcentrationTooHigh = 51,
    // #643: Loan Purpose Validation
    LoanPurposeNotAllowed = 52,
    // #645: Loan Restructuring
    RestructureRequestNotFound = 53,
    RestructureAlreadyPending = 54,
    // Dispute mechanism
    DisputeNotFound = 55,
    DisputeAlreadyResolved = 56,
    DisputeWindowExpired = 57,
    // Granular pause
    FunctionPaused = 58,
    // Admin config
    InvalidAdminThreshold = 59,
    // Voucher stake limit
    StakeLimitExceeded = 60,
}
