/// Quorum slice self-healing module
/// Issue #1606: Implement Quorum Slice Self-Healing

use soroban_sdk::{contracttype, Address, Env, Vec};
use crate::errors::ContractError;

/// Represents a quorum slice configuration
#[contracttype]
pub struct QuorumSlice {
    pub id: u64,
    pub validators: Vec<Address>,
    pub threshold: u32,
    pub self_healing_enabled: bool,
    pub created_at: u64,
    pub last_healed_at: u64,
    pub failed_count: u32,
}

/// Represents an attestor in the quorum
#[contracttype]
pub struct Attestor {
    pub address: Address,
    pub is_active: bool,
    pub failure_count: u32,
    pub last_failure_at: u64,
    pub joined_at: u64,
}

/// Represents a self-healing event
#[contracttype]
pub struct HealingEvent {
    pub slice_id: u64,
    pub failed_attestor: Address,
    pub replacement_attestor: Address,
    pub executed_at: u64,
    pub executed_by: Address,
}

/// Represents failure detection state
#[contracttype]
pub struct FailureDetection {
    pub slice_id: u64,
    pub failed_attestors: Vec<Address>,
    pub detection_time: u64,
    pub is_critical: bool,
}

/// Enable self-healing for a slice
pub fn enable_slice_self_healing(
    env: &Env,
    admin: &Address,
    slice: &mut QuorumSlice,
) -> Result<(), ContractError> {
    admin.require_auth();

    slice.self_healing_enabled = true;
    slice.last_healed_at = env.ledger().timestamp();

    Ok(())
}

/// Detect automatic failure in quorum
pub fn detect_automatic_failure(
    env: &Env,
    slice: &QuorumSlice,
    attestors: Vec<Attestor>,
) -> Result<FailureDetection, ContractError> {
    let now = env.ledger().timestamp();
    let mut failed_attestors = Vec::new(env);
    let mut failure_count: u32 = 0;

    for i in 0..attestors.len() {
        let attestor = attestors.get(i).ok_or(ContractError::NotFound)?;

        // Mark as failed if:
        // 1. Inactive or
        // 2. Too many failures (>= 3) or
        // 3. Last failure was too recent (within 1 hour)
        if !attestor.is_active ||
           attestor.failure_count >= 3 ||
           (now.saturating_sub(attestor.last_failure_at) < 3600) {
            failed_attestors.push_back(attestor.address.clone());
            failure_count += 1;
        }
    }

    let is_critical = failure_count as u32 >= slice.threshold;

    Ok(FailureDetection {
        slice_id: slice.id,
        failed_attestors,
        detection_time: now,
        is_critical,
    })
}

/// Replace failed attestor automatically
pub fn replace_failed_attestor(
    env: &Env,
    slice: &mut QuorumSlice,
    failed_attestor: Address,
    replacement_attestor: Address,
) -> Result<HealingEvent, ContractError> {
    if !slice.self_healing_enabled {
        return Err(ContractError::SelfHealingDisabled);
    }

    // Find and replace the failed attestor
    let mut found = false;
    for i in 0..slice.validators.len() {
        let validator = slice.validators.get(i).ok_or(ContractError::NotFound)?;
        if validator == &failed_attestor {
            slice.validators.set(i, replacement_attestor.clone());
            found = true;
            break;
        }
    }

    if !found {
        return Err(ContractError::NotFound);
    }

    let now = env.ledger().timestamp();
    slice.last_healed_at = now;
    slice.failed_count = slice.failed_count.saturating_add(1);

    Ok(HealingEvent {
        slice_id: slice.id,
        failed_attestor,
        replacement_attestor,
        executed_at: now,
        executed_by: Address::from_contract_id(&env.self_address()),
    })
}

/// Track self-healing events
pub fn track_healing_event(
    env: &Env,
    healing_event: &HealingEvent,
) -> Result<(), ContractError> {
    // Verify the healing event is recent (within 1 hour)
    let now = env.ledger().timestamp();
    if now.saturating_sub(healing_event.executed_at) > 3600 {
        return Err(ContractError::NotFound);
    }

    Ok(())
}

/// Initialize attestor for a slice
pub fn initialize_attestor(
    env: &Env,
    address: Address,
) -> Attestor {
    let now = env.ledger().timestamp();

    Attestor {
        address,
        is_active: true,
        failure_count: 0,
        last_failure_at: 0,
        joined_at: now,
    }
}

/// Mark attestor as failed
pub fn mark_attestor_failed(
    env: &Env,
    attestor: &mut Attestor,
) {
    let now = env.ledger().timestamp();
    attestor.failure_count = attestor.failure_count.saturating_add(1);
    attestor.last_failure_at = now;

    if attestor.failure_count >= 3 {
        attestor.is_active = false;
    }
}

/// Reset attestor failure count
pub fn reset_attestor_failures(
    env: &Env,
    attestor: &mut Attestor,
) {
    attestor.failure_count = 0;
    attestor.is_active = true;
    attestor.last_failure_at = 0;
}

/// Get quorum status
pub fn get_quorum_status(
    env: &Env,
    slice: &QuorumSlice,
    attestors: &Vec<Attestor>,
) -> Result<bool, ContractError> {
    let mut active_count: u32 = 0;

    for i in 0..attestors.len() {
        let attestor = attestors.get(i).ok_or(ContractError::NotFound)?;
        if attestor.is_active {
            active_count += 1;
        }
    }

    if active_count >= slice.threshold {
        Ok(true)
    } else {
        Err(ContractError::InsufficientQuorum)
    }
}

/// Create quorum slice
pub fn create_quorum_slice(
    env: &Env,
    slice_id: u64,
    validators: Vec<Address>,
    threshold: u32,
) -> Result<QuorumSlice, ContractError> {
    if threshold == 0 || threshold as usize > validators.len() {
        return Err(ContractError::InvalidThreshold);
    }

    let now = env.ledger().timestamp();

    Ok(QuorumSlice {
        id: slice_id,
        validators,
        threshold,
        self_healing_enabled: false,
        created_at: now,
        last_healed_at: 0,
        failed_count: 0,
    })
}
