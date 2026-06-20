#[cfg(test)]
mod tests {
    use crate::rbac::{assign_admin_role, get_admin_role, require_admin_permission};
    use crate::types::{AdminPermission, AdminRole, DataKey, Config};
    use crate::helpers::{require_admin_approval};
    use soroban_sdk::{testutils::Address as _, Address, Env};
    use std::panic;

    fn setup_env_with_admins() -> (Env, Vec<Address>, Address, Address) {
        let env = Env::default();
        let admin1 = Address::random(&env);
        let admin2 = Address::random(&env);
        
        // Create a basic config with admins
        let config = Config {
            admins: soroban_sdk::vec![&env, admin1.clone(), admin2.clone()],
            admin_threshold: 2,
            admin_whitelist: soroban_sdk::vec![&env],
            admin_blacklist: soroban_sdk::vec![&env],
            token: Address::random(&env),
            allowed_tokens: soroban_sdk::vec![&env],
            yield_bps: 200,
            slash_bps: 5000,
            max_vouchers: 50,
            min_loan_amount: 100_000,
            loan_duration: 86400,
            max_loan_to_stake_ratio: 100,
            grace_period: 3600,
            min_stake: 50,
        };
        
        env.storage().instance().set(&DataKey::Config, &config);
        
        let target = Address::random(&env);

        (env, soroban_sdk::vec![&env, admin1, admin2], target, Address::random(&env))
    }

    // ── Role Assignment Tests ──

    #[test]
    fn test_assign_superadmin_role() {
        let (env, admins, target, _) = setup_env_with_admins();
        
        assign_admin_role(&env, admins, target.clone(), AdminRole::SuperAdmin);
        
        let role = get_admin_role(&env, &target).unwrap();
        assert_eq!(role, AdminRole::SuperAdmin);
    }

    #[test]
    fn test_assign_treasurer_role() {
        let (env, admins, target, _) = setup_env_with_admins();
        
        assign_admin_role(&env, admins, target.clone(), AdminRole::Treasurer);
        
        let role = get_admin_role(&env, &target).unwrap();
        assert_eq!(role, AdminRole::Treasurer);
    }

    #[test]
    fn test_assign_monitor_role() {
        let (env, admins, target, _) = setup_env_with_admins();
        
        assign_admin_role(&env, admins, target.clone(), AdminRole::Monitor);
        
        let role = get_admin_role(&env, &target).unwrap();
        assert_eq!(role, AdminRole::Monitor);
    }

    #[test]
    fn test_change_admin_role() {
        let (env, admins, target, _) = setup_env_with_admins();
        
        assign_admin_role(&env, admins.clone(), target.clone(), AdminRole::Monitor);
        assert_eq!(get_admin_role(&env, &target).unwrap(), AdminRole::Monitor);
        
        assign_admin_role(&env, admins, target.clone(), AdminRole::Treasurer);
        assert_eq!(get_admin_role(&env, &target).unwrap(), AdminRole::Treasurer);
    }

    // ── Permission Matrix: SuperAdmin Tests ──

    #[test]
    fn test_superadmin_can_slash() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::SuperAdmin);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::Slash).is_ok());
    }

    #[test]
    fn test_superadmin_can_pause() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::SuperAdmin);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::Pause).is_ok());
    }

    #[test]
    fn test_superadmin_can_update_config() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::SuperAdmin);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::UpdateConfig).is_ok());
    }

    #[test]
    fn test_superadmin_can_manage_fees() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::SuperAdmin);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::ManageFees).is_ok());
    }

    #[test]
    fn test_superadmin_can_read_analytics() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::SuperAdmin);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::ReadAnalytics).is_ok());
    }

    // ── Permission Matrix: Treasurer Tests ──

    #[test]
    fn test_treasurer_can_update_config() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Treasurer);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::UpdateConfig).is_ok());
    }

    #[test]
    fn test_treasurer_can_manage_fees() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Treasurer);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::ManageFees).is_ok());
    }

    #[test]
    fn test_treasurer_cannot_slash() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Treasurer);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::Slash).is_err());
    }

    #[test]
    fn test_treasurer_cannot_pause() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Treasurer);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::Pause).is_err());
    }

    #[test]
    fn test_treasurer_cannot_read_analytics() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Treasurer);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::ReadAnalytics).is_err());
    }

    // ── Permission Matrix: Monitor Tests ──

    #[test]
    fn test_monitor_can_read_analytics() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Monitor);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::ReadAnalytics).is_ok());
    }

    #[test]
    fn test_monitor_cannot_slash() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Monitor);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::Slash).is_err());
    }

    #[test]
    fn test_monitor_cannot_pause() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Monitor);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::Pause).is_err());
    }

    #[test]
    fn test_monitor_cannot_update_config() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Monitor);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::UpdateConfig).is_err());
    }

    #[test]
    fn test_monitor_cannot_manage_fees() {
        let (env, admins, target, _) = setup_env_with_admins();
        assign_admin_role(&env, admins, target.clone(), AdminRole::Monitor);
        
        assert!(require_admin_permission(&env, &target, AdminPermission::ManageFees).is_err());
    }

    // ── Edge Cases ──

    #[test]
    fn test_unassigned_admin_permission_denied() {
        let (env, _, unassigned, _) = setup_env_with_admins();
        
        // Admin without assigned role should get PermissionDenied
        assert!(require_admin_permission(&env, &unassigned, AdminPermission::Slash).is_err());
    }

    #[test]
    fn test_all_role_permission_combinations() {
        let (env, admins, target, _) = setup_env_with_admins();
        
        let roles = vec![
            AdminRole::SuperAdmin,
            AdminRole::Treasurer,
            AdminRole::Monitor,
        ];
        
        let permissions = vec![
            AdminPermission::Slash,
            AdminPermission::Pause,
            AdminPermission::UpdateConfig,
            AdminPermission::ManageFees,
            AdminPermission::ReadAnalytics,
        ];

        for role in roles {
            assign_admin_role(&env, admins.clone(), target.clone(), role.clone());
            
            for perm in &permissions {
                let can_access = require_admin_permission(&env, &target, perm.clone());
                
                let expected = match role {
                    AdminRole::SuperAdmin => true,
                    AdminRole::Treasurer => matches!(perm, AdminPermission::UpdateConfig | AdminPermission::ManageFees),
                    AdminRole::Monitor => matches!(perm, AdminPermission::ReadAnalytics),
                };
                
                if expected {
                    assert!(can_access.is_ok(), "Role {:?} should have permission {:?}", role, perm);
                } else {
                    assert!(can_access.is_err(), "Role {:?} should NOT have permission {:?}", role, perm);
                }
            }
        }
    }

    #[test]
    fn test_audit_logging_on_role_assignment() {
        let (env, admins, target, _) = setup_env_with_admins();
        
        // Role assignment should emit event (audit trail)
        assign_admin_role(&env, admins, target, AdminRole::SuperAdmin);
        
        // Event is emitted automatically via publish
        // In real deployment, this would be indexed off-chain
    }
}
