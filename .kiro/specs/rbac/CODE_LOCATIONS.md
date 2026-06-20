# RBAC Code Locations (Issue #16)

## Type Definitions

**File: `src/types.rs`**

```rust
// Lines 122-128: AdminRole enum
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdminRole {
    SuperAdmin,
    Treasurer,
    Monitor,
}

// Lines 130-138: AdminPermission enum
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdminPermission {
    Slash,
    Pause,
    UpdateConfig,
    ManageFees,
    ReadAnalytics,
}

// Lines 140-144: PermissionMatrix struct
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionMatrix {
    pub role: AdminRole,
    pub permissions: Vec<AdminPermission>,
}

// Line 368: Storage key for admin roles
DataKey::AdminRole(Address),  // admin address -> AdminRole
```

## Core RBAC Implementation

**File: `src/rbac.rs`** (NEW - 95 lines)

```rust
// Lines 7-22: assign_admin_role function
pub fn assign_admin_role(
    env: &Env,
    admin_signers: Vec<Address>,
    target_admin: Address,
    role: AdminRole,
)

// Lines 25-30: get_admin_role function
pub fn get_admin_role(env: &Env, admin: &Address) -> Result<AdminRole, ContractError>

// Lines 33-50: require_admin_permission function (ENFORCES PERMISSIONS)
pub fn require_admin_permission(
    env: &Env,
    admin: &Address,
    permission: AdminPermission,
) -> Result<(), ContractError>

// Lines 53-95: Inline unit tests (3 tests)
#[cfg(test)]
mod tests {
    // test_superadmin_all_permissions
    // test_treasurer_config_and_fees
    // test_monitor_read_only
}
```

## Contract Integration

**File: `src/lib.rs`**

```rust
// Line 11: Module declaration
pub mod rbac;

// Line 16: Test module declaration
#[cfg(test)]
mod rbac_test;

// Lines 1242-1249: assign_admin_role contract function
pub fn assign_admin_role(
    env: Env,
    admin_signers: Vec<Address>,
    target_admin: Address,
    role: AdminRole,
) {
    rbac::assign_admin_role(&env, admin_signers, target_admin, role)
}

// Lines 1251-1253: get_admin_role contract function
pub fn get_admin_role(env: Env, admin: Address) -> Result<AdminRole, ContractError> {
    rbac::get_admin_role(&env, &admin)
}
```

## Test Suite

**File: `src/rbac_test.rs`** (NEW - 380+ lines)

```rust
// Line 1: Module declaration
#[cfg(test)]
mod tests {
    // Helper: setup_env_with_admins() - Lines 8-30

    // Role Assignment Tests (4 tests) - Lines 33-75
    #[test] fn test_assign_superadmin_role()
    #[test] fn test_assign_treasurer_role()
    #[test] fn test_assign_monitor_role()
    #[test] fn test_change_admin_role()

    // SuperAdmin Permission Tests (5 tests) - Lines 78-132
    #[test] fn test_superadmin_can_slash()
    #[test] fn test_superadmin_can_pause()
    #[test] fn test_superadmin_can_update_config()
    #[test] fn test_superadmin_can_manage_fees()
    #[test] fn test_superadmin_can_read_analytics()

    // Treasurer Permission Tests (5 tests) - Lines 135-189
    #[test] fn test_treasurer_can_update_config()
    #[test] fn test_treasurer_can_manage_fees()
    #[test] fn test_treasurer_cannot_slash()
    #[test] fn test_treasurer_cannot_pause()
    #[test] fn test_treasurer_cannot_read_analytics()

    // Monitor Permission Tests (5 tests) - Lines 192-246
    #[test] fn test_monitor_can_read_analytics()
    #[test] fn test_monitor_cannot_slash()
    #[test] fn test_monitor_cannot_pause()
    #[test] fn test_monitor_cannot_update_config()
    #[test] fn test_monitor_cannot_manage_fees()

    // Edge Cases (2+ tests) - Lines 249-380
    #[test] fn test_unassigned_admin_permission_denied()
    #[test] fn test_all_role_permission_combinations()
    #[test] fn test_audit_logging_on_role_assignment()
}
```

## Documentation Files

**File: `.kiro/specs/rbac/implementation.md`**
- High-level overview of RBAC architecture
- Role and permission definitions
- Integration points

**File: `.kiro/specs/rbac/test-coverage.md`**
- Breakdown of 19 tests by category
- Permission matrix coverage table
- Test execution checklist

**File: `.kiro/specs/rbac/INTEGRATION_GUIDE.md`**
- How to add permission checks to admin functions
- Event monitoring examples
- Role assignment workflow
- Migration strategy
- Debugging guide

**File: `.kiro/specs/rbac/VERIFICATION.md`**
- Complete verification checklist
- All 8 requirements confirmed
- All 19 tests listed
- Quality metrics

**File: `.kiro/specs/rbac/CODE_LOCATIONS.md`**
- This file - exact line numbers for all code

**File: `/workspaces/QuorumCredit/RBAC_DELIVERY.md`**
- Executive summary
- All deliverables listed
- Quality metrics
- Next steps

## Error Codes

Uses existing error: `ContractError::PermissionDenied = 60`

**File: `src/errors.rs` line 60**
```rust
/// Caller does not have the required role or permission.
PermissionDenied = 60,
```

## Event Definitions

**File: `src/rbac.rs` lines 19-22**

Event published on role assignment:
- **Topic**: ("admin", "role_assigned")
- **Data**: (assigner: Address, target: Address, role: AdminRole)

## Summary Statistics

| Item | Location | Line(s) | Count |
|------|----------|---------|-------|
| AdminRole enum | types.rs | 122-128 | 1 |
| AdminPermission enum | types.rs | 130-138 | 1 |
| PermissionMatrix struct | types.rs | 140-144 | 1 |
| DataKey::AdminRole | types.rs | 368 | 1 |
| rbac module | lib.rs | 11 | 1 |
| Test module | lib.rs | 16 | 1 |
| Contract functions | lib.rs | 1242-1253 | 2 |
| Core functions | rbac.rs | 7-50 | 3 |
| Inline tests | rbac.rs | 53-95 | 3 |
| Test file | rbac_test.rs | Full | 19 |
| **Total** | | | **36** |

## Running Tests

```bash
# Run RBAC tests
cd /workspaces/QuorumCredit
cargo test rbac_test -- --test-threads=1

# Run with output
cargo test rbac_test -- --test-threads=1 --nocapture

# Run single test
cargo test test_superadmin_can_slash
```

## Key Functions Quick Reference

| Function | Module | Purpose |
|----------|--------|---------|
| assign_admin_role() | rbac | Assign role to admin (requires quorum) |
| get_admin_role() | rbac | Retrieve admin's role |
| require_admin_permission() | rbac | Enforce permission (used in admin ops) |
| AdminRole enum | types | Define the 3 roles |
| AdminPermission enum | types | Define the 5 permissions |
| DataKey::AdminRole | types | Storage key for role mapping |

## Integration Example

To add permission check to an admin operation:

```rust
// Before (in admin.rs):
pub fn slash(env: Env, admin_signers: Vec<Address>, borrower: Address) {
    require_admin_approval(&env, &admin_signers);
    // ... slash logic
}

// After (with RBAC):
pub fn slash(env: Env, admin_signers: Vec<Address>, borrower: Address) {
    require_admin_approval(&env, &admin_signers);
    rbac::require_admin_permission(&env, &admin_signers[0], AdminPermission::Slash)?;
    // ... slash logic
}
```

No other changes needed!
