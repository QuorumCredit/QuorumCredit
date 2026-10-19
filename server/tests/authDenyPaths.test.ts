import { describe, it, expect } from "vitest";
import type { IncomingMessage } from "node:http";
import { ZeroTrustAuthService } from "../src/auth/zeroTrustAuth.js";
import { RBACEnforcer } from "../src/auth/rbacEnforcer.js";
import { RequestValidator } from "../src/auth/requestValidator.js";
import { ServiceAuthManager } from "../src/auth/serviceAuth.js";
import { tokenExpirationMonitor } from "../src/auth/tokenExpirationMonitor.js";

function requestWith(headers: Record<string, string>): IncomingMessage {
  return { headers } as unknown as IncomingMessage;
}

describe("zero-trust auth denies unknown callers", () => {
  it("denies a request with no client ID", () => {
    const auth = new ZeroTrustAuthService();
    const ctx = auth.authenticateRequest(requestWith({}), "/loans");
    expect(ctx.authenticated).toBe(false);
    expect(ctx.authorized).toBe(false);
  });

  it("denies a client ID that was never registered", () => {
    const auth = new ZeroTrustAuthService();
    const ctx = auth.authenticateRequest(requestWith({ "x-client-id": "ghost" }), "/loans");
    expect(ctx.authenticated).toBe(false);
  });
});

describe("RBAC denies by default", () => {
  it("grants no permission to a client with no roles", () => {
    const rbac = new RBACEnforcer();
    expect(rbac.hasPermission("nobody", "read:loans")).toBe(false);
  });

  it("revokes a permission when the role is removed", () => {
    const rbac = new RBACEnforcer();
    rbac.assignRole("client-a", "viewer");
    rbac.removeRole("client-a", "viewer");
    expect(rbac.hasPermission("client-a", "read:loans")).toBe(false);
  });
});

describe("request validation rejects bad bodies", () => {
  it("rejects a body missing a required field", () => {
    const validator = new RequestValidator();
    validator.registerRules("/loan", [{ field: "amount", type: "number", required: true }]);
    const result = validator.validateBody("/loan", {});
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Missing required field: amount");
  });
});

describe("service auth denies invalid tokens", () => {
  it("rejects a token that was not issued by a registered service", () => {
    const services = new ServiceAuthManager();
    services.registerService("svc-a", ["svc-b"]);
    const result = services.validateServiceToken("not-a-real-token", "svc-b");
    expect(result.authenticated).toBe(false);
  });
});

describe("token expiration monitor reports expired tokens", () => {
  it("reports a token whose expiry is in the past as expired", () => {
    const now = Date.now();
    tokenExpirationMonitor.registerToken("jti-expired", now - 1000, "subject", now - 60_000);
    expect(tokenExpirationMonitor.hasExpired("jti-expired")).toBe(true);
  });

  it("does not report a token with time left as expired", () => {
    const now = Date.now();
    tokenExpirationMonitor.registerToken("jti-valid", now + 3_600_000, "subject", now);
    expect(tokenExpirationMonitor.hasExpired("jti-valid")).toBe(false);
  });
});
