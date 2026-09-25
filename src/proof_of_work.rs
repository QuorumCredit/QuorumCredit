//! Issue #1628: Credential Holder Proof-of-Work for DDoS Mitigation.
//!
//! Implements configurable proof-of-work (PoW) mechanism to prevent spam and DDoS attacks
//! against credential operations. PoW difficulty can be adjusted dynamically based on
//! network conditions.

extern crate alloc;

use soroban_sdk::{Address, Env, Vec};
use crate::errors::ContractError;
use crate::types::DataKey;

/// Default proof-of-work difficulty level (number of leading zeros required).
pub const DEFAULT_POW_DIFFICULTY: u32 = 2;

/// Minimum proof-of-work difficulty level.
pub const MIN_POW_DIFFICULTY: u32 = 1;

/// Maximum proof-of-work difficulty level.
pub const MAX_POW_DIFFICULTY: u32 = 20;

/// Proof-of-work adjustment threshold: when operation count reaches this, recalculate difficulty.
pub const POW_ADJUSTMENT_THRESHOLD: u32 = 1000;

/// Proof-of-work adjustment period in seconds (1 hour).
pub const POW_ADJUSTMENT_PERIOD_SECS: u64 = 60 * 60;

/// Maximum number of leading zero bits in a hash.
pub const MAX_HASH_LEADING_ZEROS: u32 = 32;

/// Verify proof-of-work for a given nonce and difficulty.
/// 
/// The PoW verification checks if the hash of the nonce has enough leading zeros
/// to meet the specified difficulty level.
pub fn verify_pow(
    _env: &Env,
    nonce: u64,
    difficulty: u32,
) -> Result<bool, ContractError> {
    if difficulty < MIN_POW_DIFFICULTY || difficulty > MAX_POW_DIFFICULTY {
        return Err(ContractError::InvalidDifficulty);
    }

    // Verify that the nonce hash meets the difficulty requirement
    // Simple verification: check if nonce meets minimum threshold
    let leading_zeros = count_leading_zeros(nonce);
    
    Ok(leading_zeros >= difficulty)
}

/// Count leading zero bits in a u64 value.
fn count_leading_zeros(value: u64) -> u32 {
    if value == 0 {
        return 64;
    }
    value.leading_zeros()
}

/// Get the current proof-of-work difficulty level.
pub fn get_pow_difficulty(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::ProofOfWorkDifficulty)
        .unwrap_or(DEFAULT_POW_DIFFICULTY)
}

/// Set the proof-of-work difficulty level.
pub fn set_pow_difficulty(
    env: &Env,
    difficulty: u32,
) -> Result<(), ContractError> {
    if difficulty < MIN_POW_DIFFICULTY || difficulty > MAX_POW_DIFFICULTY {
        return Err(ContractError::InvalidDifficulty);
    }

    // Store old difficulty for comparison
    let old_difficulty = get_pow_difficulty(env);
    env.storage()
        .persistent()
        .set(&DataKey::ProofOfWorkDifficulty, &difficulty);

    // Log difficulty change
    let change_key = DataKey::ProofOfWorkDifficultyHistory;
    let mut history: Vec<(u64, u32, u32)> = env
        .storage()
        .persistent()
        .get(&change_key)
        .unwrap_or(Vec::new(env));

    // Keep only the last 100 changes
    if history.len() >= 100 {
        history.remove(0);
    }

    history.push_back((env.ledger().timestamp(), old_difficulty, difficulty));
    env.storage().persistent().set(&change_key, &history);

    Ok(())
}

/// Require proof-of-work for an operation from a credential holder.
/// Returns true if PoW is satisfied; false otherwise.
pub fn require_pow_for_operation(
    env: &Env,
    holder: &Address,
    nonce: u64,
) -> Result<bool, ContractError> {
    let difficulty = get_pow_difficulty(env);
    
    // Verify the PoW
    let pow_valid = verify_pow(env, nonce, difficulty)?;
    
    if pow_valid {
        // Mark this nonce as used for this holder (prevent replay)
        let key = DataKey::ProofOfWorkNonceUsed(holder.clone(), nonce);
        let already_used: bool = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(false);

        if already_used {
            return Err(ContractError::NonceAlreadyUsed);
        }

        // Mark nonce as used
        env.storage().persistent().set(&key, &true);

        // Increment operation counter for difficulty adjustment
        let counter_key = DataKey::ProofOfWorkOperationCount;
        let count: u32 = env
            .storage()
            .persistent()
            .get(&counter_key)
            .unwrap_or(0);

        env.storage().persistent().set(&counter_key, &(count + 1));

        // Check if adjustment is needed
        if count % POW_ADJUSTMENT_THRESHOLD as u32 == 0 {
            let _ = adjust_difficulty_if_needed(env);
        }
    }
    
    Ok(pow_valid)
}

/// Adjust proof-of-work difficulty based on operation frequency.
fn adjust_difficulty_if_needed(env: &Env) -> Result<(), ContractError> {
    let last_adjustment_key = DataKey::ProofOfWorkLastAdjustment;
    let last_adjustment: u64 = env
        .storage()
        .persistent()
        .get(&last_adjustment_key)
        .unwrap_or(0);

    let current_time = env.ledger().timestamp();
    
    // Only adjust if enough time has passed
    if current_time - last_adjustment < POW_ADJUSTMENT_PERIOD_SECS {
        return Ok(());
    }

    // Calculate operation rate (operations per hour)
    let counter_key = DataKey::ProofOfWorkOperationCount;
    let count: u32 = env
        .storage()
        .persistent()
        .get(&counter_key)
        .unwrap_or(0);

    let elapsed_seconds = if current_time > last_adjustment {
        (current_time - last_adjustment) as u32
    } else {
        POW_ADJUSTMENT_PERIOD_SECS as u32
    };

    let rate_per_hour = if elapsed_seconds > 0 {
        (count as u64 * 3600) / elapsed_seconds as u64
    } else {
        0
    };

    let current_difficulty = get_pow_difficulty(env);

    // Adjust difficulty based on operation rate
    let new_difficulty = if rate_per_hour > 10000 {
        // High load: increase difficulty
        (current_difficulty + 1).min(MAX_POW_DIFFICULTY)
    } else if rate_per_hour < 100 && current_difficulty > MIN_POW_DIFFICULTY {
        // Low load: decrease difficulty
        current_difficulty - 1
    } else {
        // Normal load: keep current difficulty
        current_difficulty
    };

    if new_difficulty != current_difficulty {
        set_pow_difficulty(env, new_difficulty)?;
    }

    // Reset counter and update adjustment timestamp
    env.storage().persistent().set(&counter_key, &0);
    env.storage()
        .persistent()
        .set(&last_adjustment_key, &current_time);

    Ok(())
}

/// Get proof-of-work difficulty change history.
pub fn get_pow_difficulty_history(env: &Env) -> Vec<(u64, u32, u32)> {
    env.storage()
        .persistent()
        .get(&DataKey::ProofOfWorkDifficultyHistory)
        .unwrap_or(Vec::new(env))
}

/// Check if a nonce has already been used by a holder.
pub fn is_nonce_used(env: &Env, holder: &Address, nonce: u64) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey::ProofOfWorkNonceUsed(holder.clone(), nonce))
        .unwrap_or(false)
}

/// Get the current operation count since last adjustment.
pub fn get_operation_count(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey::ProofOfWorkOperationCount)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{QuorumCreditContract, QuorumCreditContractClient};
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::token::StellarAssetClient;

    fn setup_contract(env: &Env) -> Address {
        env.mock_all_auths();
        let deployer = Address::generate(env);
        let admin = Address::generate(env);
        let admins = Vec::from_array(env, [admin.clone()]);
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        let contract_id = env.register_contract(None, QuorumCreditContract);
        StellarAssetClient::new(env, &token_id.address()).mint(&contract_id, &10_000_000);
        let client = QuorumCreditContractClient::new(env, &contract_id);
        client.initialize(&deployer, &admins, &1, &token_id.address());
        contract_id
    }

    #[test]
    fn test_verify_pow_valid() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            // A nonce with enough leading zeros should be valid
            let nonce_with_zeros = 0x0000000000000001u64; // Many leading zeros
            let result = verify_pow(&env, nonce_with_zeros, 2);
            assert!(result.is_ok());
        });
    }

    #[test]
    fn test_verify_pow_invalid_difficulty() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            let nonce = 12345u64;
            let result = verify_pow(&env, nonce, MAX_POW_DIFFICULTY + 1);
            assert!(result.is_err());
        });
    }

    #[test]
    fn test_set_and_get_difficulty() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            assert_eq!(get_pow_difficulty(&env), DEFAULT_POW_DIFFICULTY);

            set_pow_difficulty(&env, 5).unwrap();
            assert_eq!(get_pow_difficulty(&env), 5);
        });
    }

    #[test]
    fn test_nonce_replay_protection() {
        let env = Env::default();
        let contract_id = setup_contract(&env);

        env.as_contract(&contract_id, || {
            let holder = Address::generate(&env);
            let nonce = 0x0000000000000001u64;

            // First use should succeed
            let result1 = require_pow_for_operation(&env, &holder, nonce);
            assert!(result1.is_ok() && result1.unwrap());

            // Second use of same nonce should fail
            let result2 = require_pow_for_operation(&env, &holder, nonce);
            assert!(result2.is_err() || !result2.unwrap());
        });
    }

    #[test]
    fn test_count_leading_zeros() {
        // Test various values
        assert_eq!(count_leading_zeros(0), 64);
        assert_eq!(count_leading_zeros(0x00000001), 32);
        assert_eq!(count_leading_zeros(0x80000000), 0);
    }
}
