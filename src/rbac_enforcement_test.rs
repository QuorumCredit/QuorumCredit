#![cfg(test)]

extern crate std;
use std::vec;

use crate::errors::ContractError;
use crate::helpers::config;
use crate::rbac::{self, AdminAction};
use crate::types::{AdminPermission, AdminRole, Config, DataKey};
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

// ── Test Setup Helpers ────────────────────────────────────────────────────

fn setup_admin_system(env: &Env, superadmin_count: u32, config_fn: impl Fn(&mut Config)) -> (Address, Vec<Address>, Address) {
    env.mock_all_auths();
    let contract_id = env.register(crate::QuorumCreditContract, ());
    let client = crate::QuorumCreditContractClient::new(env, &contract_id);

    let deployer = Address::generate(env);
    let mut admin_addrs = Vec::new(env);
    for _ in 0..superadmin_count {
        let addr = Address::generate(env);
        admin_addrs.push_back(addr.clone());
    }

    let token_id = env.register_stellar_asset_contract_v2(admin_addrs.get(0).unwrap().clone());
    let threshold = superadmin_count / 2 + 1;

    client.initialize(&deployer, &admin_addrs, &threshold, &token_id.address());

    env.as_contract(&contract_id, || {
        for admin in admin_addrs.iter() {
            env.storage().persistent().set(&DataKey::AdminRole(admin.clone()), &AdminRole::SuperAdmin);
        }

        let mut cfg = config(env);
        config_fn(&mut cfg);
        env.storage().instance().set(&DataKey::Config, &cfg);
    });

    (admin_addrs.get(0).unwrap(), admin_addrs, contract_id)
}

fn assign_roles(env: &Env, contract_id: &Address, admins: &Vec<Address>, roles: std::vec::Vec<(u32, AdminRole)>) {
    env.as_contract(contract_id, || {
        for (idx, role) in roles.into_iter() {
            let admin = admins.get(idx).unwrap();
            env.storage().persistent().set(&DataKey::AdminRole(admin), &role);
        }
    });
}

// ── Unit Tests ────────────────────────────────────────────────────────────

#[test]
fn test_permission_matrix_superadmin_all_actions() {
    let test_actions = [
        AdminAction::Pause,
        AdminAction::AddAdmin,
        AdminAction::RemoveAdmin,
        AdminAction::SetAdminThreshold,
        AdminAction::UpdateFees,
        AdminAction::UpdateConfig,
        AdminAction::Slash,
    ];

    for action in test_actions {
        let required_perm = rbac::get_required_permission(action);
        assert!(rbac::check_admin_permission(&AdminRole::SuperAdmin, &required_perm),
            "SuperAdmin should have permission for action: {:?}", action);
    }
}

#[test]
fn test_permission_matrix_treasurer_limited_actions() {
    let treasurer = AdminRole::Treasurer;

    assert!(rbac::check_admin_permission(&treasurer, &AdminPermission::UpdateConfig));
    assert!(rbac::check_admin_permission(&treasurer, &AdminPermission::ManageFees));

    assert!(!rbac::check_admin_permission(&treasurer, &AdminPermission::Slash));
    assert!(!rbac::check_admin_permission(&treasurer, &AdminPermission::Pause));
    assert!(!rbac::check_admin_permission(&treasurer, &AdminPermission::ReadAnalytics));
}

#[test]
fn test_permission_matrix_monitor_read_only() {
    let monitor = AdminRole::Monitor;

    assert!(rbac::check_admin_permission(&monitor, &AdminPermission::ReadAnalytics));

    assert!(!rbac::check_admin_permission(&monitor, &AdminPermission::Slash));
    assert!(!rbac::check_admin_permission(&monitor, &AdminPermission::Pause));
    assert!(!rbac::check_admin_permission(&monitor, &AdminPermission::UpdateConfig));
    assert!(!rbac::check_admin_permission(&monitor, &AdminPermission::ManageFees));
}

#[test]
fn test_action_permission_mapping() {
    assert_eq!(rbac::get_required_permission(AdminAction::Pause), AdminPermission::Pause);
    assert_eq!(rbac::get_required_permission(AdminAction::Slash), AdminPermission::Slash);
    assert_eq!(rbac::get_required_permission(AdminAction::AddAdmin), AdminPermission::UpdateConfig);
    assert_eq!(rbac::get_required_permission(AdminAction::UpdateFees), AdminPermission::ManageFees);
    assert_eq!(rbac::get_required_permission(AdminAction::UpdateConfig), AdminPermission::UpdateConfig);
}

// ── Integration Tests: Role-Based Gating ──────────────────────────────────

#[test]
fn test_both_threshold_and_role_required() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 3, |cfg| {
        cfg.admin_threshold = 2;
    });

    let admin1 = signers.get(0).unwrap();
    let admin2 = signers.get(1).unwrap();

    assign_roles(&env, &contract_id, &signers, vec![(0, AdminRole::SuperAdmin), (1, AdminRole::Monitor)]);

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(admin1);
    test_signers.push_back(admin2);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(
            &env,
            &test_signers,
            AdminAction::Pause,
        )
    });

    assert!(result.is_err(), "Should fail because Monitor lacks Pause permission");
    match result {
        Err(ContractError::PermissionDenied) => {},
        _ => panic!("Expected PermissionDenied error"),
    }
}

#[test]
fn test_threshold_check_before_role_check() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 3, |cfg| {
        cfg.admin_threshold = 3;
    });

    let admin1 = signers.get(0).unwrap();
    let admin2 = signers.get(1).unwrap();

    assign_roles(&env, &contract_id, &signers, vec![
        (0, AdminRole::SuperAdmin),
        (1, AdminRole::SuperAdmin),
    ]);

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(admin1);
    test_signers.push_back(admin2);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(
            &env,
            &test_signers,
            AdminAction::Pause,
        )
    });

    assert!(result.is_err(), "Should fail because threshold not met (2 < 3)");
    match result {
        Err(ContractError::UnauthorizedCaller) => {},
        _ => panic!("Expected UnauthorizedCaller error"),
    }
}

#[test]
fn test_monitor_cannot_pause_even_with_threshold() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 1, |cfg| {
        cfg.admin_threshold = 1;
    });

    let monitor = signers.get(0).unwrap();
    assign_roles(&env, &contract_id, &signers, vec![(0, AdminRole::Monitor)]);

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(monitor);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(
            &env,
            &test_signers,
            AdminAction::Pause,
        )
    });

    assert!(result.is_err(), "Monitor should not be able to pause");
    match result {
        Err(ContractError::PermissionDenied) => {},
        _ => panic!("Expected PermissionDenied error"),
    }
}

#[test]
fn test_treasurer_can_set_protocol_fee() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 2, |cfg| {
        cfg.admin_threshold = 1;
    });

    let treasurer = signers.get(0).unwrap();
    assign_roles(&env, &contract_id, &signers, vec![(0, AdminRole::Treasurer)]);

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(treasurer);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(
            &env,
            &test_signers,
            AdminAction::UpdateFees,
        )
    });

    assert!(result.is_ok(), "Treasurer should be able to update fees");
}

#[test]
fn test_treasurer_cannot_slash() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 2, |cfg| {
        cfg.admin_threshold = 1;
    });

    let treasurer = signers.get(0).unwrap();
    assign_roles(&env, &contract_id, &signers, vec![(0, AdminRole::Treasurer)]);

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(treasurer);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(
            &env,
            &test_signers,
            AdminAction::Slash,
        )
    });

    assert!(result.is_err(), "Treasurer should not be able to slash");
    match result {
        Err(ContractError::PermissionDenied) => {},
        _ => panic!("Expected PermissionDenied error"),
    }
}

#[test]
fn test_monitor_can_read_analytics() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 1, |cfg| {
        cfg.admin_threshold = 1;
    });

    let monitor = signers.get(0).unwrap();
    assign_roles(&env, &contract_id, &signers, vec![(0, AdminRole::Monitor)]);

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(monitor);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_with_permission(
            &env,
            &test_signers,
            AdminPermission::ReadAnalytics,
        )
    });

    assert!(result.is_ok(), "Monitor should be able to read analytics");
}

#[test]
fn test_all_signers_must_have_permission() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 2, |cfg| {
        cfg.admin_threshold = 2;
    });

    let admin1 = signers.get(0).unwrap();
    let admin2 = signers.get(1).unwrap();

    assign_roles(&env, &contract_id, &signers, vec![
        (0, AdminRole::SuperAdmin),
        (1, AdminRole::Monitor),
    ]);

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(admin1);
    test_signers.push_back(admin2);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(
            &env,
            &test_signers,
            AdminAction::Pause,
        )
    });

    assert!(result.is_err(), "All signers must have the required permission");
}

#[test]
fn test_superadmin_with_treasurer_can_manage_fees() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 2, |cfg| {
        cfg.admin_threshold = 2;
    });

    let admin1 = signers.get(0).unwrap();
    let admin2 = signers.get(1).unwrap();

    assign_roles(&env, &contract_id, &signers, vec![
        (0, AdminRole::SuperAdmin),
        (1, AdminRole::Treasurer),
    ]);

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(admin1);
    test_signers.push_back(admin2);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(
            &env,
            &test_signers,
            AdminAction::UpdateFees,
        )
    });

    assert!(result.is_ok(), "SuperAdmin + Treasurer should be able to update fees");
}

// ── Migration Tests ───────────────────────────────────────────────────────

#[test]
fn test_backward_compatibility_superadmin_has_all_permissions() {
    let critical_actions = [
        AdminAction::Pause,
        AdminAction::Slash,
        AdminAction::UpdateConfig,
        AdminAction::UpdateFees,
        AdminAction::AddAdmin,
    ];

    for action in critical_actions {
        let required_perm = rbac::get_required_permission(action);
        let has_perm = rbac::check_admin_permission(&AdminRole::SuperAdmin, &required_perm);
        assert!(has_perm, "SuperAdmin should have permission for: {:?}", action);
    }
}

#[test]
fn test_default_role_assignment() {
    let env = Env::default();
    let (_, signers, contract_id) = setup_admin_system(&env, 1, |_| {});
    let admin = signers.get(0).unwrap();

    let mut admins = Vec::new(&env);
    admins.push_back(admin.clone());

    env.as_contract(&contract_id, || {
        rbac::assign_admin_role(&env, admins, admin.clone(), AdminRole::SuperAdmin);

        let retrieved_role = rbac::get_admin_role(&env, &admin);
        assert!(retrieved_role.is_ok());
        assert_eq!(retrieved_role.unwrap(), AdminRole::SuperAdmin);
    });
}

// ── Regression Tests: Ensure Existing Behavior ────────────────────────────

#[test]
fn test_revoked_admin_cannot_act() {
    let env = Env::default();

    let (_, signers, contract_id) = setup_admin_system(&env, 1, |cfg| {
        cfg.admin_threshold = 1;
    });

    let admin = signers.get(0).unwrap();

    let mut test_signers = Vec::new(&env);
    test_signers.push_back(admin.clone());

    let result = env.as_contract(&contract_id, || {
        env.storage()
            .persistent()
            .set(&DataKey::RevokedAdmin(admin.clone()), &true);

        rbac::require_admin_approval_with_permission(
            &env,
            &test_signers,
            AdminPermission::UpdateConfig,
        )
    });

    assert!(result.is_err(), "Revoked admin should not be able to act");
}

#[test]
fn test_unknown_admin_cannot_act() {
    let env = Env::default();

    let (_, _signers, contract_id) = setup_admin_system(&env, 2, |cfg| {
        cfg.admin_threshold = 1;
    });

    let unknown_admin = Address::generate(&env);
    let mut test_signers = Vec::new(&env);
    test_signers.push_back(unknown_admin);

    let result = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_with_permission(
            &env,
            &test_signers,
            AdminPermission::UpdateConfig,
        )
    });

    assert!(result.is_err(), "Unknown admin should not be able to act");
}

// ── Property-Based Test Concepts ──────────────────────────────────────────

#[test]
fn test_permission_hierarchy_is_monotonic() {
    let permissions = [
        AdminPermission::ReadAnalytics,
        AdminPermission::ManageFees,
        AdminPermission::UpdateConfig,
        AdminPermission::Pause,
        AdminPermission::Slash,
    ];

    let roles = [
        AdminRole::Monitor,
        AdminRole::Treasurer,
        AdminRole::Slasher,
        AdminRole::GovernanceOperator,
        AdminRole::SuperAdmin,
    ];

    for perm in &permissions {
        let mut can_access = std::vec::Vec::new();

        for role in &roles {
            if rbac::check_admin_permission(role, perm) {
                can_access.push(role);
            }
        }

        if !can_access.is_empty() {
            assert_eq!(
                *can_access.last().unwrap(),
                &AdminRole::SuperAdmin,
                "SuperAdmin should be able to do everything: {:?}",
                perm
            );
        }
    }
}

#[test]
fn test_every_admin_action_has_defined_permission() {
    let test_actions = [
        AdminAction::Pause,
        AdminAction::Slash,
        AdminAction::UpdateConfig,
        AdminAction::UpdateFees,
        AdminAction::AddAdmin,
        AdminAction::RemoveAdmin,
        AdminAction::SetAdminThreshold,
    ];

    for action in test_actions {
        let perm = rbac::get_required_permission(action);

        assert!(
            matches!(perm,
                AdminPermission::Slash
                | AdminPermission::Pause
                | AdminPermission::UpdateConfig
                | AdminPermission::ManageFees
                | AdminPermission::ReadAnalytics
            ),
            "Action {:?} has undefined permission: {:?}",
            action,
            perm
        );
    }
}

// ── Migration Tests ──────────────────────────────────────────────────────

#[test]
fn test_migrate_legacy_admins_assigns_superadmin() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuorumCreditContract, ());
    let client = crate::QuorumCreditContractClient::new(&env, &contract_id);

    let deployer = Address::generate(&env);
    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);
    let admin3 = Address::generate(&env);

    let mut admins = Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());
    admins.push_back(admin3.clone());

    let token_id = env.register_stellar_asset_contract_v2(admin1.clone());
    client.initialize(&deployer, &admins, &2, &token_id.address());

    env.as_contract(&contract_id, || {
        rbac::migrate_legacy_admins_to_superadmin(&env);

        assert_eq!(
            rbac::get_admin_role(&env, &admin1).unwrap(),
            AdminRole::SuperAdmin
        );
        assert_eq!(
            rbac::get_admin_role(&env, &admin2).unwrap(),
            AdminRole::SuperAdmin
        );
        assert_eq!(
            rbac::get_admin_role(&env, &admin3).unwrap(),
            AdminRole::SuperAdmin
        );
    });
}

#[test]
fn test_migrate_does_not_override_existing_roles() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(crate::QuorumCreditContract, ());
    let client = crate::QuorumCreditContractClient::new(&env, &contract_id);

    let deployer = Address::generate(&env);
    let admin1 = Address::generate(&env);
    let admin2 = Address::generate(&env);

    let mut admins = Vec::new(&env);
    admins.push_back(admin1.clone());
    admins.push_back(admin2.clone());

    let token_id = env.register_stellar_asset_contract_v2(admin1.clone());
    client.initialize(&deployer, &admins, &1, &token_id.address());

    env.as_contract(&contract_id, || {
        rbac::assign_admin_role(&env, admins.clone(), admin1.clone(), AdminRole::Treasurer);

        rbac::migrate_legacy_admins_to_superadmin(&env);

        assert_eq!(
            rbac::get_admin_role(&env, &admin1).unwrap(),
            AdminRole::Treasurer,
            "Migration should not override existing Treasurer role"
        );
        assert_eq!(
            rbac::get_admin_role(&env, &admin2).unwrap(),
            AdminRole::SuperAdmin,
            "Migration should assign SuperAdmin to admin without a role"
        );
    });
}

// ── Issue #1445: Granular Slasher and GovernanceOperator Role Tests ─────────

#[test]
fn test_permission_matrix_slasher() {
    let slasher = AdminRole::Slasher;

    assert!(rbac::check_admin_permission(&slasher, &AdminPermission::Slash));
    assert!(rbac::check_admin_permission(&slasher, &AdminPermission::ReadAnalytics));

    assert!(!rbac::check_admin_permission(&slasher, &AdminPermission::Pause));
    assert!(!rbac::check_admin_permission(&slasher, &AdminPermission::UpdateConfig));
    assert!(!rbac::check_admin_permission(&slasher, &AdminPermission::ManageFees));
}

#[test]
fn test_permission_matrix_governance_operator() {
    let gov_op = AdminRole::GovernanceOperator;

    assert!(rbac::check_admin_permission(&gov_op, &AdminPermission::UpdateConfig));

    assert!(!rbac::check_admin_permission(&gov_op, &AdminPermission::Slash));
    assert!(!rbac::check_admin_permission(&gov_op, &AdminPermission::Pause));
    assert!(!rbac::check_admin_permission(&gov_op, &AdminPermission::ManageFees));
    assert!(!rbac::check_admin_permission(&gov_op, &AdminPermission::ReadAnalytics));
}

#[test]
fn test_slasher_role_enforcement_on_slashing() {
    let env = Env::default();
    let (_, signers, contract_id) = setup_admin_system(&env, 1, |_| {});

    assign_roles(&env, &contract_id, &signers, vec![(0, AdminRole::Slasher)]);

    // Slasher can sign for Slash action
    let slash_res = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(&env, &signers, AdminAction::Slash)
    });
    assert!(slash_res.is_ok());

    // Slasher cannot sign for Pause or UpdateFees
    let pause_res = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(&env, &signers, AdminAction::Pause)
    });
    assert_eq!(pause_res, Err(ContractError::PermissionDenied));

    let fees_res = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(&env, &signers, AdminAction::UpdateFees)
    });
    assert_eq!(fees_res, Err(ContractError::PermissionDenied));
}

#[test]
fn test_governance_operator_role_enforcement() {
    let env = Env::default();
    let (_, signers, contract_id) = setup_admin_system(&env, 1, |_| {});

    assign_roles(&env, &contract_id, &signers, vec![(0, AdminRole::GovernanceOperator)]);

    // GovernanceOperator can sign for UpdateConfig
    let cfg_res = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(&env, &signers, AdminAction::UpdateConfig)
    });
    assert!(cfg_res.is_ok());

    // GovernanceOperator can sign for SetAdminThreshold
    let threshold_res = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(&env, &signers, AdminAction::SetAdminThreshold)
    });
    assert!(threshold_res.is_ok());

    // GovernanceOperator cannot sign for Slash or Pause
    let slash_res = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(&env, &signers, AdminAction::Slash)
    });
    assert_eq!(slash_res, Err(ContractError::PermissionDenied));

    let pause_res = env.as_contract(&contract_id, || {
        rbac::require_admin_approval_for_action(&env, &signers, AdminAction::Pause)
    });
    assert_eq!(pause_res, Err(ContractError::PermissionDenied));
}
