# RBAC Integration Guide for Admin Functions

## Overview

This guide provides step-by-step instructions for integrating role-based access control (RBAC) into all admin functions across the codebase. The pattern is consistent and can be applied mechanically to each function.

## Pattern Template

### Before Integration

```rust
pub fn some_admin_function(env: Env, admin_signers: Vec<Address>, param1: Type1) {
    require_admin_approval(&env, &admin_signers);
    
    // Implementation
    let mut cfg = config(&env);
    cfg.some_field = param1;
    env.storage().instance().set(&DataKey::Config, &cfg);
    
    env.events().publish(...);
}
```

### After Integration

```rust
pub fn some_admin_function(env: Env, admin_signers: Vec<Address>, param1: Type1) {
    // Step 1: Check multisig threshold (existing gate)
    require_admin_approval(&env, &admin_signers);
    
    // Step 2: Check role permissions (new gate)
    if let Err(err) = crate::rbac::require_admin_approval_for_action(
        &env,
        &admin_signers,
        crate::rbac::AdminAction::SomeAction  // Change based on function
    ) {
        panic_with_error!(&env, err);
    }
    
    // Implementation (unchanged)
    let mut cfg = config(&env);
    cfg.some_field = param1;
    env.storage().instance().set(&DataKey::Config, &cfg);
    
    env.events().publish(...);
}
```

## Mapping Admin Functions to AdminActions

### Admin Management Functions

These control the admin set itself and require SuperAdmin permissions (AdminAction::UpdateConfig).

| Function | AdminAction | Required Permission |
|----------|------------|-------------------|
| `add_admin()` | AddAdmin | UpdateConfig |
| `remove_admin()` | RemoveAdmin | UpdateConfig |
| `rotate_admin()` | RotateAdmin | UpdateConfig |
| `set_admin_threshold()` | SetAdminThreshold | UpdateConfig |
| `add_to_admin_whitelist()` | ManageWhitelist | UpdateConfig |
| `remove_from_admin_whitelist()` | ManageWhitelist | UpdateConfig |
| `add_to_admin_blacklist()` | ManageBlacklisted | UpdateConfig |
| `remove_from_admin_blacklist()` | ManageBlacklisted | UpdateConfig |
| `revoke_admin()` | RevokeAdmin | UpdateConfig |

### Pause/Unpause Functions

These control contract pause state and require Pause permission.

| Function | AdminAction | Required Permission |
|----------|------------|-------------------|
| `pause()` | Pause | Pause |
| `begin_thaw()` | Pause | Pause |
| `unpause()` | Unpause | Pause |
| `pause_with_thaw()` | Pause | Pause |

### Configuration Functions

These modify protocol parameters and require UpdateConfig permission (Treasurer level).

| Function | AdminAction | Required Permission |
|----------|------------|-------------------|
| `set_config()` | SetConfig | UpdateConfig |
| `update_config()` | UpdateConfig | UpdateConfig |
| `batch_update_config()` | UpdateConfig | UpdateConfig |
| `set_min_stake()` | SetLoanParams | UpdateConfig |
| `set_max_loan_amount()` | SetLoanParams | UpdateConfig |
| `set_min_vouchers()` | SetLoanParams | UpdateConfig |
| `set_max_loan_to_stake_ratio()` | SetLoanParams | UpdateConfig |
| `set_grace_period()` | SetLoanParams | UpdateConfig |
| `set_dynamic_slash_threshold()` | ManageDynamicSlash | UpdateConfig |
| `set_loan_size_slash_enabled()` | ManageDynamicSlash | UpdateConfig |
| `set_loan_size_slash_max_bps()` | ManageDynamicSlash | UpdateConfig |
| `set_reputation_nft()` | SetReputationNft | UpdateConfig |

### Fee Management Functions

These control fee parameters and require ManageFees permission.

| Function | AdminAction | Required Permission |
|----------|------------|-------------------|
| `set_protocol_fee()` | UpdateFees | ManageFees |
| `set_fee_treasury()` | UpdateFees | ManageFees |
| `add_allowed_token()` | UpdateConfig | UpdateConfig |
| `remove_allowed_token()` | UpdateConfig | UpdateConfig |
| `whitelist_voucher()` | UpdateConfig | UpdateConfig |
| `set_whitelist_enabled()` | UpdateConfig | UpdateConfig |

### Slash Operations

These are dangerous and require Slash permission (SuperAdmin only).

| Function | AdminAction | Required Permission |
|----------|------------|-------------------|
| `execute_slash()` | Slash | Slash |
| `blacklist()` | ManageBlacklisted | UpdateConfig |

### Upgrade Functions

These require SuperAdmin permissions.

| Function | AdminAction | Required Permission |
|----------|------------|-------------------|
| `upgrade()` | Upgrade | UpdateConfig |

## Step-by-Step Integration

### Step 1: Identify the AdminAction

Look up the function in the mapping table above. For example:
- `set_protocol_fee()` → `AdminAction::UpdateFees`
- `pause()` → `AdminAction::Pause`
- `add_admin()` → `AdminAction::AddAdmin`

### Step 2: Add the RBAC Check

In the function body, right after `require_admin_approval()`, add:

```rust
if let Err(err) = crate::rbac::require_admin_approval_for_action(
    &env,
    &admin_signers,
    crate::rbac::AdminAction::YourActionHere  // ← Change this
) {
    panic_with_error!(&env, err);
}
```

### Step 3: Test

Add a test in `rbac_enforcement_test.rs` to verify the permission is enforced:

```rust
#[test]
fn test_monitor_cannot_call_your_function() {
    let env = Env::default();
    let (_, signers) = setup_admin_system(&env, 1, |cfg| {
        cfg.admin_threshold = 1;
    });

    let monitor = signers.get(0).unwrap();
    assign_roles(&env, &signers, vec![(0, AdminRole::Monitor)]);

    let mut test_signers = Vec::new();
    test_signers.push_back(monitor.clone());

    // This should fail
    let result = rbac::require_admin_approval_for_action(
        &env,
        &test_signers,
        AdminAction::YourActionHere,  // ← Change this
    );

    assert!(result.is_err(), "Monitor should not be able to call your_function()");
}
```

## Integration Checklist

Use this checklist to track integration progress:

### Admin Management (9 functions)

- [ ] `add_admin()` - AdminAction::AddAdmin
- [ ] `remove_admin()` - AdminAction::RemoveAdmin
- [ ] `rotate_admin()` - AdminAction::RotateAdmin
- [ ] `set_admin_threshold()` - AdminAction::SetAdminThreshold
- [ ] `add_to_admin_whitelist()` - AdminAction::ManageWhitelist
- [ ] `remove_from_admin_whitelist()` - AdminAction::ManageWhitelist
- [ ] `add_to_admin_blacklist()` - AdminAction::ManageBlacklisted
- [ ] `remove_from_admin_blacklist()` - AdminAction::ManageBlacklisted
- [ ] `revoke_admin()` - AdminAction::RevokeAdmin

### Pause/Unpause (4 functions)

- [ ] `pause()` - AdminAction::Pause
- [ ] `begin_thaw()` - AdminAction::Pause
- [ ] `unpause()` - AdminAction::Unpause
- [ ] `pause_with_thaw()` - AdminAction::Pause

### Configuration (13 functions)

- [ ] `set_config()` - AdminAction::SetConfig
- [ ] `update_config()` - AdminAction::UpdateConfig
- [ ] `batch_update_config()` - AdminAction::UpdateConfig
- [ ] `set_min_stake()` - AdminAction::SetLoanParams
- [ ] `set_max_loan_amount()` - AdminAction::SetLoanParams
- [ ] `set_min_vouchers()` - AdminAction::SetLoanParams
- [ ] `set_max_loan_to_stake_ratio()` - AdminAction::SetLoanParams
- [ ] `set_grace_period()` - AdminAction::SetLoanParams
- [ ] `set_dynamic_slash_threshold()` - AdminAction::ManageDynamicSlash
- [ ] `set_loan_size_slash_enabled()` - AdminAction::ManageDynamicSlash
- [ ] `set_loan_size_slash_max_bps()` - AdminAction::ManageDynamicSlash
- [ ] `set_reputation_nft()` - AdminAction::SetReputationNft
- [ ] `blacklist()` - AdminAction::ManageBlacklisted

### Fee Management (6 functions)

- [ ] `set_protocol_fee()` - AdminAction::UpdateFees ← Already done
- [ ] `set_fee_treasury()` - AdminAction::UpdateFees
- [ ] `add_allowed_token()` - AdminAction::UpdateConfig
- [ ] `remove_allowed_token()` - AdminAction::UpdateConfig
- [ ] `whitelist_voucher()` - AdminAction::UpdateConfig
- [ ] `set_whitelist_enabled()` - AdminAction::UpdateConfig

### Slash Operations (1 function)

- [ ] `execute_slash()` (or similar) - AdminAction::Slash

### Upgrade (1 function)

- [ ] `upgrade()` - AdminAction::Upgrade

### Governance Functions (if applicable)

Governance.rs functions that dispatch admin actions should also be reviewed:
- [ ] Any functions that call admin functions should verify roles

## Common Issues and Solutions

### Issue: "AdminAction not found"

**Cause**: You used an AdminAction that doesn't exist in the enum.

**Solution**: Check the AdminAction enum in rbac.rs. If the action is missing, add it:
```rust
pub enum AdminAction {
    // ... existing actions
    MyNewAction,  // ← Add here
}
```

Then add it to `get_required_permission()`:
```rust
fn get_required_permission(action: AdminAction) -> AdminPermission {
    match action {
        // ... existing matches
        AdminAction::MyNewAction => AdminPermission::UpdateConfig,  // ← Add here
    }
}
```

### Issue: "Tests fail with PermissionDenied"

**Cause**: You added the RBAC check but test signers don't have assigned roles.

**Solution**: In your test, call `assign_roles()` to give signers the right roles:
```rust
assign_roles(&env, &signers, vec![
    (0, AdminRole::SuperAdmin),  // First admin is SuperAdmin
    (1, AdminRole::Treasurer),   // Second admin is Treasurer
]);
```

### Issue: "Backward compatibility broken"

**Cause**: Existing deployments have admins without assigned roles.

**Solution**: Call `migrate_legacy_to_rbac()` on first deployment. This assigns SuperAdmin to all existing admins, restoring backward compatibility.

## Verification

### Pre-Deployment Verification

1. **Code Review**: Verify every call site has the RBAC check
   ```bash
   grep -n "require_admin_approval(&env" src/admin.rs | wc -l  # Count all calls
   grep -n "require_admin_approval_for_action" src/admin.rs | wc -l  # Should match
   ```

2. **Mapping Verification**: Verify all actions map to correct permissions
   ```bash
   cargo test rbac_enforcement_test::test_every_admin_action_has_defined_permission
   ```

3. **Tests**: Run full test suite
   ```bash
   cargo test rbac
   cargo test admin
   ```

### Post-Deployment Verification

1. **Query roles**: Verify existing admins are SuperAdmin
   ```bash
   # Query get_admin_role(admin1) → should return SuperAdmin
   # Query get_admin_role(admin2) → should return SuperAdmin
   ```

2. **Permission enforcement**: Verify Monitor cannot call restricted functions
   ```bash
   # Assign admin2 as Monitor
   # Try to call pause() with Monitor → should fail
   # Try to call get_config() with Monitor → should succeed
   ```

3. **Threshold enforcement**: Verify threshold still works
   ```bash
   # With threshold=3 and only 2 signers → should fail
   # With threshold=2 and 3 signers → should succeed
   ```

## Reference

- `rbac.rs`: Core RBAC implementation
- `rbac_enforcement_test.rs`: Test examples
- `RBAC_IMPLEMENTATION.md`: Detailed design document
- `AdminAction` enum: Maps functions to required permissions
- `get_required_permission()`: Deterministic permission lookup
