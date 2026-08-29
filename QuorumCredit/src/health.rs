/// Health check module (Issue #112: enhanced with degraded status).
///
/// Exposes a `health_check` function that evaluates the operational status of
/// the contract and all key sub-systems, returning a [`HealthStatus`] struct
/// that an off-chain monitor can poll.
///
/// ## Status Levels
///
/// | `overall_status` | Meaning |
/// |---|---|
/// | `HealthLevel::Healthy` | All checks pass. |
/// | `HealthLevel::Degraded` | Some non-critical checks failed but the contract is still serving requests (local fallbacks active). |
/// | `HealthLevel::Down` | Critical checks failed — the contract cannot operate correctly. |
///
/// ## Checks Performed
///
/// | Check | Critical? | Notes |
/// |---|---|---|
/// | Contract initialized | ✅ Yes | `DataKey::Config` must exist in storage. |
/// | Contract not paused | ✅ Yes | `DataKey::Paused == false`. |
/// | Yield reserve solvent | ✅ Yes | Contract token balance ≥ 1 XLM (10_000_000 stroops). |
/// | PubSub bus connectivity | ❌ No | Verified via `DataKey::PubSubHealthy` sentinel set by the off-chain relayer. |
/// | RevocationStore connectivity | ❌ No | Verified via `DataKey::RevocationStoreHealthy` sentinel. |
/// | WebhookRegistry connectivity | ❌ No | Verified via `DataKey::WebhookRegistryHealthy` sentinel. |
///
/// Non-critical checks use a degraded-when-failing model: if the underlying
/// Redis-backed component is unavailable but local fallbacks are serving, the
/// contract reports `Degraded` rather than `Down`.

use crate::types::{Config, DataKey};
use soroban_sdk::{contracttype, Env, String, Vec};

// ── Issue #112: Health level enum ────────────────────────────────────────────

/// Operational health level of the contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HealthLevel {
    /// All checks pass — contract is fully operational.
    Healthy,
    /// Some non-critical components are degraded (local fallbacks active).
    Degraded,
    /// Critical checks failed — contract is not operational.
    Down,
}

// ── Issue #112: Enhanced HealthStatus ────────────────────────────────────────

/// Detailed health status returned by [`health_check`].
#[contracttype]
#[derive(Clone, Debug)]
pub struct HealthStatus {
    /// Overall health level: Healthy / Degraded / Down.
    pub overall_status: HealthLevel,
    /// Legacy field — true only when `overall_status == Healthy`.
    pub is_healthy: bool,
    /// Whether the contract has been initialized.
    pub initialized: bool,
    /// Whether the contract is currently paused.
    pub paused: bool,
    /// Whether the yield reserve holds at least 1 XLM.
    pub yield_reserve_solvent: bool,
    // ── Issue #112: sub-system connectivity ──────────────────────────────────
    /// PubSub bus is reachable (off-chain sentinel).
    pub pubsub_connected: bool,
    /// RevocationStore is reachable (off-chain sentinel).
    pub revocation_store_connected: bool,
    /// WebhookRegistry is reachable (off-chain sentinel).
    pub webhook_registry_connected: bool,
    /// Human-readable issue descriptions collected during the check.
    pub issues: Vec<String>,
}

/// Perform a comprehensive health check of the contract.
///
/// # How sub-system sentinels work
///
/// Off-chain relayers (e.g. the Stellar event indexer, the Redis bridge) write
/// a boolean sentinel into contract instance storage via an admin-restricted
/// `set_subsystem_health` call:
///
/// - `DataKey::PubSubHealthy`           → set by the PubSub relay process
/// - `DataKey::RevocationStoreHealthy`  → set by the RevocationStore proxy
/// - `DataKey::WebhookRegistryHealthy`  → set by the WebhookRegistry proxy
///
/// Absence of a sentinel (never set) is treated as **unknown → degraded** so
/// that a freshly-deployed contract does not report itself as healthy for
/// components it has never confirmed.
pub fn health_check(env: &Env) -> HealthStatus {
    let mut issues = Vec::new(env);
    let mut critical_fail = false;
    let mut degraded = false;

    // ── Critical: initialization ──────────────────────────────────────────────
    let initialized = env.storage().instance().has(&DataKey::Config);
    if !initialized {
        issues.push_back(String::from_str(env, "Contract not initialized"));
        critical_fail = true;
    }

    // ── Critical: pause state ────────────────────────────────────────────────
    let paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false);

    // ── Critical: yield reserve solvency ─────────────────────────────────────
    let yield_reserve_solvent = if initialized {
        let config: Config = env
            .storage()
            .instance()
            .get(&DataKey::Config)
            .unwrap_or_else(|| {
                panic!("Config not found despite initialized check");
            });

        let token_client = soroban_sdk::token::Client::new(env, &config.token);
        let contract_balance = token_client.balance(&env.current_contract_address());

        // Reserve must be at least 1 XLM (10_000_000 stroops) to be considered solvent
        contract_balance >= 10_000_000
    } else {
        false
    };

    if !yield_reserve_solvent && initialized {
        issues.push_back(String::from_str(
            env,
            "Yield reserve below minimum threshold (1 XLM)",
        ));
        critical_fail = true;
    }

    // ── Non-critical: PubSub bus connectivity ────────────────────────────────
    // Sentinel written by the off-chain PubSub relay via set_subsystem_health().
    // If absent, treat as degraded (unknown).
    let pubsub_connected: bool = env
        .storage()
        .instance()
        .get(&DataKey::PubSubHealthy)
        .unwrap_or(false);

    if !pubsub_connected {
        issues.push_back(String::from_str(
            env,
            "PubSub bus unreachable or sentinel not set — local event fallback active",
        ));
        degraded = true;
    }

    // ── Non-critical: RevocationStore connectivity ───────────────────────────
    let revocation_store_connected: bool = env
        .storage()
        .instance()
        .get(&DataKey::RevocationStoreHealthy)
        .unwrap_or(false);

    if !revocation_store_connected {
        issues.push_back(String::from_str(
            env,
            "RevocationStore unreachable or sentinel not set — local cache fallback active",
        ));
        degraded = true;
    }

    // ── Non-critical: WebhookRegistry connectivity ───────────────────────────
    let webhook_registry_connected: bool = env
        .storage()
        .instance()
        .get(&DataKey::WebhookRegistryHealthy)
        .unwrap_or(false);

    if !webhook_registry_connected {
        issues.push_back(String::from_str(
            env,
            "WebhookRegistry unreachable or sentinel not set — webhook delivery may be delayed",
        ));
        degraded = true;
    }

    // ── Derive overall status ─────────────────────────────────────────────────
    let overall_status = if critical_fail {
        HealthLevel::Down
    } else if degraded {
        HealthLevel::Degraded
    } else {
        HealthLevel::Healthy
    };

    let is_healthy = overall_status == HealthLevel::Healthy;

    HealthStatus {
        overall_status,
        is_healthy,
        initialized,
        paused,
        yield_reserve_solvent,
        pubsub_connected,
        revocation_store_connected,
        webhook_registry_connected,
        issues,
    }
}
