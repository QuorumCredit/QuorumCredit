/**
 * Service-to-Service Authentication Module
 *
 * #1758: Mutual authentication for inter-service calls.
 *
 * Provides:
 * - Service registry (register/deregister services)
 * - JWT-based service tokens (HMAC-SHA256, no external library)
 * - Mutual TLS support flag (TLS termination handled by infra; this module
 *   tracks whether mTLS is required for a given service pair)
 * - Per-call tracking (call count, last-seen timestamp)
 */

import { createHmac, randomBytes, timingSafeEqual } from "node:crypto";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface ServiceRegistration {
  serviceId: string;
  secret: string; // HMAC secret for JWT signing
  allowedCallers: string[]; // serviceIds permitted to call this service
  requireMtls: boolean;
  registeredAt: number;
  lastSeenAt?: number;
  callCount: number;
}

export interface ServiceTokenPayload {
  iss: string; // issuing serviceId
  aud: string; // target serviceId
  iat: number;
  exp: number;
  jti: string;
}

export interface ServiceCallRecord {
  callerServiceId: string;
  targetServiceId: string;
  at: number;
  authenticated: boolean;
}

export interface ServiceAuthResult {
  authenticated: boolean;
  callerServiceId?: string;
  reason?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function base64url(buf: Buffer | string): string {
  return Buffer.from(buf).toString("base64url");
}

function hmacSign(secret: string, data: string): string {
  return createHmac("sha256", secret).update(data).digest("base64url");
}

// ---------------------------------------------------------------------------
// ServiceAuthManager
// ---------------------------------------------------------------------------

/**
 * Manages service identity, token issuance, and mutual authentication.
 *
 * Usage:
 *   const mgr = new ServiceAuthManager();
 *   mgr.registerService("svc-a", ["svc-b"]);  // returns { serviceId, secret }
 *   const token = mgr.issueServiceToken("svc-a", "svc-b", secret, 60);
 *   const result = mgr.validateServiceToken(token, "svc-b");
 */
export class ServiceAuthManager {
  private readonly registry = new Map<string, ServiceRegistration>();
  private readonly callLog: ServiceCallRecord[] = [];
  private readonly maxCallLogSize = 10_000;

  // -------------------------------------------------------------------------
  // Registry
  // -------------------------------------------------------------------------

  /**
   * Register a new service. Returns the generated secret — store it securely.
   */
  registerService(
    serviceId: string,
    allowedCallers: string[] = [],
    requireMtls: boolean = false
  ): { serviceId: string; secret: string } {
    if (!serviceId.trim()) throw new Error("serviceId must not be empty");

    const secret = randomBytes(32).toString("hex");
    this.registry.set(serviceId, {
      serviceId,
      secret,
      allowedCallers,
      requireMtls,
      registeredAt: Date.now(),
      callCount: 0,
    });
    return { serviceId, secret };
  }

  /**
   * Deregister a service.
   */
  deregisterService(serviceId: string): boolean {
    return this.registry.delete(serviceId);
  }

  /**
   * Look up a registered service (secret is redacted).
   */
  getService(serviceId: string): Omit<ServiceRegistration, "secret"> | undefined {
    const svc = this.registry.get(serviceId);
    if (!svc) return undefined;
    const { secret: _secret, ...rest } = svc;
    return rest;
  }

  /**
   * List all registered services (secrets redacted).
   */
  listServices(): Omit<ServiceRegistration, "secret">[] {
    return Array.from(this.registry.values()).map(({ secret: _secret, ...rest }) => rest);
  }

  /**
   * Update the allowed callers list for a service.
   */
  updateAllowedCallers(serviceId: string, allowedCallers: string[]): boolean {
    const svc = this.registry.get(serviceId);
    if (!svc) return false;
    svc.allowedCallers = allowedCallers;
    return true;
  }

  // -------------------------------------------------------------------------
  // Token issuance
  // -------------------------------------------------------------------------

  /**
   * Issue a short-lived service JWT.
   *
   * @param issuerServiceId - The calling service's ID.
   * @param audienceServiceId - The target service's ID.
   * @param issuerSecret - The issuer's secret (from registerService).
   * @param ttlSeconds - Token lifetime in seconds (default 60).
   */
  issueServiceToken(
    issuerServiceId: string,
    audienceServiceId: string,
    issuerSecret: string,
    ttlSeconds: number = 60
  ): string {
    const now = Math.floor(Date.now() / 1000);
    const payload: ServiceTokenPayload = {
      iss: issuerServiceId,
      aud: audienceServiceId,
      iat: now,
      exp: now + ttlSeconds,
      jti: randomBytes(16).toString("hex"),
    };
    const encodedHeader = base64url(JSON.stringify({ alg: "HS256", typ: "JWT" }));
    const encodedPayload = base64url(JSON.stringify(payload));
    const signingInput = `${encodedHeader}.${encodedPayload}`;
    const signature = hmacSign(issuerSecret, signingInput);
    return `${signingInput}.${signature}`;
  }

  // -------------------------------------------------------------------------
  // Token validation
  // -------------------------------------------------------------------------

  /**
   * Validate an inbound service token.
   *
   * Checks:
   *  1. Token is well-formed.
   *  2. Audience matches the receiving service.
   *  3. Token has not expired.
   *  4. Issuing service is registered.
   *  5. Signature is valid (using the issuer's registered secret).
   *  6. Caller is in the target service's allowedCallers list.
   *
   * Also records the call for metrics.
   */
  validateServiceToken(token: string, receivingServiceId: string): ServiceAuthResult {
    const parts = token.split(".");
    if (parts.length !== 3) {
      return { authenticated: false, reason: "malformed token" };
    }

    const [encodedHeader, encodedPayload, signature] = parts;

    let payload: ServiceTokenPayload;
    try {
      payload = JSON.parse(Buffer.from(encodedPayload, "base64url").toString("utf8"));
    } catch {
      return { authenticated: false, reason: "malformed payload" };
    }

    // Audience check
    if (payload.aud !== receivingServiceId) {
      return { authenticated: false, reason: "audience mismatch" };
    }

    // Expiry check
    if (!payload.exp || payload.exp * 1000 < Date.now()) {
      return { authenticated: false, reason: "token expired" };
    }

    // Issuer must be registered
    const issuerReg = this.registry.get(payload.iss);
    if (!issuerReg) {
      this.recordCall(payload.iss, receivingServiceId, false);
      return { authenticated: false, reason: "unknown issuer" };
    }

    // Signature check
    const signingInput = `${encodedHeader}.${encodedPayload}`;
    const expected = hmacSign(issuerReg.secret, signingInput);
    const provided = Buffer.from(signature);
    const expectedBuf = Buffer.from(expected);
    if (
      provided.length !== expectedBuf.length ||
      !timingSafeEqual(provided, expectedBuf)
    ) {
      this.recordCall(payload.iss, receivingServiceId, false);
      return { authenticated: false, reason: "invalid signature" };
    }

    // Caller authorisation check on the target service
    const targetReg = this.registry.get(receivingServiceId);
    if (targetReg) {
      const allowed =
        targetReg.allowedCallers.length === 0 ||
        targetReg.allowedCallers.includes(payload.iss);
      if (!allowed) {
        this.recordCall(payload.iss, receivingServiceId, false);
        return { authenticated: false, reason: "caller not in allowedCallers" };
      }
      // Update metrics on target
      targetReg.lastSeenAt = Date.now();
      targetReg.callCount += 1;
    }

    this.recordCall(payload.iss, receivingServiceId, true);
    return { authenticated: true, callerServiceId: payload.iss };
  }

  // -------------------------------------------------------------------------
  // mTLS flag helpers
  // -------------------------------------------------------------------------

  /**
   * Returns true if the target service requires mTLS for inbound calls.
   * Actual TLS termination is handled by the infrastructure layer (e.g.
   * Envoy / a load balancer); this flag lets application code enforce the
   * policy at the service level.
   */
  requiresMtls(serviceId: string): boolean {
    return this.registry.get(serviceId)?.requireMtls ?? false;
  }

  setRequireMtls(serviceId: string, required: boolean): boolean {
    const svc = this.registry.get(serviceId);
    if (!svc) return false;
    svc.requireMtls = required;
    return true;
  }

  // -------------------------------------------------------------------------
  // Call tracking
  // -------------------------------------------------------------------------

  /**
   * Return recent service-to-service call records.
   */
  getCallLog(limit: number = 100): ServiceCallRecord[] {
    return this.callLog.slice(-limit);
  }

  /**
   * Return aggregate call statistics per (caller, target) pair.
   */
  getCallStats(): Record<string, { total: number; authenticated: number; rejected: number }> {
    const stats: Record<string, { total: number; authenticated: number; rejected: number }> = {};
    for (const record of this.callLog) {
      const key = `${record.callerServiceId}->${record.targetServiceId}`;
      if (!stats[key]) stats[key] = { total: 0, authenticated: 0, rejected: 0 };
      stats[key].total += 1;
      if (record.authenticated) stats[key].authenticated += 1;
      else stats[key].rejected += 1;
    }
    return stats;
  }

  private recordCall(callerServiceId: string, targetServiceId: string, authenticated: boolean): void {
    this.callLog.push({ callerServiceId, targetServiceId, at: Date.now(), authenticated });
    if (this.callLog.length > this.maxCallLogSize) {
      this.callLog.splice(0, this.callLog.length - this.maxCallLogSize);
    }
  }
}

export const serviceAuthManager = new ServiceAuthManager();
