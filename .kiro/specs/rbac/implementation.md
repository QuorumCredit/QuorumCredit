# RBAC Implementation (Issue #16)

## Overview
Implemented Role-Based Access Control (RBAC) with three admin roles (SuperAdmin, Treasurer, Monitor) and granular permissions.

## Architecture

### Enums

#### AdminRole
Three distinct admin roles with hierarchical permissions:
- **SuperAdmin**: All operations (slash, pause, config, fees, analytics)
- **Treasurer**: Config and fee management only
- **Monitor**: Read-only analytics access

#### AdminPermission
Five granular permissions:
- `Slash`: Authority to slash vouchers
- `Pause`: Authority to pause/unpause contract
- `UpdateConfig`: Authority to modify protocol config
- `ManageFees`: Authority to manage protocol fees
- `ReadAnalytics`: Authority to read analytics (Monitor only)

### Data Storage

**DataKey::AdminRole(Address)** → AdminRole
Stores the assigned role for each admin address. One role per admin.

### Functions

#### `assign_admin_role(admin_signers, target_admin, role)`
- Requires admin quorum approval
- Assigns or reassigns a role to an admin
- Emits `AdminRoleAssigned` event for audit trail
- Can change roles dynamically

#### `get_admin_role(admin) -> Result<AdminRole>`
- Reads the role for an admin
- Returns `PermissionDenied` if no role assigned
- Used before enforcing permissions

#### `require_admin_permission(admin, permission) -> Result<()>`
- Permission enforcement check
- SuperAdmin: always passes
- Treasurer: passes for UpdateConfig, ManageFees
- Monitor: passes only for ReadAnalytics
- Returns `PermissionDenied` for insufficient permissions

## Permission Matrix

| Permission | SuperAdmin | Treasurer | Monitor |
|--|--|--|--|
| Slash | ✓ | ✗ | ✗ |
| Pause | ✓ | ✗ | ✗ |
| UpdateConfig | ✓ | ✓ | ✗ |
| ManageFees | ✓ | ✓ | ✗ |
| ReadAnalytics | ✓ | ✗ | ✓ |

## Integration Points

### In Admin Functions
Admin operations should call `require_admin_permission` before execution:

```rust
pub fn slash(...) {
    rbac::require_admin_permission(&env, &admin, AdminPermission::Slash)?;
    // ... perform slash
}
```

### Event Audit Trail
Role assignments emit events with:
- Admin who assigned the role
- Target admin receiving the role
- New role assigned

```
Event: ("admin", "role_assigned")
Data: (assigner, target, role)
```

## Testing

16+ comprehensive tests covering:

1. **Role Assignment (4 tests)**
   - Assign each role (SuperAdmin, Treasurer, Monitor)
   - Change role dynamically

2. **SuperAdmin Permissions (5 tests)**
   - All 5 permissions available to SuperAdmin

3. **Treasurer Permissions (5 tests)**
   - Can access: UpdateConfig, ManageFees
   - Cannot access: Slash, Pause, ReadAnalytics

4. **Monitor Permissions (5 tests)**
   - Can access: ReadAnalytics
   - Cannot access: all others

5. **Permission Matrix Coverage (9 tests)**
   - All 3 roles × 3 sampled permissions

6. **Edge Cases (2+ tests)**
   - Unassigned admin gets PermissionDenied
   - Admin with no role denied all operations
   - Audit logging on role assignment

## Usage Example

```rust
// Assign Treasurer role to treasury_admin
assign_admin_role(env, admin_signers, treasury_admin, AdminRole::Treasurer);

// Later: Check permission before allowing config change
require_admin_permission(env, &admin, AdminPermission::UpdateConfig)?;
config::update_config(...);

// Monitor can only read analytics
require_admin_permission(env, &monitor, AdminPermission::ReadAnalytics)?;
get_metrics();
```

## Future Extensions

The design is extensible for:
- Custom roles via role registry
- Dynamic permission assignment
- Time-based role expiration
- Delegation to sub-admins with limited scope
