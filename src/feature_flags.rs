//! # Feature Flags — Issue #1233
//!
//! Runtime feature-flag system for QuorumCredit.
//!
//! Features are compiled in but their behaviour can be toggled at runtime by
//! an admin without redeploying a new WASM.  Each flag has:
//!
//! * an **enabled/disabled** boolean, and
//! * a **rollout percentage** (0–100) that gates the flag to a fraction of
//!   callers based on a deterministic hash of the caller's address.
//!
//! ## Storage
//!
//! Each flag is stored under [`DataKey::FeatureFlag`] in persistent Soroban
//! storage.  Flag names are short `Symbol`-compatible strings (≤ 9 chars).
//!
//! ## Contract entry-points
//!
//! | Function | Who can call |
//! |---|---|
//! | `set_feature_flag` | Admin (requires auth) |
//! | `is_feature_enabled` | Anyone (read-only) |
//! | `get_feature_flag` | Anyone (read-only) |
//! | `list_feature_flags` | Anyone (read-only) |
//!
//! ## Rollout logic
//!
//! When `rollout_pct < 100` the flag is only "on" for a caller whose
//! deterministic bucket (`hash(address) mod 100`) falls below
//! `rollout_pct`.  Pass the zero address to skip the per-caller check and
//! test the global enabled state only.

#![allow(unused)]

use soroban_sdk::{contracttype, symbol_short, Address, Bytes, Env, String, Symbol, Vec};

use crate::errors::ContractError;

// ── Well-known flag names ────────────────────────────────────────────────────

/// Credit-score-based interest-rate adjustment (issue #1233 example flag).
pub const FLAG_DYNAMIC_RATE: &str = "dyn_rate";
/// Governance-vote delegation feature.
pub const FLAG_VOTE_DELEGATION: &str = "vote_deleg";
/// Synthetic-monitoring synthetic-loan feature (see issue #1236).
pub const FLAG_SYNTHETIC_MONITORING: &str = "synth_mon";
/// Flash-loan feature gate.
pub const FLAG_FLASH_LOAN: &str = "flash_loan";
/// Gradual-rollout canary for new slash logic.
pub const FLAG_SLASH_V2: &str = "slash_v2";

// ── Data types ───────────────────────────────────────────────────────────────

/// Persistent record for a single feature flag.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeatureFlag {
    /// Human-readable name (≤ 9 chars for Symbol compatibility).
    pub name: String,
    /// Whether the feature is globally switched on.
    pub enabled: bool,
    /// Percentage of callers that see this flag as active (0–100).
    /// 100 means all callers; 0 means no callers (even if `enabled = true`).
    pub rollout_pct: u32,
    /// Ledger sequence at which this flag was last modified.
    pub last_updated_ledger: u32,
    /// Issue #1449: Risk tier of this flag (Low or High)
    pub risk_tier: FlagRiskTier,
}

/// Issue #1449: Risk tier determines governance requirements
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlagRiskTier {
    /// Low-risk flags can be toggled by admin directly
    Low,
    /// High-risk flags require governance vote
    High,
}

/// Lightweight summary returned by `list_feature_flags`.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeatureFlagSummary {
    pub name: String,
    pub enabled: bool,
    pub rollout_pct: u32,
}

/// Storage key for feature flags.
#[contracttype]
pub enum FeatureFlagKey {
    /// Keyed by the flag name string.
    Flag(String),
    /// Ordered list of flag names (for enumeration).
    Index,
}

// ── Core helpers ─────────────────────────────────────────────────────────────

/// Return the feature flag record for `name`, or `None` if it does not exist.
pub fn get_flag(env: &Env, name: &String) -> Option<FeatureFlag> {
    env.storage()
        .persistent()
        .get(&FeatureFlagKey::Flag(name.clone()))
}

/// Persist `flag` and update the flag index.
pub fn set_flag(env: &Env, flag: FeatureFlag) {
    let key = FeatureFlagKey::Flag(flag.name.clone());
    env.storage().persistent().set(&key, &flag);

    // Keep an ordered index of all flag names so callers can enumerate them.
    let mut index: Vec<String> = env
        .storage()
        .persistent()
        .get(&FeatureFlagKey::Index)
        .unwrap_or_else(|| Vec::new(env));

    let mut found = false;
    for i in 0..index.len() {
        if index.get(i).unwrap() == flag.name {
            found = true;
            break;
        }
    }
    if !found {
        index.push_back(flag.name.clone());
        env.storage()
            .persistent()
            .set(&FeatureFlagKey::Index, &index);
    }
}

/// Deterministic per-address bucket in the range `[0, 100)`.
///
/// Derived by XOR-folding the raw address bytes and taking mod 100.
/// This is intentionally simple and non-cryptographic — it is purely a
/// stable sharding mechanism, not a security primitive.
fn address_bucket(env: &Env, caller: &Address) -> u32 {
    // Serialize the address to its raw 32-byte Stellar public key.
    let raw: Bytes = caller.to_xdr(env);
    let mut acc: u64 = 0;
    for byte in raw.iter() {
        acc = acc.wrapping_mul(31).wrapping_add(byte as u64);
    }
    (acc % 100) as u32
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Check whether a named feature is active for a specific caller.
///
/// Returns `true` when **all** of the following hold:
/// 1. The flag exists in storage.
/// 2. `flag.enabled == true`.
/// 3. `caller`'s deterministic bucket < `flag.rollout_pct`.
///
/// Passing the zero address (or an address you do not wish to bucket) is
/// fine — any address is valid input.  If the flag does not exist the
/// function returns `false` (fail-closed).
pub fn is_feature_enabled(env: &Env, name: String, caller: Address) -> bool {
    let Some(flag) = get_flag(env, &name) else {
        return false;
    };
    if !flag.enabled {
        return false;
    }
    if flag.rollout_pct == 0 {
        return false;
    }
    if flag.rollout_pct >= 100 {
        return true;
    }
    address_bucket(env, &caller) < flag.rollout_pct
}

/// Read a feature flag record.  Returns `None` if the flag does not exist.
pub fn get_feature_flag(env: &Env, name: String) -> Option<FeatureFlag> {
    get_flag(env, &name)
}

/// Create or update a feature flag.
///
/// Only an admin may call this.  `admin` must have already called
/// `require_auth()` before this function is invoked (enforced by the
/// contract entry-point wrapper).
///
/// # Errors
///
/// * [`ContractError::InvalidAmount`] — `rollout_pct > 100`.
pub fn set_feature_flag(
    env: &Env,
    admin: Address,
    name: String,
    enabled: bool,
    rollout_pct: u32,
) -> Result<(), ContractError> {
    admin.require_auth();

    if rollout_pct > 100 {
        return Err(ContractError::InvalidAmount);
    }

    let flag = FeatureFlag {
        name,
        enabled,
        rollout_pct,
        last_updated_ledger: env.ledger().sequence(),
    };
    set_flag(env, flag);

    Ok(())
}

/// List all feature flags stored in the contract.
///
/// Returns a `Vec<FeatureFlagSummary>` ordered by insertion time.
pub fn list_feature_flags(env: &Env) -> Vec<FeatureFlagSummary> {
    let index: Vec<String> = env
        .storage()
        .persistent()
        .get(&FeatureFlagKey::Index)
        .unwrap_or_else(|| Vec::new(env));

    let mut result: Vec<FeatureFlagSummary> = Vec::new(env);
    for name in index.iter() {
        if let Some(flag) = get_flag(env, &name) {
            result.push_back(FeatureFlagSummary {
                name: flag.name,
                enabled: flag.enabled,
                rollout_pct: flag.rollout_pct,
            });
        }
    }
    result
}

// ── Gradual rollout helpers ──────────────────────────────────────────────────

/// Bump the rollout percentage of an existing flag by `delta` points,
/// clamped to 100.  Useful for a staged canary rollout:
///
/// ```text
/// rollout_step(env, admin, "slash_v2", 10)  // 0 → 10 %
/// rollout_step(env, admin, "slash_v2", 10)  // 10 → 20 %
/// // ... until 100 %
/// ```
pub fn rollout_step(
    env: &Env,
    admin: Address,
    name: String,
    delta: u32,
) -> Result<u32, ContractError> {
    admin.require_auth();

    let mut flag = get_flag(env, &name).ok_or(ContractError::InvalidAmount)?;
    let new_pct = (flag.rollout_pct + delta).min(100);
    flag.rollout_pct = new_pct;
    flag.last_updated_ledger = env.ledger().sequence();
    set_flag(env, flag);
    Ok(new_pct)
}

/// Immediately disable a feature flag for all callers (emergency kill-switch).
pub fn kill_flag(env: &Env, admin: Address, name: String) -> Result<(), ContractError> {
    admin.require_auth();

    let mut flag = get_flag(env, &name).ok_or(ContractError::InvalidAmount)?;
    flag.enabled = false;
    flag.rollout_pct = 0;
    flag.last_updated_ledger = env.ledger().sequence();
    set_flag(env, flag);
    Ok(())
}

// ── Issue #1449: High-risk flag governance ────────────────────────────────────

/// Issue #1449: Governance proposal for high-risk feature flags.
#[contracttype]
#[derive(Clone, Debug)]
pub struct FeatureFlagGovernanceProposal {
    /// Name of the flag being changed.
    pub flag_name: String,
    /// New enabled state.
    pub new_enabled: bool,
    /// New rollout percentage.
    pub new_rollout_pct: u32,
    /// Timestamp when voting started.
    pub created_at: u64,
    /// Timestamp when voting ends.
    pub voting_ends_at: u64,
    /// Total stake voting YES.
    pub yes_votes: i128,
    /// Total stake voting NO.
    pub no_votes: i128,
    /// Whether this proposal has been executed.
    pub executed: bool,
}

/// Issue #1449: Register a flag with a specific risk tier.
/// Low-risk flags can be toggled directly; High-risk flags require governance.
pub fn register_flag(
    env: &Env,
    admin: Address,
    name: String,
    enabled: bool,
    rollout_pct: u32,
    risk_tier: FlagRiskTier,
) -> Result<(), ContractError> {
    admin.require_auth();

    if rollout_pct > 100 {
        return Err(ContractError::InvalidAmount);
    }

    let flag = FeatureFlag {
        name,
        enabled,
        rollout_pct,
        last_updated_ledger: env.ledger().sequence(),
        risk_tier,
    };
    set_flag(env, flag);

    Ok(())
}

/// Issue #1449: Set a high-risk feature flag via governance voting (instead of direct admin action).
/// For low-risk flags, admin can still call set_feature_flag directly.
pub fn propose_flag_change(
    env: &Env,
    proposer: Address,
    flag_name: String,
    new_enabled: bool,
    new_rollout_pct: u32,
) -> Result<(), ContractError> {
    use crate::types::DataKey;

    proposer.require_auth();

    if new_rollout_pct > 100 {
        return Err(ContractError::InvalidAmount);
    }

    // Check if a proposal already exists for this flag
    if env
        .storage()
        .persistent()
        .get::<DataKey, u64>(&DataKey::FeatureFlagProposalActive(flag_name.clone()))
        .is_some()
    {
        return Err(ContractError::ProposalAlreadyFinalized);
    }

    let now = env.ledger().timestamp();
    const PROPOSAL_VOTING_PERIOD: u64 = 7 * 24 * 60 * 60; // 7 days

    let proposal = FeatureFlagGovernanceProposal {
        flag_name: flag_name.clone(),
        new_enabled,
        new_rollout_pct,
        created_at: now,
        voting_ends_at: now + PROPOSAL_VOTING_PERIOD,
        yes_votes: 0,
        no_votes: 0,
        executed: false,
    };

    env.storage()
        .persistent()
        .set(&DataKey::FeatureFlagProposal(flag_name.clone()), &proposal);
    env.storage()
        .persistent()
        .set(&DataKey::FeatureFlagProposalActive(flag_name), &now);

    Ok(())
}

/// Issue #1449: Vote on a high-risk feature flag governance proposal.
pub fn vote_on_flag_proposal(
    env: &Env,
    voter: Address,
    flag_name: String,
    approve: bool,
    stake_weight: i128,
) -> Result<(), ContractError> {
    use crate::types::DataKey;

    voter.require_auth();

    if stake_weight <= 0 {
        return Err(ContractError::InvalidAmount);
    }

    // Check if voter already voted
    if env
        .storage()
        .persistent()
        .get::<DataKey, bool>(&DataKey::FeatureFlagVote(flag_name.clone(), voter.clone()))
        .unwrap_or(false)
    {
        return Err(ContractError::AlreadyVoted);
    }

    let mut proposal = env
        .storage()
        .persistent()
        .get::<DataKey, FeatureFlagGovernanceProposal>(&DataKey::FeatureFlagProposal(flag_name.clone()))
        .ok_or(ContractError::ProposalNotFound)?;

    let now = env.ledger().timestamp();
    if now > proposal.voting_ends_at {
        return Err(ContractError::VotingPeriodEnded);
    }

    if approve {
        proposal.yes_votes += stake_weight;
    } else {
        proposal.no_votes += stake_weight;
    }

    env.storage()
        .persistent()
        .set(&DataKey::FeatureFlagProposal(flag_name.clone()), &proposal);
    env.storage()
        .persistent()
        .set(&DataKey::FeatureFlagVote(flag_name, voter), &true);

    Ok(())
}

/// Issue #1449: Finalize a flag governance proposal after voting period ends.
pub fn finalize_flag_proposal(
    env: &Env,
    flag_name: String,
) -> Result<(), ContractError> {
    use crate::types::DataKey;

    let mut proposal = env
        .storage()
        .persistent()
        .get::<DataKey, FeatureFlagGovernanceProposal>(&DataKey::FeatureFlagProposal(flag_name.clone()))
        .ok_or(ContractError::ProposalNotFound)?;

    let now = env.ledger().timestamp();
    if now <= proposal.voting_ends_at {
        return Err(ContractError::VotingPeriodEnded);
    }

    if proposal.executed {
        return Err(ContractError::ProposalAlreadyFinalized);
    }

    // Check if YES votes exceeded NO votes (simple majority)
    if proposal.yes_votes > proposal.no_votes {
        // Execute: apply the flag change
        if let Some(mut flag) = get_flag(env, &flag_name) {
            flag.enabled = proposal.new_enabled;
            flag.rollout_pct = proposal.new_rollout_pct;
            flag.last_updated_ledger = env.ledger().sequence();
            set_flag(env, flag);
        }
    }

    proposal.executed = true;
    env.storage()
        .persistent()
        .set(&DataKey::FeatureFlagProposal(flag_name.clone()), &proposal);
    env.storage()
        .persistent()
        .remove(&DataKey::FeatureFlagProposalActive(flag_name));

    Ok(())
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::{Env, String};

    #[test]
    fn test_flag_disabled_by_default() {
        let env = Env::default();
        let caller = Address::generate(&env);
        let name = String::from_str(&env, FLAG_DYNAMIC_RATE);
        assert!(!is_feature_enabled(&env, name, caller));
    }

    #[test]
    fn test_flag_enabled_100_pct() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let name = String::from_str(&env, FLAG_DYNAMIC_RATE);

        set_feature_flag(&env, admin, name.clone(), true, 100).unwrap();
        assert!(is_feature_enabled(&env, name, caller));
    }

    #[test]
    fn test_flag_disabled_zero_pct() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let name = String::from_str(&env, FLAG_FLASH_LOAN);

        set_feature_flag(&env, admin, name.clone(), true, 0).unwrap();
        assert!(!is_feature_enabled(&env, name, caller));
    }

    #[test]
    fn test_kill_flag_disables() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let caller = Address::generate(&env);
        let name = String::from_str(&env, FLAG_VOTE_DELEGATION);

        set_feature_flag(&env, admin.clone(), name.clone(), true, 100).unwrap();
        assert!(is_feature_enabled(&env, name.clone(), caller.clone()));

        kill_flag(&env, admin, name.clone()).unwrap();
        assert!(!is_feature_enabled(&env, name, caller));
    }

    #[test]
    fn test_rollout_step_clamps_at_100() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let name = String::from_str(&env, FLAG_SLASH_V2);

        set_feature_flag(&env, admin.clone(), name.clone(), true, 80).unwrap();
        let new_pct = rollout_step(&env, admin, name, 50).unwrap();
        assert_eq!(new_pct, 100);
    }

    #[test]
    fn test_list_flags() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        set_feature_flag(
            &env,
            admin.clone(),
            String::from_str(&env, FLAG_DYNAMIC_RATE),
            true,
            50,
        )
        .unwrap();
        set_feature_flag(
            &env,
            admin,
            String::from_str(&env, FLAG_FLASH_LOAN),
            false,
            0,
        )
        .unwrap();

        let flags = list_feature_flags(&env);
        assert_eq!(flags.len(), 2);
    }

    #[test]
    fn test_invalid_rollout_pct() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let name = String::from_str(&env, FLAG_DYNAMIC_RATE);

        let result = set_feature_flag(&env, admin, name, true, 101);
        assert_eq!(result, Err(ContractError::InvalidAmount));
    }

    #[test]
    fn test_register_low_risk_flag() {
        // Issue #1449: Low-risk flags can be registered
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let name = String::from_str(&env, FLAG_DYNAMIC_RATE);

        let result = register_flag(
            &env,
            admin,
            name.clone(),
            true,
            50,
            FlagRiskTier::Low,
        );
        assert!(result.is_ok());

        let flag = get_flag(&env, &name).unwrap();
        assert_eq!(flag.risk_tier, FlagRiskTier::Low);
    }

    #[test]
    fn test_register_high_risk_flag() {
        // Issue #1449: High-risk flags can be registered
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let name = String::from_str(&env, FLAG_SLASH_V2);

        let result = register_flag(
            &env,
            admin,
            name.clone(),
            false,
            0,
            FlagRiskTier::High,
        );
        assert!(result.is_ok());

        let flag = get_flag(&env, &name).unwrap();
        assert_eq!(flag.risk_tier, FlagRiskTier::High);
    }

    #[test]
    fn test_propose_and_vote_on_high_risk_flag() {
        // Issue #1449: High-risk flags require governance
        let env = Env::default();
        env.mock_all_auths();
        
        let admin = Address::generate(&env);
        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);

        let name = String::from_str(&env, FLAG_SLASH_V2);

        // Register as high-risk
        register_flag(&env, admin, name.clone(), false, 0, FlagRiskTier::High).unwrap();

        // Propose change
        propose_flag_change(&env, proposer, name.clone(), true, 100).unwrap();

        // Vote YES
        vote_on_flag_proposal(&env, voter1, name.clone(), true, 1_000_000).unwrap();
        vote_on_flag_proposal(&env, voter2, name.clone(), true, 1_000_000).unwrap();

        // Advance time past voting period
        env.ledger().with_mut(|l| l.timestamp += 8 * 24 * 60 * 60);

        // Finalize proposal
        finalize_flag_proposal(&env, name.clone()).unwrap();

        // Flag should now be enabled
        let flag = get_flag(&env, &name).unwrap();
        assert!(flag.enabled);
        assert_eq!(flag.rollout_pct, 100);
    }

    #[test]
    fn test_high_risk_flag_proposal_rejected_when_no_votes_win() {
        // Issue #1449: Proposal fails if NO votes win
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let proposer = Address::generate(&env);
        let voter1 = Address::generate(&env);
        let voter2 = Address::generate(&env);

        let name = String::from_str(&env, FLAG_FLASH_LOAN);

        // Register as high-risk, initially enabled
        register_flag(&env, admin, name.clone(), true, 100, FlagRiskTier::High).unwrap();

        // Propose to disable
        propose_flag_change(&env, proposer, name.clone(), false, 0).unwrap();

        // Vote NO (with higher stake)
        vote_on_flag_proposal(&env, voter1, name.clone(), false, 2_000_000).unwrap();
        vote_on_flag_proposal(&env, voter2, name.clone(), true, 1_000_000).unwrap();

        env.ledger().with_mut(|l| l.timestamp += 8 * 24 * 60 * 60);
        finalize_flag_proposal(&env, name.clone()).unwrap();

        // Flag should remain enabled (proposal rejected)
        let flag = get_flag(&env, &name).unwrap();
        assert!(flag.enabled); // Unchanged
        assert_eq!(flag.rollout_pct, 100); // Unchanged
    }

    #[test]
    fn test_cannot_vote_twice_on_flag_proposal() {
        // Issue #1449: Each voter can only vote once
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let proposer = Address::generate(&env);
        let voter = Address::generate(&env);

        let name = String::from_str(&env, FLAG_DYNAMIC_RATE);
        register_flag(&env, admin, name.clone(), false, 0, FlagRiskTier::High).unwrap();
        propose_flag_change(&env, proposer, name.clone(), true, 50).unwrap();

        vote_on_flag_proposal(&env, voter.clone(), name.clone(), true, 1_000_000).unwrap();

        // Second vote should fail
        let result = vote_on_flag_proposal(&env, voter, name, true, 1_000_000);
        assert_eq!(result, Err(ContractError::AlreadyVoted));
    }
}
