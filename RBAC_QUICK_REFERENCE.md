# RBAC Quick Reference Card

## Permission Matrix at a Glance

```
Action                          | SuperAdmin | Treasurer | Monitor
────────────────────────────────┼────────────┼───────────┼────────
Admin Management (Add/Remove)   |     ✓      |     ✗     |    ✗
Slash Operations                |     ✓      |     ✗     |    ✗
Pause/Unpause                   |     ✓      |     ✗     |    ✗
Upgrade Contract                |     ✓      |     ✗     |    ✗
Set Config/Parameters           |     ✓      |     ✓     |    ✗
Manage Fees                      |     ✓      |     ✓     |    ✗
Read/Query Operations           |     ✓      |     ✓     |    ✓
```

## Role Descriptions

| Role | Purpose | When to Use |
|------|---------|-----------|
| **SuperAdmin** | Full control, all operations | Primary admin keys, emergency operations |
| **Treasurer** | Financial operations, configuration | Financial officer role, config management |
| **Monitor** | Read-only access, analytics | Monitoring systems, observability tools |

## Common Tasks

### Task: Assign a Treasurer Role

```
Caller: SuperAdmin(s) with threshold signature
Action: assign_admin_role(treasury_admin, Treasurer)
Result: Treasury admin can call set_protocol_fee(), set_min_stake(), etc.
```

### Task: Pause Emergency (All Admins Paused)

```
Who Can Do It: SuperAdmin only
Action: pause()
Note: If all active admins are Monitor/Treasurer, pause is impossible → keep 1 SuperAdmin
```

### Task: Set Protocol Fee

```
Who Can Do It: SuperAdmin or Treasurer (or both)
Threshold: Must meet admin_threshold minimum signatures
Permission: ManageFees (Treasurer level)
Action: set_protocol_fee(new_bps)
```

### Task: Remove Compromised Monitor

```
Who Can Do It: SuperAdmin(s)
Action: remove_admin(compromised_monitor)
Note: Even though Monitor is read-only, it's good hygiene
```

### Task: Read Configuration

```
Who Can Do It: Anyone (no role required for queries)
Action: get_config()
Note: Read operations don't require admin signature
```

## Troubleshooting

### "PermissionDenied" Error

**Cause**: Signer doesn't have required permission for the action

**Solution**:
1. Check signer role: `get_admin_role(signer)`
2. Check required permission for action (see matrix)
3. If role doesn't match, upgrade role: `assign_admin_role(signer, NewRole)`

### "UnauthorizedCaller" Error

**Cause**: One of two issues:
- Signer not in admin list
- Signer count below threshold

**Solution**:
1. Verify all signers are in `get_admins()`
2. Verify number of signers ≥ `get_admin_threshold()`
3. Verify no signer is revoked (checked automatically)

### "Operation impossible with current admin setup"

**Cause**: Role restrictions prevent operation

**Example**: All admins are Monitor, trying to pause
- **Why**: Monitor lacks Pause permission

**Solution**:
1. Assign at least one SuperAdmin: `assign_admin_role(admin, SuperAdmin)`
2. Retry operation with SuperAdmin in signature set

## Permission Enforcement Rules

```
Success = (Threshold Met) AND (All Signers Have Permission)
```

Both conditions required:
- ✓ Threshold: Enough signers? (e.g., 2 of 3)
- ✓ Permission: Do all signers have role for this action?

If either fails → PermissionDenied

## Migration from Old Contract

**Existing admins**: Auto-assigned SuperAdmin on upgrade
**No action needed**: All previous operations work
**Then**: Gradually assign Treasurer/Monitor roles as desired

Call `migrate_legacy_admins_to_superadmin()` if needed.

## Role Assignment Examples

```
# Setup: 3 admins, threshold = 2, all currently SuperAdmin

# Make admin2 a Treasurer
assign_admin_role([admin1, admin3], admin2, Treasurer)
  → admin2 can now set fees, config but NOT pause, slash

# Make admin3 a Monitor  
assign_admin_role([admin1, admin2], admin3, Monitor)
  → admin3 can only read; cannot perform any state-changing operations

# Result: Mixed roles with threshold enforcement
  - pause() needs: 2 signatures minimum + all must have Pause permission
    → admin1 (SuperAdmin) + admin2 (Treasurer) = FAILS (admin2 lacks Pause)
    → admin1 (SuperAdmin) + admin3 (Monitor) = FAILS (admin3 lacks Pause)
    → admin1 must have another SuperAdmin to pause

# Solution: Keep 2+ SuperAdmins for dangerous operations
```

## Dangerous Configurations to Avoid

| Config | Risk | Fix |
|--------|------|-----|
| threshold = N, admins = N Treasurers | Cannot pause | Keep 1 SuperAdmin |
| All admins Monitor | Cannot do anything | Assign 1 SuperAdmin |
| threshold = 3, only 2 SuperAdmins exist | Cannot upgrade | Lower threshold or add SuperAdmin |
| threshold = 1, 1 Monitor admin | Monitor can read but cannot act (OK) | All read operations safe |

## Testing a New Role Assignment

```
# Test: Can Treasurer call set_protocol_fee()?

1. Assign: assign_admin_role([superadmin], treasurer, Treasurer)
2. Call: set_protocol_fee([treasurer], 50)
3. Result: Should succeed (Treasurer has ManageFees permission)

# Test: Can Treasurer call pause()?

1. Assign: assign_admin_role([superadmin], treasurer, Treasurer)
2. Call: pause([treasurer])
3. Result: Should fail with PermissionDenied (Treasurer lacks Pause permission)
```

## Monitoring RBAC Events

**Events to watch**:
- `("admin", "role_assigned")` → Role was assigned
- `("rbac", "legacy_migration_complete")` → Migration ran

**Sample monitoring**:
```
On every admin operation:
  IF event.error == PermissionDenied
    THEN alert("Role permission check failed - check signer roles")
  
  IF event == "role_assigned"
    THEN log("Role changed - verify expected change")
```

## See Also

- **Full Design**: RBAC_IMPLEMENTATION.md
- **Integration Guide**: RBAC_INTEGRATION_GUIDE.md
- **Implementation Status**: RBAC_IMPLEMENTATION_SUMMARY.md
- **Test Examples**: rbac_enforcement_test.rs (30+ tests)

## FAQ

**Q: Can I call operations without being an admin?**  
A: No. All admin operations require an admin signer (registered in config).

**Q: Do read-only queries require a role?**  
A: No. Only mutating operations (pause, update config, etc.) require role checks.

**Q: What if I downgrade a SuperAdmin to Monitor?**  
A: They lose all dangerous permissions. Safe operation. Can be reversed.

**Q: Can threshold change break things?**  
A: No. Threshold and role checks are independent. Lower threshold might make some operations possible (or impossible if you don't have enough SuperAdmins).

**Q: How do I know which role I am?**  
A: Query `get_admin_role(my_address)`. Returns SuperAdmin, Treasurer, or Monitor.
