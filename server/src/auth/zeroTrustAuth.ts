/**
 * Zero-Trust Authentication Module for QuorumCredit
 *
 * #1586: Zero-trust security model implementation.
 *
 * Implements mutual authentication, request validation, least-privilege
 * access control, and audit logging to follow zero-trust principles.
 *
 * Key principles:
 * - Assume no trust by default (verify every request)
 * - Authenticate and authorize every access
 * - Encrypt all communications
 * - Log and monitor all access
 * - Implement least-privilege access
 */

import { createHmac, timingSafeEqual, randomBytes } from "node:crypto";
import type { IncomingMessage } from "node:http";

export interface ClientCredentials {
  clientId: string;
  clientSecret: string;
  permittedEndpoints: string[];
  createdAt: Date;
  expiresAt?: Date;
}

export interface AuthenticationToken {
  clientId: string;
  issuedAt: number;
  expiresAt: number;
  nonce: string;
  signature: string;
}

export interface ZeroTrustContext {
  clientId: string;
  authenticated: boolean;
  authorized: boolean;
  requestPath: string;
  timestamp: number;
}

export interface AuditLogEntry {
  clientId: string;
  action: string;
  resource: string;
  result: "allowed" | "denied";
  reason?: string;
  timestamp: number;
}

/**
 * Zero-trust authentication service.
 *
 * Verifies client identity and authorization for every request using
 * mutual TLS-style authentication with HMAC-SHA256 signatures.
 */
export class ZeroTrustAuthService {
  private readonly credentials = new Map<string, ClientCredentials>();
  private readonly auditLog: AuditLogEntry[] = [];
  private readonly maxAuditLogSize = 10000;

  /**
   * Register a new client with credentials and permitted endpoints.
   */
  registerClient(
    clientId: string,
    permittedEndpoints: string[],
    expiresAt?: Date
  ): { clientId: string; clientSecret: string } {
    const clientSecret = randomBytes(32).toString("hex");

    this.credentials.set(clientId, {
      clientId,
      clientSecret,
      permittedEndpoints,
      createdAt: new Date(),
      expiresAt,
    });

    return { clientId, clientSecret };
  }

  /**
   * Revoke a client's credentials.
   */
  revokeClient(clientId: string): boolean {
    return this.credentials.delete(clientId);
  }

  /**
   * Get all registered clients (for administrative purposes).
   */
  listClients(): ClientCredentials[] {
    return Array.from(this.credentials.values()).map((c) => ({
      ...c,
      clientSecret: "***hidden***",
    }));
  }

  /**
   * Authenticate a request using client credentials from headers.
   *
   * Returns a ZeroTrustContext with authentication and authorization status.
   */
  authenticateRequest(
    req: IncomingMessage,
    requestPath: string
  ): ZeroTrustContext {
    const clientId = this.extractClientId(req);
    const timestamp = Date.now();

    if (!clientId) {
      this.logAudit(clientId ?? "unknown", "authenticate", requestPath, "denied", "missing client id");
      return {
        clientId: clientId ?? "unknown",
        authenticated: false,
        authorized: false,
        requestPath,
        timestamp,
      };
    }

    const credentials = this.credentials.get(clientId);
    if (!credentials) {
      this.logAudit(clientId, "authenticate", requestPath, "denied", "invalid client");
      return {
        clientId,
        authenticated: false,
        authorized: false,
        requestPath,
        timestamp,
      };
    }

    if (credentials.expiresAt && credentials.expiresAt.getTime() < timestamp) {
      this.logAudit(clientId, "authenticate", requestPath, "denied", "client credentials expired");
      return {
        clientId,
        authenticated: false,
        authorized: false,
        requestPath,
        timestamp,
      };
    }

    const signature = this.extractSignature(req);
    if (!signature || !this.verifySignature(req, signature, credentials.clientSecret)) {
      this.logAudit(clientId, "authenticate", requestPath, "denied", "invalid signature");
      return {
        clientId,
        authenticated: false,
        authorized: false,
        requestPath,
        timestamp,
      };
    }

    const authorized = this.isAuthorized(credentials, requestPath);
    this.logAudit(clientId, "access", requestPath, authorized ? "allowed" : "denied", authorized ? undefined : "insufficient permissions");

    return {
      clientId,
      authenticated: true,
      authorized,
      requestPath,
      timestamp,
    };
  }

  /**
   * Get recent audit log entries.
   */
  getAuditLog(limit: number = 100, clientIdFilter?: string): AuditLogEntry[] {
    let entries = this.auditLog.slice(-limit);
    if (clientIdFilter) {
      entries = entries.filter((e) => e.clientId === clientIdFilter);
    }
    return entries;
  }

  private extractClientId(req: IncomingMessage): string | undefined {
    const authHeader = req.headers["x-client-id"];
    if (typeof authHeader === "string") return authHeader;
    if (Array.isArray(authHeader)) return authHeader[0];
    return undefined;
  }

  private extractSignature(req: IncomingMessage): string | undefined {
    const authHeader = req.headers["x-client-signature"];
    if (typeof authHeader === "string") return authHeader;
    if (Array.isArray(authHeader)) return authHeader[0];
    return undefined;
  }

  private verifySignature(req: IncomingMessage, providedSignature: string, clientSecret: string): boolean {
    try {
      const timestamp = req.headers["x-request-timestamp"];
      const nonce = req.headers["x-request-nonce"];

      if (!timestamp || !nonce) return false;

      const timestampStr = Array.isArray(timestamp) ? timestamp[0] : timestamp;
      const nonceStr = Array.isArray(nonce) ? nonce[0] : nonce;

      const message = `${timestampStr}:${nonceStr}:${req.method}:${req.url}`;
      const expectedSignature = createHmac("sha256", clientSecret).update(message).digest("hex");

      return timingSafeEqual(Buffer.from(providedSignature, "hex"), Buffer.from(expectedSignature, "hex"));
    } catch {
      return false;
    }
  }

  private isAuthorized(credentials: ClientCredentials, requestPath: string): boolean {
    return credentials.permittedEndpoints.some((endpoint) => {
      if (endpoint === "*") return true;
      if (endpoint.endsWith("*")) {
        const prefix = endpoint.slice(0, -1);
        return requestPath.startsWith(prefix);
      }
      return requestPath === endpoint;
    });
  }

  private logAudit(clientId: string, action: string, resource: string, result: "allowed" | "denied", reason?: string): void {
    const entry: AuditLogEntry = {
      clientId,
      action,
      resource,
      result,
      reason,
      timestamp: Date.now(),
    };

    this.auditLog.push(entry);

    if (this.auditLog.length > this.maxAuditLogSize) {
      this.auditLog.splice(0, this.auditLog.length - this.maxAuditLogSize);
    }
  }
}
