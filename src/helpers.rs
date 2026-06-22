use crate::errors::ContractError;
use crate::types::{
    Config, DataKey, LoanRecord, LoanStatus, PauseMode, ThawState,
    MIN_DYNAMIC_SLASH_BPS, MAX_DYNAMIC_SLASH_BPS, HEALTH_THRESHOLD_BPS, BPS_DENOMINATOR,
};
use soroban_sdk::{token, Address, Env, String, Vec};

// ── Reentrancy Guard ──────────────────────────────────────────────────────────

/// Acquires the reentrancy lock. Returns `Err(Reentrancy)` if already locked.
/// Must be paired with `release_lock` at the end of every state-mutating function.
pub fn acquire_lock(env: &Env) -> Result<(), ContractError> {
    let locked: bool = env
        .storage()
        .instance()
        .get(&DataKey::Locked)
        .unwrap_or(false);
    if locked {
        return Err(ContractError::Reentrancy);
    }
    env.storage().instance().set(&DataKey::Locked, &true);
    Ok(())
}

/// Releases the reentrancy lock. Always call this before returning from a guarded function.
pub fn release_lock(env: &Env) {
    env.storage().instance().set(&DataKey::Locked, &false);
}

// ── Pause Check ───────────────────────────────────────────────────────────────

/// Returns the current [`PauseMode`], automatically transitioning from `Thawing`
/// to `None` if the thaw window has expired.
pub fn get_pause_mode(env: &Env) -> PauseMode {
    let mode: PauseMode = env
        .storage()
        .instance()
        .get(&DataKey::PauseMode)
        .unwrap_or(PauseMode::None);

    if mode == PauseMode::Thawing {
        // Auto-transition: if thaw window has elapsed, clear thaw state.
        if let Some(thaw) = env
            .storage()
            .instance()
            .get::<_, ThawState>(&DataKey::ThawState)
        {
            let now = env.ledger().timestamp();
            if now >= thaw.thaw_start_timestamp + thaw.thaw_duration {
                env.storage()
                    .instance()
                    .set(&DataKey::PauseMode, &PauseMode::None);
                env.storage()
                    .instance()
                    .set(&DataKey::Paused, &false);
                env.storage()
                    .instance()
                    .remove(&DataKey::ThawState);
                return PauseMode::None;
            }
        }
    }

    mode
}

/// Blocks all write operations when `Paused`, `Thawing`, or emergency-paused.
/// This is the standard guard — call it in every state-mutating function.
pub fn require_not_paused(env: &Env) -> Result<(), ContractError> {
    let cfg = config(env);
    if cfg.emergency_pause_enabled {
        return Err(ContractError::ContractPaused);
    }

    match get_pause_mode(env) {
        PauseMode::Paused => Err(ContractError::ContractPaused),
        PauseMode::Thawing => Err(ContractError::ContractThawing),
        PauseMode::None => Ok(()),
    }
}

/// Lenient guard for read-friendly operations (e.g., `withdraw_vouch`).
/// Blocks when `Paused` or emergency-paused, but passes through during `Thawing`.
pub fn require_reads_allowed(env: &Env) -> Result<(), ContractError> {
    let cfg = config(env);
    if cfg.emergency_pause_enabled {
        return Err(ContractError::ContractPaused);
    }

    match get_pause_mode(env) {
        PauseMode::Paused => Err(ContractError::ContractPaused),
        PauseMode::Thawing | PauseMode::None => Ok(()),
    }
}

/// Alias kept for call-sites already updated to use the explicit thaw-blocking name.
/// Identical to `require_not_paused`.
#[inline]
pub fn require_not_thawing(env: &Env) -> Result<(), ContractError> {
    require_not_paused(env)
}

pub fn require_positive_amount(_env: &Env, amount: i128) -> Result<(), ContractError> {
    if amount <= 0 {
        return Err(ContractError::InvalidAmount);
    }
    Ok(())
}

/// Validates that `timestamp` is non-zero and not in the past relative to `now`.
/// Pass `now = env.ledger().timestamp()` for the current ledger time.
/// Returns `Err(InvalidAmount)` if the timestamp is zero or already expired.
pub fn validate_timestamp(_env: &Env, timestamp: u64, now: u64) -> Result<(), ContractError> {
    if timestamp == 0 || timestamp <= now {
        return Err(ContractError::InvalidAmount);
    }
    Ok(())
}

// ── Config & Loan Helpers ─────────────────────────────────────────────────────

pub fn config(env: &Env) -> Config {
    env.storage()
        .instance()
        .get(&DataKey::Config)
        .expect("not initialized")
}

pub fn get_admins(env: &Env) -> Vec<Address> {
    config(env).admins
}

pub fn has_active_loan(env: &Env, borrower: &Address) -> bool {
    matches!(
        get_active_loan_record(env, borrower),
        Ok(loan) if loan.status == LoanStatus::Active
    )
}

pub fn get_active_loan_record(env: &Env, borrower: &Address) -> Result<LoanRecord, ContractError> {
    let loan_id: u64 = env
        .storage()
        .persistent()
        .get(&DataKey::ActiveLoan(borrower.clone()))
        .ok_or(ContractError::NoActiveLoan)?;
    env.storage()
        .persistent()
        .get(&DataKey::Loan(loan_id))
        .ok_or(ContractError::NoActiveLoan)
}

pub fn get_latest_loan_record(env: &Env, borrower: &Address) -> Option<LoanRecord> {
    if let Some(loan_id) = env
        .storage()
        .persistent()
        .get(&DataKey::LatestLoan(borrower.clone()))
    {
        env.storage().persistent().get(&DataKey::Loan(loan_id))
    } else {
        None
    }
}

pub fn next_loan_id(env: &Env) -> u64 {
    let loan_id = env
        .storage()
        .persistent()
        .get(&DataKey::LoanCounter)
        .unwrap_or(0u64)
        .checked_add(1)
        .expect("loan ID overflow");
    env.storage()
        .persistent()
        .set(&DataKey::LoanCounter, &loan_id);
    loan_id
}

pub fn add_slash_balance(env: &Env, amount: i128) {
    let current: i128 = env
        .storage()
        .instance()
        .get(&DataKey::SlashTreasury)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::SlashTreasury, &(current + amount));
}

pub fn is_zero_address(env: &Env, addr: &Address) -> bool {
    let zero_account = Address::from_string(&String::from_str(
        env,
        "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
    ));
    let zero_contract = Address::from_string(&String::from_str(
        env,
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4",
    ));
    addr == &zero_account || addr == &zero_contract
}

pub fn require_valid_address(env: &Env, addr: &Address) -> Result<(), ContractError> {
    if is_zero_address(env, addr) {
        Err(ContractError::ZeroAddress)
    } else {
        Ok(())
    }
}

pub fn require_valid_token(env: &Env, addr: &Address) -> Result<(), ContractError> {
    require_valid_address(env, addr)?;
    let client = token::Client::new(env, addr);
    let probe = env.current_contract_address();
    if client.try_balance(&probe).is_err() {
        return Err(ContractError::InvalidToken);
    }
    Ok(())
}

pub fn validate_admin_config(
    env: &Env,
    admins: &Vec<Address>,
    admin_threshold: u32,
    admin_whitelist: &Vec<Address>,
    admin_blacklist: &Vec<Address>,
) -> Result<(), ContractError> {
    if admins.is_empty() {
        return Err(ContractError::InvalidAdminThreshold);
    }
    if admin_threshold == 0 || admin_threshold > admins.len() {
        return Err(ContractError::InvalidAdminThreshold);
    }
    for i in 0..admin_whitelist.len() {
        let allowed = admin_whitelist.get(i).unwrap();
        require_valid_address(env, &allowed)?;
        for j in 0..i {
            let prior = admin_whitelist.get(j).unwrap();
            if allowed == prior {
                return Err(ContractError::InvalidAdminThreshold);
            }
        }
    }
    for i in 0..admin_blacklist.len() {
        let blocked = admin_blacklist.get(i).unwrap();
        require_valid_address(env, &blocked)?;
        for j in 0..i {
            let prior = admin_blacklist.get(j).unwrap();
            if blocked == prior {
                return Err(ContractError::InvalidAdminThreshold);
            }
        }
        if admin_whitelist.iter().any(|a| a == blocked) {
            return Err(ContractError::InvalidAdminThreshold);
        }
    }
    let admin_count = admins.len();
    for i in 0..admin_count {
        let admin = admins.get(i).unwrap();
        require_valid_address(env, &admin)?;
        for j in 0..i {
            let prior_admin = admins.get(j).unwrap();
            if admin == prior_admin {
                return Err(ContractError::InvalidAdminThreshold);
            }
        }
        if !admin_whitelist.is_empty()
            && !admin_whitelist.iter().any(|allowed| allowed == admin)
        {
            return Err(ContractError::AdminNotWhitelisted);
        }
        if admin_blacklist.iter().any(|blocked| blocked == admin) {
            return Err(ContractError::AdminBlacklisted);
        }
    }
    Ok(())
}

pub fn require_admin_approval(env: &Env, admin_signers: &Vec<Address>) {
    let cfg = config(env);
    assert!(
        admin_signers.len() >= cfg.admin_threshold,
        "insufficient admin approvals"
    );
    for signer in admin_signers.iter() {
        assert!(
            cfg.admins.iter().any(|a| a == signer),
            "signer is not a registered admin"
        );
        signer.require_auth();
    }
}

pub fn is_admin(env: &Env, addr: &Address) -> bool {
    config(env).admins.iter().any(|a| a == *addr)
}

/// Governance participant: registered admin or holder of the protocol token.
pub fn is_governance_participant(env: &Env, addr: &Address) -> bool {
    if is_admin(env, addr) {
        return true;
    }
    let cfg = config(env);
    let token = token::Client::new(env, &cfg.token);
    token.balance(addr) > 0
}

pub fn require_governance_participant(env: &Env, addr: &Address) -> Result<(), ContractError> {
    if is_governance_participant(env, addr) {
        Ok(())
    } else {
        Err(ContractError::NotGovernanceParticipant)
    }
}

pub fn require_allowed_token<'a>(
    env: &'a Env,
    addr: &Address,
) -> Result<token::Client<'a>, ContractError> {
    let cfg = config(env);
    if *addr == cfg.token || cfg.allowed_tokens.iter().any(|t| t == *addr) {
        Ok(token::Client::new(env, addr))
    } else {
        Err(ContractError::InvalidToken)
    }
}

pub fn loan_status(env: &Env, borrower: &Address) -> LoanStatus {
    if let Ok(loan) = get_active_loan_record(env, borrower) {
        return loan.status;
    }
    if let Some(loan) = get_latest_loan_record(env, borrower) {
        return loan.status;
    }
    LoanStatus::None
}

/// Compute the effective slash threshold considering dynamic adjustment.
/// When `Config.dynamic_slash_threshold` is false, returns `Config.slash_bps` unchanged.
/// When true, lowers the threshold proportionally when protocol health ≥ `HEALTH_THRESHOLD_BPS`
/// and raises it when health is poor, clamped to `[MIN_DYNAMIC_SLASH_BPS, MAX_DYNAMIC_SLASH_BPS]`.
pub fn calculate_dynamic_slash_threshold(env: &Env) -> i128 {
    let cfg = config(env);
    if !cfg.dynamic_slash_threshold {
        return cfg.slash_bps;
    }

    let health = calculate_protocol_health_score(env);
    if health >= HEALTH_THRESHOLD_BPS {
        cfg.slash_bps.max(MIN_DYNAMIC_SLASH_BPS)
    } else {
        let adjustment = (HEALTH_THRESHOLD_BPS - health) * (MAX_DYNAMIC_SLASH_BPS - MIN_DYNAMIC_SLASH_BPS) / HEALTH_THRESHOLD_BPS;
        (cfg.slash_bps + adjustment).clamp(MIN_DYNAMIC_SLASH_BPS, MAX_DYNAMIC_SLASH_BPS)
    }
}

/// Protocol health score in basis points (0–10000).
/// Factors: whether config exists (30%), not paused (30%), yield reserve solvency (40%).
pub fn calculate_protocol_health_score(env: &Env) -> i128 {
    let mut score: i128 = 0;

    // 30%: initialized
    if env.storage().instance().has(&DataKey::Config) {
        score += 3_000;
    }

    // 30%: not paused
    let cfg = config(env);
    if get_pause_mode(env) == PauseMode::None && !cfg.emergency_pause_enabled {
        score += 3_000;
    }

    // 40%: yield reserve solvent (non-zero balance)
    let reserve: i128 = env.storage().instance().get(&DataKey::YieldReserve).unwrap_or(0);
    if reserve > 0 {
        score += 4_000;
    }

    score
}

/// Register `borrower` in the global `BorrowerList` if not already present.
pub fn register_borrower_if_needed(env: &Env, borrower: &Address) {
    use soroban_sdk::Vec as SdkVec;
    let mut list: SdkVec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::BorrowerList)
        .unwrap_or(SdkVec::new(env));

    if !list.iter().any(|b| &b == borrower) {
        list.push_back(borrower.clone());
        env.storage().persistent().set(&DataKey::BorrowerList, &list);
    }
}

pub fn primary_token(env: &Env) -> token::Client {
    token::Client::new(env, &config(env).token)
}

// ── Rate Limiting ─────────────────────────────────────────────────────────────

pub fn check_rate_limit(env: &Env, account: &Address) -> Result<(), ContractError> {
    let cfg = config(env);
    if !cfg.rate_limit_config.enabled {
        return Ok(());
    }

    let now = env.ledger().timestamp();
    let (last_window_start, count): (u64, u32) = env
        .storage()
        .persistent()
        .get(&DataKey::RateLimit(account.clone()))
        .unwrap_or((0, 0));

    if now < last_window_start + cfg.rate_limit_config.window_secs {
        if count >= cfg.rate_limit_config.max_calls {
            return Err(ContractError::RateLimitExceeded);
        }
        env.storage().persistent().set(
            &DataKey::RateLimit(account.clone()),
            &(last_window_start, count + 1),
        );
    } else {
        env.storage()
            .persistent()
            .set(&DataKey::RateLimit(account.clone()), &(now, 1));
    }

    Ok(())
}

// ── Access Control ────────────────────────────────────────────────────────────

pub fn check_permission(
    env: &Env,
    account: &Address,
    permission_fn: fn(&crate::types::RolePermissions) -> bool,
) -> Result<(), ContractError> {
    // Admins always have all permissions
    if is_admin(env, account) {
        return Ok(());
    }

    let permissions: crate::types::RolePermissions = env
        .storage()
        .persistent()
        .get(&DataKey::RolePermissions(account.clone()))
        .ok_or(ContractError::PermissionDenied)?;

    if permission_fn(&permissions) {
        Ok(())
    } else {
        Err(ContractError::PermissionDenied)
    }
}
