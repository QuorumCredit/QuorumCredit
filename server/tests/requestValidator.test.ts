import { describe, it, expect, afterEach } from "vitest";
import type { IncomingMessage } from "node:http";
import { RequestValidator } from "../src/auth/requestValidator.js";

function validSecurityHeaders(
  extra: Record<string, string | string[]> = {},
): Record<string, string | string[]> {
  return {
    "x-client-id": "client-1",
    "x-client-signature": "sig-abc",
    "x-request-timestamp": String(Date.now()),
    "x-request-nonce": "0123456789abcdef0123456789abcdef",
    ...extra,
  };
}

function makeRequest(opts: {
  encrypted?: boolean;
  remoteAddress?: string;
  headers?: Record<string, string | string[]>;
}): IncomingMessage {
  return {
    socket: {
      encrypted: opts.encrypted ?? false,
      remoteAddress: opts.remoteAddress,
    },
    headers: opts.headers ?? validSecurityHeaders(),
  } as unknown as IncomingMessage;
}

describe("RequestValidator HTTPS enforcement (#2154)", () => {
  const originalEnv = { ...process.env };

  afterEach(() => {
    process.env = { ...originalEnv };
  });

  describe("direct TLS", () => {
    it("accepts a direct TLS request by default", () => {
      const validator = new RequestValidator();
      const req = makeRequest({ encrypted: true });
      const result = validator.validateHeaders(req);

      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });
  });

  describe("proxied TLS", () => {
    it("accepts X-Forwarded-Proto: https from a configured trusted proxy", () => {
      const validator = new RequestValidator({
        trustedProxies: ["10.0.0.5"],
      });
      const req = makeRequest({
        encrypted: false,
        remoteAddress: "10.0.0.5",
        headers: validSecurityHeaders({ "x-forwarded-proto": "https" }),
      });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    it("matches IPv4-mapped IPv6 remoteAddress against IPv4 trusted proxy", () => {
      const validator = new RequestValidator({
        trustedProxies: ["10.0.0.5"],
      });
      const req = makeRequest({
        encrypted: false,
        remoteAddress: "::ffff:10.0.0.5",
        headers: validSecurityHeaders({ "x-forwarded-proto": "https" }),
      });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    it("accepts multi-hop X-Forwarded-Proto starting with https from a trusted proxy", () => {
      const validator = new RequestValidator({
        trustedProxies: ["10.0.0.5"],
      });
      const req = makeRequest({
        encrypted: false,
        remoteAddress: "10.0.0.5",
        headers: validSecurityHeaders({ "x-forwarded-proto": "https, http" }),
      });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    it("reads trusted proxies from TRUSTED_PROXIES env var when not passed in options", () => {
      process.env.TRUSTED_PROXIES = "127.0.0.1, 10.0.0.8";
      const validator = new RequestValidator();
      const req = makeRequest({
        encrypted: false,
        remoteAddress: "10.0.0.8",
        headers: validSecurityHeaders({ "x-forwarded-proto": "https" }),
      });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    it("rejects X-Forwarded-Proto: https from an untrusted proxy IP", () => {
      const validator = new RequestValidator({
        trustedProxies: ["10.0.0.5"],
      });
      const req = makeRequest({
        encrypted: false,
        remoteAddress: "192.168.1.99",
        headers: validSecurityHeaders({ "x-forwarded-proto": "https" }),
      });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("HTTPS required");
    });

    it("rejects X-Forwarded-Proto: https when no trusted proxies are configured", () => {
      const validator = new RequestValidator();
      const req = makeRequest({
        encrypted: false,
        remoteAddress: "10.0.0.5",
        headers: validSecurityHeaders({ "x-forwarded-proto": "https" }),
      });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("HTTPS required");
    });

    it("rejects X-Forwarded-Proto: http even from a trusted proxy", () => {
      const validator = new RequestValidator({
        trustedProxies: ["10.0.0.5"],
      });
      const req = makeRequest({
        encrypted: false,
        remoteAddress: "10.0.0.5",
        headers: validSecurityHeaders({ "x-forwarded-proto": "http" }),
      });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("HTTPS required");
    });
  });

  describe("plain HTTP", () => {
    it("rejects plain HTTP by default even outside production", () => {
      process.env.NODE_ENV = "development";
      delete process.env.ALLOW_INSECURE_HTTP;

      const validator = new RequestValidator();
      const req = makeRequest({ encrypted: false, remoteAddress: "127.0.0.1" });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("HTTPS required");
    });

    it("rejects plain HTTP when NODE_ENV is unset", () => {
      delete process.env.NODE_ENV;
      delete process.env.ALLOW_INSECURE_HTTP;

      const validator = new RequestValidator();
      const req = makeRequest({ encrypted: false });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(false);
      expect(result.errors).toContain("HTTPS required");
    });

    it("allows plain HTTP only when requireHttps is explicitly set to false", () => {
      const validator = new RequestValidator({ requireHttps: false });
      const req = makeRequest({ encrypted: false, remoteAddress: "127.0.0.1" });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    it("allows plain HTTP when ALLOW_INSECURE_HTTP=true is explicitly set", () => {
      process.env.ALLOW_INSECURE_HTTP = "true";

      const validator = new RequestValidator();
      const req = makeRequest({ encrypted: false, remoteAddress: "127.0.0.1" });

      const result = validator.validateHeaders(req);
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });
  });
});
