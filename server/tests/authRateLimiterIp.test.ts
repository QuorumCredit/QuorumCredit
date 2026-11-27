import { describe, it, expect } from "vitest";
import type { IncomingMessage } from "node:http";
import { getClientIp, normalizeIp } from "../src/http/clientIp.js";
import { LocalAuthRateLimiter } from "../src/auth/rateLimiter.js";

function mockRequest(
  socketIp: string,
  headers: Record<string, string | undefined> = {}
): IncomingMessage {
  return {
    socket: { remoteAddress: socketIp },
    headers,
  } as unknown as IncomingMessage;
}

describe("Issue #2155: Client IP for auth rate limiting and rotating X-Forwarded-For protection", () => {
  it("normalizes IPv4-mapped and bracketed IPs", () => {
    expect(normalizeIp("::ffff:127.0.0.1")).toBe("127.0.0.1");
    expect(normalizeIp("[::1]")).toBe("::1");
    expect(normalizeIp("  10.0.0.1  ")).toBe("10.0.0.1");
  });

  it("uses the socket address when no trusted proxy is configured, ignoring X-Forwarded-For", () => {
    const req = mockRequest("198.51.100.10", {
      "x-forwarded-for": "203.0.113.1, 203.0.113.2",
    });

    const resolvedIp = getClientIp(req);
    expect(resolvedIp).toBe("198.51.100.10");
  });

  it("rotating X-Forwarded-For does not reset the rate limit for an untrusted socket", async () => {
    const limiter = new LocalAuthRateLimiter({ maxAttempts: 3, windowMs: 60_000 });
    const attackerSocket = "198.51.100.99";

    // Attacker rotates X-Forwarded-For on each attempt
    const spoofedIps = ["1.1.1.1", "2.2.2.2", "3.3.3.3", "4.4.4.4"];

    for (let i = 0; i < 3; i++) {
      const req = mockRequest(attackerSocket, {
        "x-forwarded-for": spoofedIps[i],
      });
      const resolvedIp = getClientIp(req);
      expect(resolvedIp).toBe(attackerSocket);

      const isBlockedBefore = await limiter.isBlocked(resolvedIp);
      expect(isBlockedBefore).toBe(false);

      await limiter.recordFailure(resolvedIp);
    }

    // Fourth attempt with yet another spoofed IP must be blocked because the limiter keys on socket address
    const req4 = mockRequest(attackerSocket, {
      "x-forwarded-for": spoofedIps[3],
    });
    const resolvedIp4 = getClientIp(req4);
    expect(resolvedIp4).toBe(attackerSocket);

    const isBlockedAfter = await limiter.isBlocked(resolvedIp4);
    expect(isBlockedAfter).toBe(true);
  });

  it("resolves the rightmost untrusted hop when request arrives from a configured trusted proxy", () => {
    const trustedProxies = ["10.0.0.1", "10.0.0.2"];
    const proxySocket = "10.0.0.1";

    // Single untrusted client hop
    const req1 = mockRequest(proxySocket, {
      "x-forwarded-for": "203.0.113.50",
    });
    expect(getClientIp(req1, trustedProxies)).toBe("203.0.113.50");

    // Client -> untrusted proxy -> trusted proxy -> server
    const req2 = mockRequest(proxySocket, {
      "x-forwarded-for": "198.51.100.20, 203.0.113.88, 10.0.0.2",
    });
    // 10.0.0.2 is trusted, so rightmost untrusted hop is 203.0.113.88
    expect(getClientIp(req2, trustedProxies)).toBe("203.0.113.88");
  });

  it("returns socket address if request socket is not in the trusted proxies list", () => {
    const trustedProxies = ["10.0.0.1"];
    const untrustedSocket = "192.168.1.50";

    const req = mockRequest(untrustedSocket, {
      "x-forwarded-for": "203.0.113.50",
    });
    expect(getClientIp(req, trustedProxies)).toBe(untrustedSocket);
  });
});
