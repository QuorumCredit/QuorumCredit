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
/// Extracted into a free function so it can be used both in `ApiError` and in
/// off-chain tooling that imports this crate.
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
            String::from_str(env, "Caller is not authorized for this operation. Ensure the correct address signs the transaction."),
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
        ContractError::LoanBelowMinAmount => (
            String::from_str(env, "LoanBelowMinAmount"),
            String::from_str(env, "Requested loan is below the admin-configured minimum. Request a larger amount."),
        ),
        ContractError::QuorumNotMet => (
            String::from_str(env, "QuorumNotMet"),
            String::from_str(env, "Slash vote quorum not reached. Recruit more voucher votes."),
        ),
        ContractError::DelayNotElapsed => (
            String::from_str(env, "DelayNotElapsed"),
            String::from_str(env, "Required delay has not elapsed. Wait and retry."),
        ),
        ContractError::MaxVouchersPerBorrowerExceeded => (
            String::from_str(env, "MaxVouchersPerBorrowerExceeded"),
            String::from_str(env, "Borrower has reached the maximum number of vouchers allowed."),
        ),
        ContractError::InsufficientVoucherBalance => (
            String::from_str(env, "InsufficientVoucherBalance"),
            String::from_str(env, "Voucher has insufficient token balance to cover the requested stake."),
        ),
        ContractError::SelfVouchNotAllowed => (
            String::from_str(env, "SelfVouchNotAllowed"),
            String::from_str(env, "Voucher and borrower cannot be the same address."),
        ),
        ContractError::DuplicateToken => (
            String::from_str(env, "DuplicateToken"),
            String::from_str(env, "Token is already in the allowed tokens list."),
        ),
        ContractError::InvalidAdminThreshold => (
            String::from_str(env, "InvalidAdminThreshold"),
            String::from_str(env, "Admin threshold is 0 or exceeds the number of admins. Set between 1 and len(admins)."),
        ),
        ContractError::InsufficientYieldReserve => (
            String::from_str(env, "InsufficientYieldReserve"),
            String::from_str(env, "Yield reserve is insufficient to cover promised yield. Admin must pre-fund the reserve."),
        ),
        ContractError::ReminderAlreadySent => (
            String::from_str(env, "ReminderAlreadySent"),
            String::from_str(env, "Repayment reminder has already been sent for this loan."),
        ),
        ContractError::InsurancePoolEmpty => (
            String::from_str(env, "InsurancePoolEmpty"),
            String::from_str(env, "Insurance pool has no funds to cover the claim."),
        ),
        ContractError::InsuranceClaimAlreadyMade => (
            String::from_str(env, "InsuranceClaimAlreadyMade"),
            String::from_str(env, "An insurance claim has already been made for this loan."),
        ),
        ContractError::InvalidBps => (
            String::from_str(env, "InvalidBps"),
            String::from_str(env, "Basis points value is invalid. Must be in range 0–10000."),
        ),
        ContractError::WithdrawalAlreadyQueued => (
            String::from_str(env, "WithdrawalAlreadyQueued"),
            String::from_str(env, "A withdrawal is already queued for this voucher/borrower pair."),
        ),
        ContractError::WithdrawalNotQueued => (
            String::from_str(env, "WithdrawalNotQueued"),
            String::from_str(env, "No queued withdrawal found for this voucher/borrower pair."),
        ),
        ContractError::PartialWithdrawalExceedsCap => (
            String::from_str(env, "PartialWithdrawalExceedsCap"),
            String::from_str(env, "Partial withdrawal amount exceeds the 50% cap."),
        ),
        ContractError::SlashCooldownActive => (
            String::from_str(env, "SlashCooldownActive"),
            String::from_str(env, "Borrower was slashed too recently. Slash cooldown is still active."),
        ),
        ContractError::NotGovernanceParticipant => (
            String::from_str(env, "NotGovernanceParticipant"),
            String::from_str(env, "Caller is not an admin or protocol-token holder eligible to govern."),
        ),
        ContractError::VotingPeriodEnded => (
            String::from_str(env, "VotingPeriodEnded"),
            String::from_str(env, "Governance action is not allowed after the voting period has ended."),
        ),
        ContractError::ProposalNotFound => (
            String::from_str(env, "ProposalNotFound"),
            String::from_str(env, "Governance proposal not found. Verify the proposal ID."),
        ),
        ContractError::ProposalAlreadyFinalized => (
            String::from_str(env, "ProposalAlreadyFinalized"),
            String::from_str(env, "Governance proposal has already been finalized."),
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────

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
    DelayNotElapsed = 36,
    MaxVouchersPerBorrowerExceeded = 37,
    InsufficientVoucherBalance = 38,
    SelfVouchNotAllowed = 39,
    DuplicateToken = 40,
    InvalidAdminThreshold = 41,
    InsufficientYieldReserve = 42,
    ReminderAlreadySent = 43,
    /// Insurance pool has no funds to cover the claim.
    InsurancePoolEmpty = 44,
    /// Insurance claim already made for this loan.
    InsuranceClaimAlreadyMade = 45,
    /// Basis points value is invalid (must be 0–10000).
    InvalidBps = 46,
    /// Withdrawal request already queued for this voucher/borrower pair.
    // NOTE: was erroneously = 46 (same as InvalidBps); corrected to 54 (Issue #109).
    WithdrawalAlreadyQueued = 54,
    /// No queued withdrawal found for this voucher/borrower pair.
    WithdrawalNotQueued = 47,
    /// Partial withdrawal amount exceeds the 50% cap.
    PartialWithdrawalExceedsCap = 48,
    /// Borrower was slashed too recently; slash cooldown is still active.
    SlashCooldownActive = 49,
    /// Caller is not an admin or protocol-token holder allowed to govern.
    NotGovernanceParticipant = 50,
    /// Governance action is not allowed after the voting period has ended.
    VotingPeriodEnded = 51,
    /// Governance proposal was not found.
    ProposalNotFound = 52,
    /// Governance proposal was already finalized.
    ProposalAlreadyFinalized = 53,
}
