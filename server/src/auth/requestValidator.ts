/**
 * Request Validation Module for Zero-Trust Security
 *
 * #1586: Request validation ensures all inputs conform to expected format
 * and prevent injection attacks.
 */

import type { IncomingMessage } from "node:http";

export interface ValidationRule {
  field: string;
  type: "string" | "number" | "boolean" | "array" | "object";
  required?: boolean;
  maxLength?: number;
  pattern?: RegExp;
  allowedValues?: unknown[];
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  data?: Record<string, unknown>;
}

export interface RequestValidatorOptions {
  /**
   * Whether HTTPS is required for incoming requests.
   * Defaults to `true` (fail-closed) unless `ALLOW_INSECURE_HTTP=true` is set.
   * Set explicitly to `false` only when plain HTTP is intentionally permitted
   * (e.g. local development or isolated testing).
   */
  requireHttps?: boolean;
  /**
   * IP addresses of trusted reverse proxies / TLS-terminating load balancers
   * whose `X-Forwarded-Proto` header may be trusted.
   * Defaults to the comma-separated `TRUSTED_PROXIES` environment variable if set.
   */
  trustedProxies?: string[];
}

function normalizeIp(ip: string): string {
  const trimmed = ip.trim();
  return trimmed.startsWith("::ffff:") ? trimmed.slice(7) : trimmed;
}

function parseTrustedProxiesEnv(): string[] {
  const raw = process.env.TRUSTED_PROXIES;
  if (!raw) return [];
  return raw
    .split(",")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);
}

/**
 * Validates request headers and body against zero-trust rules.
 */
export class RequestValidator {
  private readonly rules = new Map<string, ValidationRule[]>();
  private readonly requireHttps: boolean;
  private readonly trustedProxies: ReadonlySet<string>;

  constructor(options: RequestValidatorOptions = {}) {
    this.requireHttps =
      options.requireHttps ?? process.env.ALLOW_INSECURE_HTTP !== "true";
    const proxies = options.trustedProxies ?? parseTrustedProxiesEnv();
    this.trustedProxies = new Set(proxies.map(normalizeIp));
  }

  /**
   * Determine whether the incoming request arrived over HTTPS, either via
   * direct TLS or via a configured trusted proxy setting `X-Forwarded-Proto: https`.
   */
  private isHttpsRequest(req: IncomingMessage): boolean {
    const socket = req.socket as
      | { encrypted?: boolean; remoteAddress?: string }
      | undefined;

    if (socket?.encrypted === true) {
      return true;
    }

    const remoteAddress = socket?.remoteAddress;
    if (!remoteAddress || !this.trustedProxies.has(normalizeIp(remoteAddress))) {
      return false;
    }

    const forwardedProto = req.headers["x-forwarded-proto"];
    const rawProto = Array.isArray(forwardedProto)
      ? forwardedProto[0]
      : forwardedProto;

    if (typeof rawProto !== "string") {
      return false;
    }

    const clientProto = rawProto.split(",")[0]?.trim().toLowerCase();
    return clientProto === "https";
  }

  /**
   * Register validation rules for an endpoint.
   */
  registerRules(endpoint: string, rules: ValidationRule[]): void {
    this.rules.set(endpoint, rules);
  }

  /**
   * Validate request headers.
   */
  validateHeaders(req: IncomingMessage): ValidationResult {
    const errors: string[] = [];

    if (this.requireHttps && !this.isHttpsRequest(req)) {
      errors.push("HTTPS required");
    }

    // Validate required security headers
    if (!req.headers["x-client-id"]) errors.push("Missing X-Client-ID header");
    if (!req.headers["x-client-signature"]) errors.push("Missing X-Client-Signature header");
    if (!req.headers["x-request-timestamp"]) errors.push("Missing X-Request-Timestamp header");
    if (!req.headers["x-request-nonce"]) errors.push("Missing X-Request-Nonce header");

    // Validate timestamp is recent (within 5 minutes)
    const timestamp = req.headers["x-request-timestamp"];
    if (typeof timestamp === "string") {
      const ts = parseInt(timestamp, 10);
      const now = Date.now();
      if (isNaN(ts) || Math.abs(now - ts) > 5 * 60 * 1000) {
        errors.push("Invalid or stale request timestamp");
      }
    }

    // Validate nonce format (should be hex string)
    const nonce = req.headers["x-request-nonce"];
    if (typeof nonce === "string" && !/^[a-f0-9]{32,}$/.test(nonce)) {
      errors.push("Invalid nonce format");
    }

    return {
      valid: errors.length === 0,
      errors,
    };
  }

  /**
   * Validate request body against registered rules.
   */
  validateBody(endpoint: string, body: Record<string, unknown>): ValidationResult {
    const rules = this.rules.get(endpoint);
    if (!rules) {
      return { valid: true, errors: [], data: body };
    }

    const errors: string[] = [];

    for (const rule of rules) {
      const value = body[rule.field];

      if (rule.required && (value === undefined || value === null)) {
        errors.push(`Missing required field: ${rule.field}`);
        continue;
      }

      if (value === undefined || value === null) continue;

      // Type validation
      const actualType = Array.isArray(value) ? "array" : typeof value;
      if (actualType !== rule.type) {
        errors.push(`Field ${rule.field} must be ${rule.type}, got ${actualType}`);
        continue;
      }

      // String validations
      if (rule.type === "string" && typeof value === "string") {
        if (rule.maxLength && value.length > rule.maxLength) {
          errors.push(`Field ${rule.field} exceeds max length of ${rule.maxLength}`);
        }
        if (rule.pattern && !rule.pattern.test(value)) {
          errors.push(`Field ${rule.field} does not match required pattern`);
        }
      }

      // Allowed values validation
      if (rule.allowedValues && !rule.allowedValues.includes(value)) {
        errors.push(`Field ${rule.field} has invalid value. Allowed: ${rule.allowedValues.join(", ")}`);
      }
    }

    return {
      valid: errors.length === 0,
      errors,
      data: body,
    };
  }

  /**
   * Sanitize input to prevent injection attacks.
   */
  sanitize(input: string): string {
    return input
      .replace(/[<>]/g, "") // Remove angle brackets
      .replace(/[{}]/g, "") // Remove braces
      .replace(/["']/g, "") // Remove quotes
      .trim();
  }
}
