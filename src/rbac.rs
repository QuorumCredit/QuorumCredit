use crate::errors::ContractError;
use crate::helpers::require_admin_approval;
use crate::types::{AdminPermission, AdminRole, DataKey};
use soroban_sdk::{Address, Env};

/// Assigns an admin role to an address. Requires admin authorization.
pub fn assign_admin_role(
    env: &Env,
    admin_signers: Vec<Address>,
    target_admin: Address,
    role: AdminRole,
) {
    require_admin_approval(env, &admin_signers);
    
    env.storage().persistent().set(&DataKey::AdminRole(target_admin.clone()), &role);
    
    env.events().publish(
        ("admin", "role_assigned"),
        (&admin_signers[0], &target_admin, &role),
    );
}

/// Returns the role of an admin, or error if not set.
pub fn get_admin_role(env: &Env, admin: &Address) -> Result<AdminRole, ContractError> {
    env.storage()
        .persistent()
        .get::<_, AdminRole>(&DataKey::AdminRole(admin.clone()))
        .ok_or(ContractError::PermissionDenied)
}

/// Checks if an admin has a specific permission. Panics if permission denied.
pub fn require_admin_permission(
    env: &Env,
    admin: &Address,
    permission: AdminPermission,
) -> Result<(), ContractError> {
    let role = get_admin_role(env, admin)?;
    
    match role {
        AdminRole::SuperAdmin => Ok(()), // SuperAdmin has all permissions
        AdminRole::Treasurer => {
            match permission {
                AdminPermission::UpdateConfig | AdminPermission::ManageFees => Ok(()),
                _ => Err(ContractError::PermissionDenied),
            }
        }
        AdminRole::Monitor => {
            match permission {
                AdminPermission::ReadAnalytics => Ok(()),
                _ => Err(ContractError::PermissionDenied),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AdminPermission, AdminRole};

    #[test]
    fn test_superadmin_all_permissions() {
        let permissions = vec![
            AdminPermission::Slash,
            AdminPermission::Pause,
            AdminPermission::UpdateConfig,
            AdminPermission::ManageFees,
            AdminPermission::ReadAnalytics,
        ];
        
        for perm in permissions {
            assert_eq!(
                check_role_permission(&AdminRole::SuperAdmin, &perm),
                true,
                "SuperAdmin should have {:?}",
                perm
            );
        }
    }

    #[test]
    fn test_treasurer_config_and_fees() {
        assert!(check_role_permission(&AdminRole::Treasurer, &AdminPermission::UpdateConfig));
        assert!(check_role_permission(&AdminRole::Treasurer, &AdminPermission::ManageFees));
        assert!(!check_role_permission(&AdminRole::Treasurer, &AdminPermission::Slash));
        assert!(!check_role_permission(&AdminRole::Treasurer, &AdminPermission::Pause));
        assert!(!check_role_permission(&AdminRole::Treasurer, &AdminPermission::ReadAnalytics));
    }

    #[test]
    fn test_monitor_read_only() {
        assert!(check_role_permission(&AdminRole::Monitor, &AdminPermission::ReadAnalytics));
        assert!(!check_role_permission(&AdminRole::Monitor, &AdminPermission::Slash));
        assert!(!check_role_permission(&AdminRole::Monitor, &AdminPermission::Pause));
        assert!(!check_role_permission(&AdminRole::Monitor, &AdminPermission::UpdateConfig));
        assert!(!check_role_permission(&AdminRole::Monitor, &AdminPermission::ManageFees));
    }

    fn check_role_permission(role: &AdminRole, perm: &AdminPermission) -> bool {
        match role {
            AdminRole::SuperAdmin => true,
            AdminRole::Treasurer => matches!(perm, AdminPermission::UpdateConfig | AdminPermission::ManageFees),
            AdminRole::Monitor => matches!(perm, AdminPermission::ReadAnalytics),
        }
    }
}
