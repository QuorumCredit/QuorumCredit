/**
 * Role-Based Access Control (RBAC) Enforcer for Zero-Trust
 *
 * #1586: Implements least-privilege access control using role-based
 * permissions.
 */

export type Permission =
  | "read:events"
  | "write:webhooks"
  | "admin:audit_log"
  | "admin:manage_clients"
  | "read:loans"
  | "write:expenses"
  | "read:payments"
  | "write:payments";

export type Role = "viewer" | "operator" | "admin" | "service";

export interface RolePermissions {
  role: Role;
  permissions: Permission[];
  description: string;
}

/**
 * Enforces role-based access control with least-privilege by default.
 */
export class RBACEnforcer {
  private readonly roles = new Map<Role, Set<Permission>>();
  private readonly clientRoles = new Map<string, Role[]>();

  constructor() {
    this.initializeDefaultRoles();
  }

  /**
   * Assign roles to a client (client may have multiple roles).
   */
  assignRole(clientId: string, role: Role): void {
    const roles = this.clientRoles.get(clientId) ?? [];
    if (!roles.includes(role)) {
      roles.push(role);
      this.clientRoles.set(clientId, roles);
    }
  }

  /**
   * Remove a role from a client.
   */
  removeRole(clientId: string, role: Role): void {
    const roles = this.clientRoles.get(clientId) ?? [];
    const idx = roles.indexOf(role);
    if (idx >= 0) {
      roles.splice(idx, 1);
    }
  }

  /**
   * Check if a client has a specific permission.
   */
  hasPermission(clientId: string, permission: Permission): boolean {
    const roles = this.clientRoles.get(clientId) ?? [];

    for (const role of roles) {
      const perms = this.roles.get(role);
      if (perms?.has(permission)) return true;
    }

    return false;
  }

  /**
   * Get all permissions for a client.
   */
  getPermissions(clientId: string): Permission[] {
    const roles = this.clientRoles.get(clientId) ?? [];
    const permissions = new Set<Permission>();

    for (const role of roles) {
      const perms = this.roles.get(role);
      if (perms) {
        for (const perm of perms) {
          permissions.add(perm);
        }
      }
    }

    return Array.from(permissions);
  }

  /**
   * Get roles for a client.
   */
  getRoles(clientId: string): Role[] {
    return this.clientRoles.get(clientId) ?? [];
  }

  /**
   * List all role definitions.
   */
  listRoles(): RolePermissions[] {
    return [
      {
        role: "viewer",
        permissions: ["read:events", "read:loans", "read:payments"],
        description: "Read-only access to public data",
      },
      {
        role: "operator",
        permissions: [
          "read:events",
          "read:loans",
          "read:payments",
          "write:webhooks",
          "write:expenses",
          "write:payments",
        ],
        description: "Operational access for business logic",
      },
      {
        role: "admin",
        permissions: [
          "read:events",
          "write:webhooks",
          "admin:audit_log",
          "admin:manage_clients",
          "read:loans",
          "write:expenses",
          "read:payments",
          "write:payments",
        ],
        description: "Full administrative access",
      },
      {
        role: "service",
        permissions: ["read:events", "write:webhooks", "read:payments", "write:payments"],
        description: "Service-to-service integration access",
      },
    ];
  }

  private initializeDefaultRoles(): void {
    this.roles.set("viewer", new Set(["read:events", "read:loans", "read:payments"]));

    this.roles.set(
      "operator",
      new Set([
        "read:events",
        "read:loans",
        "read:payments",
        "write:webhooks",
        "write:expenses",
        "write:payments",
      ])
    );

    this.roles.set(
      "admin",
      new Set([
        "read:events",
        "write:webhooks",
        "admin:audit_log",
        "admin:manage_clients",
        "read:loans",
        "write:expenses",
        "read:payments",
        "write:payments",
      ])
    );

    this.roles.set(
      "service",
      new Set(["read:events", "write:webhooks", "read:payments", "write:payments"])
    );
  }
}
