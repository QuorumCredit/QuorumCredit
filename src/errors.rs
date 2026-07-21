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
    BorrowerHasActiveLoan = 22,
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
    /// Oracle caller is not the registered oracle contract (#666/#667).
    OracleUnauthorized = 54,
    /// Repayment retry limit has been exceeded (#669).
    MaxRetriesExceeded = 55,
    /// No escrow record found for this borrower (#666/#667).
    NoEscrowFound = 56,
    /// Withdrawal request already queued for this voucher/borrower pair.
    WithdrawalAlreadyQueued = 57,
    /// Slash has already been reversed and cannot be reversed again.
    SlashAlreadyReversed = 58,
    /// Caller has exceeded the configured rate limit.
    RateLimitExceeded = 59,
    /// Caller does not have the required role or permission.
    PermissionDenied = 60,
    /// Cryptographic proof validation failed.
    InvalidProof = 61,
    /// Arithmetic overflow or underflow occurred.
    ArithmeticError = 62,
    /// No rollback snapshot found for the requested deployment index (#744).
    RollbackSnapshotNotFound = 63,
    /// Admin address is not on the whitelist.
    AdminNotWhitelisted = 64,
    /// Admin address is on the blacklist.
    AdminBlacklisted = 65,
    /// Reentrancy detected — a guarded function was re-entered before the lock was released.
    Reentrancy = 66,
    /// Borrower is immune from being slashed (e.g. repaid within grace period).
    BorrowerImmune = 67,
    /// Target admin has already been revoked and cannot be revoked again.
    AdminAlreadyRevoked = 68,
    /// The target of revocation is not a current admin.
    AdminNotFound = 69,
    /// No slash record found for the given slash ID.
    SlashRecordNotFound = 70,
    /// Refinancing was attempted without any outstanding balance to settle.
    RefinanceNoOutstanding = 71,
    /// Loan has already been fully repaid.
    AlreadyRepaid = 72,
    /// Loan forbearance not found.
    ForbearanceNotFound = 73,
    /// Loan forbearance is not active.
    ForbearanceNotActive = 74,
    /// Loan is currently in forbearance.
    LoanInForbearance = 75,
    /// Maximum forbearance periods reached.
    MaxForbearanceExceeded = 76,
    /// Dynamic rate configuration is invalid.
    InvalidDynamicRateConfig = 77,
    /// Loan amount exceeds maximum loan-to-stake ratio.
    LoanExceedsMaxRatio = 78,
    /// Co-borrower has already been added to this loan.
    CoBorrowerAlreadyAdded = 79,
    /// Maximum co-borrowers per loan exceeded.
    MaxCoBorrowersExceeded = 80,
    /// Borrower cannot add themselves as a co-borrower.
    SelfCoBorrowerNotAllowed = 81,
    /// Insufficient repayment amount.
    InsufficientRepayment = 82,
    /// No queued withdrawal found for this voucher/borrower pair.
    NoQueuedWithdrawal = 83,
    /// Cooldown period has not yet expired.
    CooldownNotExpired = 84,
    /// Vote delegation cycle detected (circular delegation).
    CircularDelegation = 85,
    /// Vote delegation not found for this voucher.
    DelegationNotFound = 86,
    /// A write operation was attempted while the contract is in the Thawing state.
    ContractThawing = 87,
    /// Syndication not found.
    SyndicationNotFound = 88,
    /// Syndication member not found.
    SyndicationMemberNotFound = 89,
    /// Syndication already has a loan.
    SyndicationHasLoan = 90,
    /// Syndication is not in the correct status.
    InvalidSyndicationStatus = 91,
    /// Syndication member already exists.
    SyndicationMemberExists = 92,
    /// Syndication has insufficient approvals.
    InsufficientSyndicationApprovals = 93,
    /// Syndication has too many members.
    SyndicationMaxMembersExceeded = 94,
    /// Syndication has too few members.
    SyndicationMinMembersNotMet = 95,
    /// Invalid syndication share percentage.
    InvalidSyndicationShare = 96,
    /// Syndication configuration is invalid.
    InvalidSyndicationConfig = 97,
    /// The chain_id used in a cross-chain vouch is not registered or is inactive.
    InvalidChain = 98,
    /// A bridge for this chain_id has already been registered.
    BridgeAlreadyRegistered = 99,
    /// No Ed25519 verification key is configured for the origin chain.
    BridgeNotConfigured = 100,
    /// The origin/destination chain combination is invalid.
    InvalidBridgeChain = 101,
    /// This origin-chain nonce has already been consumed.
    ReplayAttackDetected = 102,
    /// The attestation is outside the accepted freshness window.
    AttestationExpired = 103,
    /// The attestation timestamp is too far ahead of the ledger clock.
    AttestationFromFuture = 104,
    /// This canonical loan has already moved its reputation to another chain.
    ReputationAlreadySpent = 105,
    /// A newer reputation attestation has already been applied.
    StaleBridgeAttestation = 106,
    /// Governance proposal has already been approved.
    ProposalAlreadyApproved = 107,
    /// Governance proposal has expired.
    ProposalExpired = 108,
    /// Governance proposal timelock delay has not elapsed.
    TimelockDelayNotElapsed = 109,
    /// Governance proposal execution window has passed.
    ExecutionWindowPassed = 110,
    /// Governance action is invalid or not supported.
    InvalidGovernanceAction = 111,
    /// Credit score calculation failed.
    CreditScoreCalculationFailed = 112,
    /// Invalid credit score tier.
    InvalidCreditTier = 113,
    /// Credit score not found for borrower.
    CreditScoreNotFound = 114,
    /// Credit score configuration is invalid.
    InvalidCreditConfig = 115,
    /// No slash escrow found for this borrower.
    AppealNotFound = 116,
    /// Voucher has already voted on this appeal.
    AppealAlreadyVoted = 117,
    /// Appeal quorum not met to overturn slash.
    AppealQuorumNotMet = 118,
    /// Escrow period has expired; appeal can no longer be filed.
    EscrowExpired = 119,
    /// Emergency cooldown bypass is not authorised for this voucher.
    EmergencyBypassNotAuthorised = 120,
    /// Cooldown bypass request already exists.
    CooldownBypassAlreadyRequested = 121,
    /// Cooldown bypass request not found.
    CooldownBypassNotFound = 122,
    /// Cooldown bypass has already been approved.
    CooldownBypassAlreadyApproved = 123,
    /// Insufficient admin approvals for cooldown bypass.
    CooldownBypassInsufficientApprovals = 124,
    /// Cross-collateral pool not found.
    CollateralPoolNotFound = 125,
    /// Cross-collateral pool is already active.
    CollateralPoolActive = 126,
    /// Caller is not a member of the specified collateral pool.
    NotPoolMember = 127,
    /// Gradual-unstake schedule not found.
    GradualUnstakeNotFound = 128,
    /// A gradual-unstake schedule is already active.
    GradualUnstakeAlreadyActive = 129,
    /// The next instalment is not yet due.
    GradualUnstakeNotDue = 130,
    /// Loan extension request already pending.
    ExtensionAlreadyRequested = 131,
    /// Maximum number of extensions per loan has been reached.
    MaxExtensionsReached = 132,
    /// Caller does not have permission to view this loan.
    LoanPrivacyRestricted = 133,
    /// Insurance pool is not connected to this loan.
    InsuranceNotLinked = 134,
    /// No relay verification key is configured for the source chain.
    RelayKeyNotConfigured = 135,
    /// Relay chain id is zero or otherwise invalid.
    InvalidRelayChain = 136,
    /// A relay attestation reused an already-consumed nonce.
    RelayReplayDetected = 137,
    /// The relay attestation is older than the freshness window allows.
    RelayEventExpired = 138,
    /// The relay attestation is timestamped too far in the future.
    RelayEventFromFuture = 139,
    /// A relay event with this (source chain, sequence) was already processed.
    RelayEventAlreadyProcessed = 140,
    /// A relay acknowledgement tried to move the cursor backwards.
    RelayAckRegression = 141,
}
