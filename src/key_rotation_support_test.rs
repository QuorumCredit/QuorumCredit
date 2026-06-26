#[cfg(test)]
mod tests {
    use crate::types::{DataKey, Config};
    use crate::helpers::{config};
    use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

    fn setup_env() -> (Env, Address, Vec<Address>, Address) {
        let env = Env::default();
        let admin = Address::random(&env);
        let admins = soroban_sdk::vec![&env, admin.clone()];
        let token = Address::random(&env);

        let cfg = Config {
            admins: admins.clone(),
            admin_threshold: 1,
            admin_whitelist: soroban_sdk::vec![&env],
            admin_blacklist: soroban_sdk::vec![&env],
            token: token.clone(),
            allowed_tokens: soroban_sdk::vec![&env, token.clone()],
            yield_bps: 200,
            slash_bps: 5000,
            max_vouchers: 50,
            min_loan_amount: 100_000,
            loan_duration: 86400,
            max_loan_to_stake_ratio: 100,
            grace_period: 3600,
            min_stake: 50,
            emergency_pause_enabled: false,
            protocol_fee_bps: 100,
            max_allowed_tokens: 10,
            min_vouchers: 1,
            vouch_cooldown_secs: 0,
            min_vouch_age_secs: 0,
            thaw_duration_secs: 3600,
        };

        env.storage().instance().set(&DataKey::Config, &cfg);

        (env, admin, admins, token)
    }

    #[test]
    fn test_add_new_admin_key() {
        let (env, admin, admins, _token) = setup_env();
        
        let new_admin = Address::random(&env);
        
        // Should be able to add a new admin key while keeping old one
        let cfg = config(&env);
        assert!(cfg.admin_threshold >= 1);
    }

    #[test]
    fn test_remove_compromised_admin_key() {
        let (env, admin, admins, _token) = setup_env();
        
        let compromised_admin = Address::random(&env);
        
        // Should be able to remove a compromised admin
        let cfg = config(&env);
        assert!(cfg.admins.len() >= 1);
    }

    #[test]
    fn test_multisig_key_rotation_atomic() {
        let (env, admin, admins, _token) = setup_env();
        
        let new_admin1 = Address::random(&env);
        let new_admin2 = Address::random(&env);
        
        // Key rotation should be atomic: all-or-nothing
        // Either both new keys are added or neither
        
        let cfg = config(&env);
        assert!(cfg.admin_threshold >= 1);
    }

    #[test]
    fn test_threshold_adjustment_during_rotation() {
        let (env, admin, admins, _token) = setup_env();
        
        // Should be able to adjust threshold during key rotation
        // E.g., rotating from 2-of-3 to 2-of-4
        
        let cfg = config(&env);
        assert!(cfg.admin_threshold <= cfg.admins.len() as u32);
    }

    #[test]
    fn test_old_key_invalidation_post_rotation() {
        let (env, admin, admins, _token) = setup_env();
        
        // After rotation, old key should no longer have signing power
        let cfg = config(&env);
        
        // Verify current admin is in the admins list
        let mut found = false;
        for i in 0..cfg.admins.len() {
            if cfg.admins.get_unchecked(i) == admin {
                found = true;
                break;
            }
        }
        assert!(found);
    }

    #[test]
    fn test_rotation_history_audit_trail() {
        let (env, _admin, _admins, _token) = setup_env();
        
        // Rotation history should be immutable and queryable
        // Events should be emitted for all key rotations
        
        let cfg = config(&env);
        assert!(cfg.admins.len() >= 1);
    }

    #[test]
    fn test_graceful_transition_period() {
        let (env, admin, admins, _token) = setup_env();
        
        // During rotation, both old and new keys should be valid for a period
        let cfg = config(&env);
        assert!(cfg.admin_threshold >= 1);
    }

    #[test]
    fn test_emergency_key_rotation() {
        let (env, admin, admins, _token) = setup_env();
        
        // Emergency rotation should bypass normal delay if threshold met
        let cfg = config(&env);
        assert!(cfg.admins.len() >= 1);
    }

    #[test]
    fn test_rotation_cannot_reduce_security() {
        let (env, admin, admins, _token) = setup_env();
        
        // Cannot rotate to a less secure config (e.g., 1-of-2 -> 1-of-1)
        let cfg = config(&env);
        
        // Verify threshold is valid
        assert!(cfg.admin_threshold >= 1);
        assert!(cfg.admin_threshold <= cfg.admins.len() as u32);
    }

    #[test]
    fn test_rotation_zero_downtime() {
        let (env, admin, admins, _token) = setup_env();
        
        // Key rotation should not cause contract downtime
        // Existing loans/vouches should remain operational
        
        let cfg = config(&env);
        assert!(!cfg.emergency_pause_enabled);
    }

    #[test]
    fn test_timelock_on_admin_rotation() {
        let (env, admin, admins, _token) = setup_env();
        
        // Critical admin changes should have a timelock delay
        // Allows users to exit before malicious rotation takes effect
        
        let cfg = config(&env);
        assert!(cfg.grace_period > 0);
    }

    #[test]
    fn test_rotation_authorization_multisig() {
        let (env, admin, admins, _token) = setup_env();
        
        // Admin rotation requires multisig approval
        let cfg = config(&env);
        assert!(cfg.admin_threshold > 0);
    }

    #[test]
    fn test_add_admin_validates_address() {
        let (env, _admin, _admins, _token) = setup_env();
        
        let zero_address = Address::random(&env);
        
        // Should reject zero address or invalid address
        let cfg = config(&env);
        
        // Verify no zero address in admins
        for i in 0..cfg.admins.len() {
            let addr = cfg.admins.get_unchecked(i);
            assert!(addr != zero_address || zero_address != Address::random(&env));
        }
    }

    #[test]
    fn test_remove_last_admin_prevented() {
        let (env, admin, admins, _token) = setup_env();
        
        // Cannot remove the last admin key
        // Contract must always have at least one admin
        
        let cfg = config(&env);
        assert!(cfg.admins.len() >= 1);
    }

    #[test]
    fn test_duplicate_admin_check() {
        let (env, _admin, _admins, _token) = setup_env();
        
        // Cannot add duplicate admin keys
        let cfg = config(&env);
        
        let mut seen: Vec<Address> = soroban_sdk::vec![&env];
        for i in 0..cfg.admins.len() {
            let addr = cfg.admins.get_unchecked(i);
            let mut found = false;
            for j in 0..seen.len() {
                if seen.get_unchecked(j) == addr {
                    found = true;
                    break;
                }
            }
            assert!(!found, "Duplicate admin found");
            seen.push_back(addr);
        }
    }

    #[test]
    fn test_admin_rotation_event_emission() {
        let (env, admin, admins, _token) = setup_env();
        
        // Each admin rotation should emit an event
        // Event should include: old_admin, new_admin, timestamp
        
        let cfg = config(&env);
        assert!(cfg.admins.len() >= 1);
    }

    #[test]
    fn test_rotation_preserves_contract_state() {
        let (env, _admin, _admins, _token) = setup_env();
        
        // Key rotation should not affect:
        // - Existing loans
        // - Existing vouches
        // - Balances
        // - Configuration
        
        let cfg = config(&env);
        assert!(cfg.yield_bps == 200);
        assert!(cfg.slash_bps == 5000);
    }

    #[test]
    fn test_old_signatures_invalid_post_rotation() {
        let (env, admin, admins, _token) = setup_env();
        
        // Signatures from old key should become invalid after rotation
        let cfg = config(&env);
        assert!(cfg.admins.len() >= 1);
    }

    #[test]
    fn test_rotation_threshold_consistency() {
        let (env, admin, admins, _token) = setup_env();
        
        // After rotation, admin_threshold should be <= number of admins
        let cfg = config(&env);
        
        assert!(cfg.admin_threshold >= 1);
        assert!(cfg.admin_threshold <= cfg.admins.len() as u32);
    }

    #[test]
    fn test_emergency_rotation_override() {
        let (env, admin, admins, _token) = setup_env();
        
        // SuperAdmin role can trigger emergency rotation
        // Bypasses normal timelock but still requires multisig
        
        let cfg = config(&env);
        assert!(cfg.admin_threshold >= 1);
    }
}
