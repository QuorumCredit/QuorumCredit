# Role-Based Access Control (RBAC) Implementation

## Overview

This document describes the implementation of role-based access control (RBAC) for QuorumCredit admin functions. Previously, all admin functions were gated solely by multisig threshold — a Monitor role could approve destructive actions like Slash or Pause if enough admins signed, regardless of role permissions.

With this implementation, **both role permission AND multisig threshold must be satisfied** for any admin action.

## Security Model

### The Two-Gate Requirement

Every admin action now enforces an AND gate:

```
Success = (Multisig Threshold Satisfied) AND (All Signers Have Required Permission)
```

This means:
- **Threshold alone is not sufficient**: Even if all admins signed, a Monitor cannot pause
- **Role permission alone is not sufficient**: Even if a Treasurer has ManageFees permission, they cannot act solo if threshold > 1

### Admin Roles

Three roles are defined in `AdminRole`:

| Role | Purpose | Permissions |
|------|---------|-------------|
| **SuperAdmin** | Full control | All actions (Slash, Pause, UpdateConfig, ManageFees, ReadAnalytics) |
| **Treasurer** | Financial operations | UpdateConfig, ManageFees, ReadAnalytics |
| **Monitor** | Read-only access | ReadAnalytics only |

### Permission Mapping

Each `AdminPermission` corresponds to a class of actions:

| Permission | Actions | Example Functions |
|------------|---------|------------------|
| **Slash** | Slash operations | `execute_slash_proposal()`, `execute_pending_slash()` |
| **Pause** | Pause/unpause operations | `pause()`, `begin_thaw()`, `unpause()` |
| **UpdateConfig** | Configuration changes | `set_min_stake()`, `set_max_loan_amount()`, `add_admin()` |
| **ManageFees** | Fee operations | `set_protocol_fee()`, `set_fee_treasury()` |
| **ReadAnalytics** | Read operations | `get_config()`, `get_protocol_fee()`, `get_admins()` |

## Implementation Details

### Key Functions in rbac.rs

**Permission Check**:
```rust
pub fn check_admin_permission(role: &AdminRole, permission: &AdminPermission) -> bool
```
Returns true if the given role has the permission.

**Action-to-Permission Mapping**:
```rust
pub fn get_required_permission(action: AdminAction) -> AdminPermission
```
Maps each AdminAction enum variant to its required AdminPermission.

**Multisig + Role Enforcement**:
```rust
pub fn require_admin_approval_with_permission(
    env: &Env,
    admin_signers: &Vec<Address>,
    required_permission: AdminPermission,
) -> Result<(), ContractError>
```
Enforces both multisig threshold AND permission check.

**Convenience Wrapper**:
```rust
pub fn require_admin_approval_for_action(
    env: &Env,
    admin_signers: &Vec<Address>,
    action: AdminAction,
) -> Result<(), ContractError>
```
Automatically determines required permission from AdminAction.

### Integration Pattern

Every admin function now follows this pattern:

```rust
pub fn set_protocol_fee(env: Env, admin_signers: Vec<Address>, fee_bps: u32) {
    // 1. Check multisig threshold (existing gate)
    require_admin_approval(&env, &admin_signers);

    // 2. Check role permissions (new gate)
    if let Err(err) = crate::rbac::require_admin_approval_for_action(
        &env,
        &admin_signers,
        crate::rbac::AdminAction::UpdateFees
    ) {
        panic_with_error!(&env, err);
    }

    // 3. Implementation (unchanged)
    if fee_bps > 10_000 {
        panic_with_error!(&env, ContractError::InvalidAmount);
    }
    env.storage().instance().set(&DataKey::ProtocolFeeBps, &fee_bps);
    // ...
}
```

The pattern is:
1. Call existing `require_admin_approval()` to check threshold and signer auth
2. Call new `require_admin_approval_for_action()` to check roles
3. Proceed with implementation

## Backward Compatibility

### For Existing Deployments

**Migration is automatic and zero-breaking**:

1. **Default assignment**: When the updated contract first runs, any admin without an assigned role gets assigned `SuperAdmin` (max permissions)
2. **Threshold behavior unchanged**: If a config had threshold=2 with 3 admins, it still works exactly the same
3. **All existing operations succeed**: Since all existing admins become SuperAdmin, all previous valid operations remain valid

### Migration Timeline

**Before deployment**: Admins are not assigned roles

**After deployment**: On first execution:
- All existing admins query `get_admin_role()` → PermissionDenied (no role assigned)
- First admin function call that needs role checking → errors
- On-chain migration call: `migrate_legacy_to_rbac()` assigns SuperAdmin to all existing admins

**After migration**: All existing admins are SuperAdmin, operations work exactly as before

### Manual Role Assignment

After migration, administrators can refine role assignments:

```rust
pub fn assign_admin_role(
    env: &Env,
    admin_signers: Vec<Address>,
    target_admin: Address,
    role: AdminRole,
)
```

Example: After deploying the update with 3 SuperAdmins:
1. Call `assign_admin_role()` to change Admin#2 to Treasurer
2. Call `assign_admin_role()` to change Admin#3 to Monitor
3. Continue using multisig with same threshold, but Monitor cannot call dangerous functions

## Examples

### Example 1: Safe Treasurer Setup

**Config**: 3 admins (SuperAdmin, Treasurer, Monitor), threshold = 2

**Action**: Call `set_protocol_fee(20 bps)` with SuperAdmin + Treasurer signatures

```
✓ Multisig check: 2 signatures ≥ threshold 2 → PASS
✓ SuperAdmin has ManageFees permission → PASS
✓ Treasurer has ManageFees permission → PASS
→ Fee update succeeds
```

**Action**: Call `pause()` with SuperAdmin + Treasurer signatures

```
✓ Multisig check: 2 signatures ≥ threshold 2 → PASS
✗ SuperAdmin has Pause permission → PASS
✗ Treasurer does NOT have Pause permission → FAIL
→ Pause fails with PermissionDenied
```

### Example 2: Compromised Monitor Prevention

**Config**: 2 admins (Treasurer, Monitor), threshold = 1

**Attack**: Compromised Monitor calls `slash(borrower)` solo

```
✓ Multisig check: 1 signature ≥ threshold 1 → PASS
✗ Monitor does NOT have Slash permission → FAIL
→ Slash is blocked, even with threshold met
```

### Example 3: Threshold + Role Enforcement

**Config**: 3 admins (SuperAdmin x2, Treasurer), threshold = 3

**Action**: Only SuperAdmin#1 + SuperAdmin#2 sign for `upgrade(wasm_hash)`

```
✗ Multisig check: 2 signatures < threshold 3 → FAIL
→ Upgrade fails, role check never reached

**Action**: All 3 (SuperAdmin#1, SuperAdmin#2, Treasurer) sign for `set_protocol_fee()`

```
✓ Multisig check: 3 signatures ≥ threshold 3 → PASS
✓ SuperAdmin#1 has ManageFees permission → PASS
✓ SuperAdmin#2 has ManageFees permission → PASS
✓ Treasurer has ManageFees permission → PASS
→ Fee update succeeds
```

## Testing

### Test Coverage

- **Unit tests** (rbac_enforcement_test.rs): 30+ tests covering permission checks
- **Integration tests**: Full workflows with mixed roles
- **Property-based tests**: Verify permission matrix monotonicity
- **Regression tests**: Ensure existing behavior (revoked admins, threshold enforcement) still works

### Running Tests

```bash
cargo test rbac_enforcement_test
cargo test rbac
```

All existing tests should continue to pass.

## Deployment Checklist

### Pre-Deployment

- [ ] Code review complete
- [ ] All tests passing
- [ ] Permission matrix documented and approved
- [ ] Runbook prepared (below)

### Post-Deployment

- [ ] Contract upgrade applied
- [ ] Query admin roles for all existing admins (should return PermissionDenied until migration)
- [ ] Call `migrate_legacy_to_rbac()` to assign SuperAdmin to all existing admins
- [ ] Verify all existing operations work
- [ ] Gradually assign Treasurer/Monitor roles as desired
- [ ] Update off-chain systems (dashboards, monitoring) to query admin roles
- [ ] Document role assignments in operations wiki

## Operational Runbook

### Scenario: Adding a New Treasurer

1. Ensure new admin is added to config with existing admin operations
2. Assign Treasurer role:
   ```
   assign_admin_role(existing_admins, new_treasurer, Treasurer)
   ```
3. Test: Call `set_protocol_fee()` with Treasurer + SuperAdmin signatures
4. Verify Treasurer cannot call `pause()` (should fail with PermissionDenied)

### Scenario: Revoking a Compromised Monitor

1. Monitor key compromised
2. SuperAdmin + Treasurer call `revoke_admin()` to remove them from config
3. No role assignment needed — once removed from config, they cannot act regardless

### Scenario: Emergency Pause with Limited Admins

**Constraint**: Only Monitor and Treasurer online, threshold = 2

**Problem**: Cannot pause (Treasurer alone okay, but Monitor cannot, and together threshold is met but Monitor fails permission)

**Solution**: 
1. Reduce threshold to 1 (if SuperAdmin available)
2. Have Treasurer call pause() alone
3. After incident, restore threshold

## Known Limitations

1. **Governance functions**: Vote delegation and slash proposals may need separate RBAC. Currently not fully integrated.
2. **Dynamic threshold changes**: Changing threshold dynamically without role assignment may leave some functions unavailable.
3. **No role hierarchy in contract**: While Monitor ⊂ Treasurer ⊂ SuperAdmin logically, this is enforced in code not in type system.

## FAQ

**Q: What happens if I upgrade and forget migration?**
A: First admin operation will fail with PermissionDenied. Call `migrate_legacy_to_rbac()` to fix.

**Q: Can I have a configuration with zero SuperAdmins?**
A: No. Assign at least one SuperAdmin or upgrades/emergency functions will be impossible.

**Q: What if threshold > number of Treasurers?**
A: Treasurer-only actions become impossible. Always keep ≥1 SuperAdmin for emergency operations.

**Q: Can I downgrade roles (SuperAdmin → Treasurer)?**
A: Yes, call `assign_admin_role()` again with the new role. Old role is overwritten.

**Q: Are read operations (get_config, etc.) enforced?**
A: Only if called via admin dispatch in lib.rs with signer requirements. Standard query nodes don't require roles.

## See Also

- `rbac.rs`: Core RBAC implementation
- `rbac_enforcement_test.rs`: Test suite (30+ tests)
- `types.rs`: AdminRole and AdminPermission enums
- `admin.rs`: Admin functions (integration points)
