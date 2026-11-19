//! Flash loan functionality for QuorumCredit (Issue #1183).
//!
//! Flash loans enable atomic borrowing and repayment within a single transaction.
//! Key characteristics:
//! - Instant capital access without collateral
//! - Must be repaid (with fee) within the same transaction block
//! - Reverts entire transaction if repayment fails
//! - Enables atomic arbitrage and liquidation operations
//!
//! ## Flash Loan Flow
//! 1. Borrower calls `flash_loan()` with amount and callback contract
//! 2. Protocol transfers amount to callback contract
//! 3. Callback contract executes arbitrary logic (trading, liquidation, etc.)
//! 4. Callback must call `repay_flash_loan()` with principal + 0.05% fee
//! 5. If repayment fails, entire transaction reverts
//!
//! ## Anti-abuse guards
//! - **Callback allowlist**: `callback_contract` must be on the admin-managed
//!   allowlist (`set_flash_loan_callback_allowed`) or the loan is rejected
//!   before any funds move. Without this, a caller could point `flash_loan`
//!   at an arbitrary, unaudited contract deployed to re-enter other protocol
//!   functions during the loan window using the borrowed liquidity.
//! - **Per-caller cooldown**: consecutive flash loans to the same
//!   `callback_contract` must be at least `FLASH_LOAN_MIN_INTERVAL_SECS`
//!   apart, so a single allowlisted integration can't spam many small loans
//!   in quick succession to grind `FlashLoanStats` or probe for exploitable
//!   states.

#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used))]

extern crate alloc;

use crate::errors::ContractError;
use crate::helpers::{config, require_admin_approval, require_not_paused, token_client};
use crate::types::{DataKey, Config};
use soroban_sdk::{
    contracttype, symbol_short, Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

/// Flash loan fee in basis points (0.05% = 5 bps)
pub const FLASH_LOAN_FEE_BPS: i128 = 5;

/// Name of the function the borrower's callback contract must export. It is
/// invoked as `on_flash_loan(amount, fee, callback_data)` and must repay
/// `amount + fee` to this contract before it returns.
pub const FLASH_LOAN_CALLBACK_FN: &str = "on_flash_loan";

/// Per-contract borrowing cap to prevent abuse.
pub const DEFAULT_PER_CONTRACT_FLASH_CAP: i128 = 10_000_000_000; // 1000 XLM

/// Maximum flash loan amount to prevent systemic risk.
pub const MAX_FLASH_LOAN_AMOUNT: i128 = 1_000_000_000_000; // 100,000 XLM

/// Minimum flash loan amount
pub const MIN_FLASH_LOAN_AMOUNT: i128 = 1_000_000; // 0.1 XLM

/// Maximum number of flash loan records kept in the hot `FlashLoanHistory`.
/// Older records are moved to per-period archive buckets, not dropped.
pub const FLASH_LOAN_HISTORY_MAX_RECORDS: u32 = 10_000;

/// Length of one archive period, in seconds. Evicted records are grouped into
/// buckets keyed by `timestamp / FLASH_LOAN_ARCHIVE_PERIOD_SECS` (one day).
pub const FLASH_LOAN_ARCHIVE_PERIOD_SECS: u64 = 86_400;

/// Minimum interval, in ledger seconds, required between two flash loans to
/// the same `callback_contract`. Bounds how fast a single caller can spam
/// flash loans regardless of amount.
pub const FLASH_LOAN_MIN_INTERVAL_SECS: u64 = 60;

/// Records flash loan activity for analytics.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FlashLoanRecord {
    /// Borrower contract address
    pub borrower: Address,
    /// Amount borrowed in stroops
    pub amount: i128,
    /// Fee collected in stroops
    pub fee: i128,
    /// Timestamp of the flash loan
    pub timestamp: u64,
    /// Token used for the loan
    pub token: Address,
}

/// Flash loan event for off-chain tracking.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FlashLoanEvent {
    /// Borrower address
    pub borrower: Address,
    /// Loan amount
    pub amount: i128,
    /// Fee amount
    pub fee: i128,
    /// Successful completion
    pub success: bool,
    /// Block timestamp
    pub timestamp: u64,
}

/// Flash loan statistics for governance.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FlashLoanStats {
    /// Total flash loans originated
    pub total_volume: i128,
    /// Total fees collected
    pub total_fees: i128,
    /// Number of flash loans
    pub loan_count: u64,
    /// Current period fees (resets daily)
    pub period_fees: i128,
}

/// Borrow cap per contract address.
#[contracttype]
#[derive(Clone, Debug)]
pub struct PerContractCap {
    /// Contract address
    pub contract: Address,
    /// Current amount borrowed in this period
    pub borrowed_amount: i128,
    /// Max borrowable amount
    pub cap: i128,
    /// Last reset timestamp
    pub last_reset: u64,
}

/// Initialize flash loan subsystem (called once during contract initialization).
pub fn initialize_flash_loans(env: &Env) -> Result<(), ContractError> {
    // Initialize flash loan statistics storage
    let stats = FlashLoanStats {
        total_volume: 0,
        total_fees: 0,
        loan_count: 0,
        period_fees: 0,
    };

    env.storage()
        .persistent()
        .set(&DataKey::FlashLoanStats, &stats);

    Ok(())
}

/// Execute a flash loan transaction.
///
/// # Arguments
/// * `env` - the Soroban environment
/// * `amount` - flash loan amount in stroops (0.1-100,000 XLM)
/// * `callback_contract` - the contract to call with the borrowed funds
/// * `callback_data` - data to pass to the callback contract
///
/// # Returns
/// `Ok(())` if flash loan succeeded and was repaid; `Err` otherwise.
///
/// # Panics
/// The entire transaction reverts if:
/// - Contract balance is insufficient
/// - Amount exceeds limits or caps
/// - Callback contract doesn't repay principal + fee in same transaction
pub fn flash_loan(
    env: &Env,
    amount: i128,
    callback_contract: Address,
    callback_data: BytesN<32>,
) -> Result<(), ContractError> {
    require_not_paused(env)?;

    // Validate amount
    if amount < MIN_FLASH_LOAN_AMOUNT {
        return Err(ContractError::InvalidAmount);
    }
    if amount > MAX_FLASH_LOAN_AMOUNT {
        return Err(ContractError::InvalidAmount);
    }

    // Reject unregistered callback contracts before any funds move.
    if !is_flash_loan_callback_allowed(env, &callback_contract) {
        return Err(ContractError::FlashLoanCallbackNotAllowlisted);
    }

    // Enforce a minimum interval between loans to the same callback contract.
    check_flash_loan_cooldown(env, &callback_contract)?;

    let cfg = config(env);

    // Check contract balance
    let contract_id = env.current_contract_address();
    let balance = token_client(env).balance(&contract_id);
    if balance < amount {
        return Err(ContractError::InsufficientFunds);
    }

    // Check per-contract rate limit
    validate_per_contract_cap(env, &callback_contract, amount)?;

    // Calculate fee (0.05% = 5 bps)
    let fee = (amount * FLASH_LOAN_FEE_BPS) / 10_000;

    // Transfer amount to callback contract
    token_client(env).transfer(&contract_id, &callback_contract, &amount);

    // Invoke the callback so it can use the borrowed funds. If the callback
    // panics, the whole transaction reverts, including the transfer above.
    let args = Vec::from_array(
        env,
        [
            amount.into_val(env),
            fee.into_val(env),
            callback_data.into_val(env),
        ],
    );
    let _: () = env.invoke_contract(
        &callback_contract,
        &Symbol::new(env, FLASH_LOAN_CALLBACK_FN),
        args,
    );

    // Enforce repayment of principal AND fee. The pre-loan balance is the
    // reference point: a callback that returns only the principal leaves the
    // balance at `balance`, which is below `balance + fee`. This runs before
    // any bookkeeping is written, so a failed repayment leaves FlashLoanStats,
    // the loan history, and the per-callback cooldown untouched.
    let required_balance = balance
        .checked_add(fee)
        .ok_or(ContractError::ArithmeticError)?;
    let balance_after = token_client(env).balance(&contract_id);
    if balance_after < required_balance {
        return Err(ContractError::FlashLoanNotRepaid);
    }

    // Record the flash loan for analytics
    let record = FlashLoanRecord {
        borrower: callback_contract.clone(),
        amount,
        fee,
        timestamp: env.ledger().timestamp(),
        token: cfg.token.clone(),
    };

    add_flash_loan_record(env, &record)?;

    // Update statistics
    update_flash_loan_stats(env, amount, fee)?;

    // Record this timestamp so the next loan to this callback contract
    // must wait out FLASH_LOAN_MIN_INTERVAL_SECS.
    env.storage().persistent().set(
        &DataKey::FlashLoanLastTimestamp(callback_contract.clone()),
        &env.ledger().timestamp(),
    );

    Ok(())
}

/// Repay a flash loan (called by the callback contract).
///
/// # Arguments
/// * `env` - the Soroban environment
/// * `principal` - the original loan amount
/// * `fee` - the 0.05% fee
pub fn repay_flash_loan(
    env: &Env,
    borrower: Address,
    principal: i128,
    fee: i128,
) -> Result<(), ContractError> {
    // Validate fee calculation. Checked arithmetic keeps an oversized principal
    // from panicking inside the contract; it is reported as a typed error.
    let expected_fee = principal
        .checked_mul(FLASH_LOAN_FEE_BPS)
        .ok_or(ContractError::ArithmeticError)?
        / 10_000;
    if fee != expected_fee {
        return Err(ContractError::InvalidFeeAmount);
    }
    let total = principal
        .checked_add(fee)
        .ok_or(ContractError::ArithmeticError)?;

    // Transfer repayment from borrower to contract
    let contract_id = env.current_contract_address();
    token_client(env).transfer(&borrower, &contract_id, &total);

    Ok(())
}

/// Get flash loan statistics.
pub fn get_flash_loan_stats(env: &Env) -> Result<FlashLoanStats, ContractError> {
    env.storage()
        .persistent()
        .get(&DataKey::FlashLoanStats)
        .ok_or(ContractError::NotFound)
}

/// Get total flash loan volume.
pub fn get_total_flash_loan_volume(env: &Env) -> Result<i128, ContractError> {
    let stats = get_flash_loan_stats(env)?;
    Ok(stats.total_volume)
}

/// Get total flash loan fees collected.
pub fn get_total_flash_loan_fees(env: &Env) -> Result<i128, ContractError> {
    let stats = get_flash_loan_stats(env)?;
    Ok(stats.total_fees)
}

/// Get flash loan count.
pub fn get_flash_loan_count(env: &Env) -> Result<u64, ContractError> {
    let stats = get_flash_loan_stats(env)?;
    Ok(stats.loan_count)
}

/// Check if a contract is at its borrowing cap.
pub fn check_per_contract_cap(
    env: &Env,
    contract: &Address,
) -> Result<i128, ContractError> {
    let cap_data: Option<PerContractCap> = env
        .storage()
        .persistent()
        .get(&DataKey::FlashLoanPerContractCap(contract.clone()));

    if let Some(cap) = cap_data {
        Ok(cap.cap - cap.borrowed_amount)
    } else {
        Ok(DEFAULT_PER_CONTRACT_FLASH_CAP)
    }
}

/// Admin: allow or revoke a callback contract's ability to receive flash loans.
/// Unregistered contracts are rejected by `flash_loan` before any funds move.
pub fn set_flash_loan_callback_allowed(
    env: &Env,
    admin_signers: Vec<Address>,
    callback_contract: Address,
    allowed: bool,
) -> Result<(), ContractError> {
    require_admin_approval(env, &admin_signers);

    env.storage().persistent().set(
        &DataKey::AllowedFlashLoanCallbacks(callback_contract.clone()),
        &allowed,
    );

    env.events().publish(
        (symbol_short!("flashln"), symbol_short!("allowlist")),
        (callback_contract, allowed),
    );

    Ok(())
}

/// Whether a callback contract is on the admin-managed flash loan allowlist.
/// Defaults to `false` (deny) for any contract never explicitly allowlisted.
pub fn is_flash_loan_callback_allowed(env: &Env, callback_contract: &Address) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::AllowedFlashLoanCallbacks(callback_contract.clone()))
        .unwrap_or(false)
}

// ── Internal functions ────────────────────────────────────────────────────────

/// Rejects a flash loan to `callback_contract` if one was issued to it within
/// the last `FLASH_LOAN_MIN_INTERVAL_SECS`.
fn check_flash_loan_cooldown(env: &Env, callback_contract: &Address) -> Result<(), ContractError> {
    let last: Option<u64> = env
        .storage()
        .persistent()
        .get(&DataKey::FlashLoanLastTimestamp(callback_contract.clone()));

    if let Some(last_timestamp) = last {
        let now = env.ledger().timestamp();
        if now.saturating_sub(last_timestamp) < FLASH_LOAN_MIN_INTERVAL_SECS {
            return Err(ContractError::FlashLoanCooldownActive);
        }
    }

    Ok(())
}

fn validate_per_contract_cap(
    env: &Env,
    contract: &Address,
    amount: i128,
) -> Result<(), ContractError> {
    let available = check_per_contract_cap(env, contract)?;

    if amount > available {
        return Err(ContractError::FlashLoanCapExceeded);
    }

    Ok(())
}

fn add_flash_loan_record(env: &Env, record: &FlashLoanRecord) -> Result<(), ContractError> {
    let mut records: Vec<FlashLoanRecord> = env
        .storage()
        .persistent()
        .get(&DataKey::FlashLoanHistory)
        .unwrap_or_else(|| Vec::new(&env));

    records.push_back(record.clone());

    // Keep only the newest FLASH_LOAN_HISTORY_MAX_RECORDS in the hot history.
    // Evicted records move to a per-period archive bucket. Aggregate figures
    // live in FlashLoanStats and are never derived from history, so archival
    // does not change them.
    while records.len() > FLASH_LOAN_HISTORY_MAX_RECORDS {
        if let Some(old) = records.pop_front() {
            archive_flash_loan_record(env, &old);
        }
    }

    env.storage()
        .persistent()
        .set(&DataKey::FlashLoanHistory, &records);

    Ok(())
}

fn archive_flash_loan_record(env: &Env, record: &FlashLoanRecord) {
    let period = record.timestamp / FLASH_LOAN_ARCHIVE_PERIOD_SECS;
    let key = DataKey::FlashLoanArchive(period);
    let mut bucket: Vec<FlashLoanRecord> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(&env));
    bucket.push_back(record.clone());
    env.storage().persistent().set(&key, &bucket);
}

/// Archived flash loan records for one period (`timestamp / FLASH_LOAN_ARCHIVE_PERIOD_SECS`).
pub fn get_flash_loan_archive(env: &Env, period: u64) -> Vec<FlashLoanRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::FlashLoanArchive(period))
        .unwrap_or_else(|| Vec::new(env))
}

fn update_flash_loan_stats(env: &Env, amount: i128, fee: i128) -> Result<(), ContractError> {
    let mut stats: FlashLoanStats = env
        .storage()
        .persistent()
        .get(&DataKey::FlashLoanStats)
        .ok_or(ContractError::NotFound)?;

    stats.total_volume = stats
        .total_volume
        .checked_add(amount)
        .ok_or(ContractError::ArithmeticError)?;
    stats.total_fees = stats
        .total_fees
        .checked_add(fee)
        .ok_or(ContractError::ArithmeticError)?;
    stats.loan_count = stats
        .loan_count
        .checked_add(1)
        .ok_or(ContractError::ArithmeticError)?;
    stats.period_fees = stats
        .period_fees
        .checked_add(fee)
        .ok_or(ContractError::ArithmeticError)?;

    env.storage()
        .persistent()
        .set(&DataKey::FlashLoanStats, &stats);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_flash_loan_fee_calculation() {
        let principal = 1_000_000;
        let expected_fee = (principal * FLASH_LOAN_FEE_BPS) / 10_000;
        // 5 basis points (0.05%) of 1,000,000 stroops = 500 stroops
        assert_eq!(expected_fee, 500);
    }

    #[test]
    fn test_flash_loan_amount_bounds() {
        assert!(MIN_FLASH_LOAN_AMOUNT > 0);
        assert!(MAX_FLASH_LOAN_AMOUNT > MIN_FLASH_LOAN_AMOUNT);
        assert!(DEFAULT_PER_CONTRACT_FLASH_CAP > 0);
    }

    #[test]
    fn test_large_loan_fee() {
        let principal = MAX_FLASH_LOAN_AMOUNT;
        let fee = (principal * FLASH_LOAN_FEE_BPS) / 10_000;
        assert!(fee > 0);
        assert!(principal + fee > principal);
    }

    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::token::StellarAssetClient;
    use soroban_sdk::{contract, contractimpl};

    /// Test double for a borrower callback. `init` records the token and the
    /// lender (the QuorumCredit contract). `on_flash_loan` repays the
    /// principal, and also the fee when `repay_fee` is true.
    #[contract]
    pub struct MockFlashCallback;

    #[contracttype]
    #[derive(Clone)]
    enum MockKey {
        Token,
        Lender,
        RepayFee,
    }

    #[contractimpl]
    impl MockFlashCallback {
        pub fn init(env: Env, token: Address, lender: Address, repay_fee: bool) {
            env.storage().instance().set(&MockKey::Token, &token);
            env.storage().instance().set(&MockKey::Lender, &lender);
            env.storage().instance().set(&MockKey::RepayFee, &repay_fee);
        }

        pub fn on_flash_loan(env: Env, amount: i128, fee: i128, _data: BytesN<32>) {
            let token: Address = env.storage().instance().get(&MockKey::Token).unwrap();
            let lender: Address = env.storage().instance().get(&MockKey::Lender).unwrap();
            let repay_fee: bool = env.storage().instance().get(&MockKey::RepayFee).unwrap();
            let repay = if repay_fee { amount + fee } else { amount };
            soroban_sdk::token::Client::new(&env, &token).transfer(
                &env.current_contract_address(),
                &lender,
                &repay,
            );
        }
    }

    fn register_mock_callback(
        env: &Env,
        token: &Address,
        lender: &Address,
        repay_fee: bool,
    ) -> Address {
        let id = env.register_contract(None, MockFlashCallback);
        MockFlashCallbackClient::new(env, &id).init(token, lender, &repay_fee);
        id
    }

    #[test]
    fn test_full_repayment_with_fee_succeeds() {
        let env = Env::default();
        let (contract_id, admin, token) = setup_contract(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let callback = register_mock_callback(&env, &token, &contract_id, true);
        // The callback receives the principal and needs the fee on hand to repay it.
        StellarAssetClient::new(&env, &token).mint(&callback, &500);
        let callback_data = BytesN::from_array(&env, &[0u8; 32]);
        let token_client = soroban_sdk::token::Client::new(&env, &token);
        let before = token_client.balance(&contract_id);

        env.as_contract(&contract_id, || {
            set_flash_loan_callback_allowed(&env, admin_signers.clone(), callback.clone(), true)
                .unwrap();
            flash_loan(&env, MIN_FLASH_LOAN_AMOUNT, callback.clone(), callback_data.clone())
                .unwrap();

            let stats = get_flash_loan_stats(&env).unwrap();
            assert_eq!(stats.loan_count, 1);
            assert_eq!(stats.total_fees, 500);
        });

        assert_eq!(token_client.balance(&contract_id), before + 500);
    }

    #[test]
    fn test_principal_only_repayment_is_rejected_with_not_repaid() {
        // Returning the principal without the fee must fail with
        // FlashLoanNotRepaid. Before the fix, the balance-only check passed here.
        let env = Env::default();
        let (contract_id, admin, token) = setup_contract(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let callback = register_mock_callback(&env, &token, &contract_id, false);
        let callback_data = BytesN::from_array(&env, &[0u8; 32]);

        env.as_contract(&contract_id, || {
            set_flash_loan_callback_allowed(&env, admin_signers.clone(), callback.clone(), true)
                .unwrap();
            let err = flash_loan(&env, MIN_FLASH_LOAN_AMOUNT, callback.clone(), callback_data)
                .unwrap_err();
            assert_eq!(err, ContractError::FlashLoanNotRepaid);

            let stats = get_flash_loan_stats(&env).unwrap();
            assert_eq!(stats.loan_count, 0);
        });
    }

    fn setup_contract(env: &Env) -> (Address, Address, Address) {
        env.mock_all_auths();
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admins = Vec::from_array(env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000_000);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        (contract_id, admin, token_id.address())
    }

    #[test]
    fn test_callback_must_be_allowlisted_before_funds_move() {
        // A callback contract that was never admin-allowlisted is rejected
        // up front, before any balance changes or stats updates.
        let env = Env::default();
        let (contract_id, _admin, token) = setup_contract(&env);
        let callback = Address::generate(&env);
        let callback_data = BytesN::from_array(&env, &[0u8; 32]);

        env.as_contract(&contract_id, || {
            let err = flash_loan(&env, MIN_FLASH_LOAN_AMOUNT, callback.clone(), callback_data)
                .unwrap_err();
            assert_eq!(err, ContractError::FlashLoanCallbackNotAllowlisted);

            let stats = get_flash_loan_stats(&env).unwrap();
            assert_eq!(stats.loan_count, 0);
        });

        let token_client = soroban_sdk::token::Client::new(&env, &token);
        assert_eq!(token_client.balance(&contract_id), 10_000_000_000);
    }

    #[test]
    fn test_failed_repayment_leaves_loan_state_unchanged() {
        // The callback never repays, so the post-loan balance check fails.
        // The failure must return a typed error and must not write any loan
        // bookkeeping: stats, history, or the per-callback cooldown timestamp.
        let env = Env::default();
        let (contract_id, admin, token) = setup_contract(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let callback = register_mock_callback(&env, &token, &contract_id, false);
        let callback_data = BytesN::from_array(&env, &[0u8; 32]);

        env.as_contract(&contract_id, || {
            set_flash_loan_callback_allowed(&env, admin_signers.clone(), callback.clone(), true)
                .unwrap();

            let err = flash_loan(&env, MIN_FLASH_LOAN_AMOUNT, callback.clone(), callback_data)
                .unwrap_err();
            assert_eq!(err, ContractError::FlashLoanNotRepaid);

            let stats = get_flash_loan_stats(&env).unwrap();
            assert_eq!(stats.loan_count, 0);
            assert_eq!(stats.total_volume, 0);
            assert_eq!(stats.total_fees, 0);
            assert_eq!(stats.period_fees, 0);

            let history: Vec<FlashLoanRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::FlashLoanHistory)
                .unwrap_or(Vec::new(&env));
            assert_eq!(history.len(), 0);

            assert!(env
                .storage()
                .persistent()
                .get::<_, u64>(&DataKey::FlashLoanLastTimestamp(callback.clone()))
                .is_none());
        });
    }

    #[test]
    fn test_repay_rejects_oversized_principal_without_panicking() {
        // A principal whose fee would overflow i128 must return a typed error
        // rather than panic.
        let env = Env::default();
        let (contract_id, _admin, _token) = setup_contract(&env);
        let borrower = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let err = repay_flash_loan(&env, borrower.clone(), i128::MAX, 0).unwrap_err();
            assert_eq!(err, ContractError::ArithmeticError);
        });
    }

    #[test]
    fn test_cooldown_blocks_rapid_repeat_loans_without_touching_stats() {
        // A second flash loan to the same callback contract within
        // FLASH_LOAN_MIN_INTERVAL_SECS of the first is rejected without
        // mutating FlashLoanStats or moving funds; once the interval has
        // elapsed, the cooldown check clears.
        let env = Env::default();
        let (contract_id, admin, token) = setup_contract(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let callback = Address::generate(&env);
        let callback_data = BytesN::from_array(&env, &[0u8; 32]);

        env.as_contract(&contract_id, || {
            set_flash_loan_callback_allowed(&env, admin_signers.clone(), callback.clone(), true)
                .unwrap();
            assert!(is_flash_loan_callback_allowed(&env, &callback));

            // Simulate a flash loan having just been taken by this callback.
            env.storage().persistent().set(
                &DataKey::FlashLoanLastTimestamp(callback.clone()),
                &env.ledger().timestamp(),
            );

            let err = flash_loan(&env, MIN_FLASH_LOAN_AMOUNT, callback.clone(), callback_data)
                .unwrap_err();
            assert_eq!(err, ContractError::FlashLoanCooldownActive);

            let stats = get_flash_loan_stats(&env).unwrap();
            assert_eq!(stats.loan_count, 0);
            assert_eq!(stats.period_fees, 0);
        });

        let token_client = soroban_sdk::token::Client::new(&env, &token);
        assert_eq!(token_client.balance(&contract_id), 10_000_000_000);

        // Advance past the cooldown window; the guard now clears.
        env.ledger()
            .set_timestamp(env.ledger().timestamp() + FLASH_LOAN_MIN_INTERVAL_SECS + 1);

        env.as_contract(&contract_id, || {
            assert!(check_flash_loan_cooldown(&env, &callback).is_ok());
        });
    }

    #[test]
    fn test_archiving_history_leaves_stats_unchanged() {
        // Filling the hot history past its cap moves the oldest record into the
        // period archive. FlashLoanStats totals must be identical before and
        // after that eviction.
        let env = Env::default();
        // Seeding the full history is a large write; lift the test budget.
        env.cost_estimate().budget().reset_unlimited();
        let (contract_id, _admin, token) = setup_contract(&env);
        let borrower = Address::generate(&env);

        env.as_contract(&contract_id, || {
            let seed = FlashLoanRecord {
                borrower: borrower.clone(),
                amount: 100,
                fee: 1,
                timestamp: 0,
                token: token.clone(),
            };
            let mut full: Vec<FlashLoanRecord> = Vec::new(&env);
            for _ in 0..FLASH_LOAN_HISTORY_MAX_RECORDS {
                full.push_back(seed.clone());
            }
            env.storage()
                .persistent()
                .set(&DataKey::FlashLoanHistory, &full);

            update_flash_loan_stats(&env, 100, 1).unwrap();
            let before = get_flash_loan_stats(&env).unwrap();

            let newest = FlashLoanRecord {
                timestamp: 1,
                amount: 200,
                ..seed.clone()
            };
            add_flash_loan_record(&env, &newest).unwrap();

            let after = get_flash_loan_stats(&env).unwrap();
            assert_eq!(after.total_volume, before.total_volume);
            assert_eq!(after.total_fees, before.total_fees);
            assert_eq!(after.loan_count, before.loan_count);
            assert_eq!(after.period_fees, before.period_fees);

            let history: Vec<FlashLoanRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::FlashLoanHistory)
                .unwrap();
            assert_eq!(history.len(), FLASH_LOAN_HISTORY_MAX_RECORDS);

            let archived = get_flash_loan_archive(&env, 0);
            assert_eq!(archived.len(), 1);
            assert_eq!(archived.get(0).unwrap().amount, 100);
        });
    }

    #[test]
    fn test_revoking_allowlist_blocks_future_loans() {
        let env = Env::default();
        let (contract_id, admin, _token) = setup_contract(&env);
        let admin_signers = Vec::from_array(&env, [admin.clone()]);
        let callback = Address::generate(&env);

        env.as_contract(&contract_id, || {
            set_flash_loan_callback_allowed(&env, admin_signers.clone(), callback.clone(), true)
                .unwrap();
            assert!(is_flash_loan_callback_allowed(&env, &callback));
        });

        env.as_contract(&contract_id, || {
            set_flash_loan_callback_allowed(&env, admin_signers.clone(), callback.clone(), false)
                .unwrap();
            assert!(!is_flash_loan_callback_allowed(&env, &callback));
        });
    }
}
