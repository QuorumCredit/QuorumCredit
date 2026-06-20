# RBAC Integration Guide

## How to Use in Admin Functions

To add permission enforcement to existing admin functions:

```rust
pub fn slash(env: Env, admin_signers: Vec<Address>, borrower: Address) {
    require_admin_approval(&env, &admin_signers);
    
    // NEW: Add permission check
    rbac::require_admin_permission(&env, &admin_signers[0], AdminPermission::Slash)?;
    
    // ... existing slash logic
}
```

## Permission Check Locations (Recommended)

### Slash Operations
- `slash()` - AdminPermission::Slash
- Location: `admin.rs`

### Pause Operations
- `pause()` - AdminPermission::Pause
- `unpause()` - AdminPermission::Pause
- Location: `admin.rs`

### Configuration Changes
- `set_config()` - AdminPermission::UpdateConfig
- `update_config()` - AdminPermission::UpdateConfig
- `set_min_stake()` - AdminPermission::UpdateConfig
- `set_max_loan_amount()` - AdminPermission::UpdateConfig
- Location: `admin.rs`

### Fee Management
- `set_protocol_fee()` - AdminPermission::ManageFees
- `set_fee_treasury()` - AdminPermission::ManageFees
- Location: `admin.rs`

### Analytics/Monitoring
- `get_metrics()` - AdminPermission::ReadAnalytics
- `get_config()` - AdminPermission::ReadAnalytics
- Location: `admin.rs`

## Event Monitoring

Off-chain indexers can subscribe to RBAC events:

```javascript
// Listen for role assignments
const topic = env.events().filter(e => 
  e[0] === "admin" && e[1] === "role_assigned"
);

topic.forEach(event => {
  const [assigner, target, role] = event.data;
  console.log(`Admin ${assigner} assigned role ${role} to ${target}`);
});
```

## Role Assignment Workflow

1. **SuperAdmin Assigns Role**
   ```rust
   assign_admin_role(
     env,
     vec![superadmin1, superadmin2],  // 2-of-2 approval
     treasurer_address,
     AdminRole::Treasurer
   );
   ```

2. **Role Persisted**
   - DataKey::AdminRole(treasurer_address) → AdminRole::Treasurer

3. **Enforcement on Operation**
   - Treasurer calls config update
   - `require_admin_permission()` checks role
   - Permission granted → execution continues
   - Permission denied → PermissionDenied error (60)

## Testing in Production

Before full deployment:

1. Test each role with sample admin addresses
2. Verify permission denials (Treasurer cannot slash)
3. Verify permission grants (Monitor can read)
4. Monitor audit logs for role changes
5. Validate error codes in client applications

## Migration Strategy

### Phase 1: Deploy RBAC (No Enforcement)
- Deploy code
- Assign roles to existing admins
- No permission checks yet

### Phase 2: Add Checks Gradually
- Enable for non-critical operations first
- Monitor for errors
- Adjust if needed

### Phase 3: Full Enforcement
- Enable all permission checks
- All admin operations require role

## Backward Compatibility

Existing code without RBAC:
- Still works with unassigned admins
- Will get PermissionDenied errors
- Must assign roles to resume operations

Migration path:
```rust
// Old: works (no role check)
slash(env, admins, borrower);

// New: requires permission
slash(env, admins, borrower);
// → Error: PermissionDenied (admin has no role)

// Fix: assign role first
assign_admin_role(env, admins, admin, AdminRole::SuperAdmin);
slash(env, admins, borrower);
// → Success
```

## Debugging

Check admin role:
```rust
match get_admin_role(env, &admin) {
    Ok(role) => println!("Role: {:?}", role),
    Err(e) => println!("Error: {:?}", e),
}
```

Check specific permission:
```rust
match rbac::require_admin_permission(env, &admin, AdminPermission::Slash) {
    Ok(_) => println!("Admin can slash"),
    Err(e) => println!("Cannot slash: {:?}", e),
}
```

## Performance Notes

- Role lookup: O(1) persistent storage read
- Permission check: O(1) enum matching
- No loops or iterations
- Minimal gas overhead

## Security Considerations

1. **Role Storage**: Uses persistent storage (immutable)
2. **Event Audit**: All role changes logged
3. **Admin Quorum**: Role assignment requires full admin quorum
4. **No Delegation**: Only direct role assignment (prevents privilege escalation)
5. **Immutable Permissions**: Permissions per role cannot be changed (add new roles instead)
