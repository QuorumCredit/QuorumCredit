import type { IncomingMessage } from "node:http";

/**
 * Normalizes an IP address by stripping brackets and IPv4-mapping prefixes.
 */
export function normalizeIp(ip: string): string {
  let cleaned = ip.trim().toLowerCase();
  if (cleaned.startsWith("[") && cleaned.endsWith("]")) {
    cleaned = cleaned.slice(1, -1);
  }
  if (cleaned.startsWith("::ffff:")) {
    cleaned = cleaned.slice(7);
  }
  return cleaned;
}

/**
 * Checks if a given IP matches any configured trusted proxy address.
 */
export function isIpTrusted(ip: string, trustedSet: Set<string>): boolean {
  if (trustedSet.has(ip)) return true;
  const normalized = normalizeIp(ip);
  return trustedSet.has(normalized);
}

/**
 * Resolves the client IP address from an HTTP request.
 *
 * Requirements (Issue #2155):
 * - Use the socket address unless the request arrives from a configured trusted proxy.
 * - When arriving from a configured trusted proxy, take the client address from
 *   the rightmost untrusted hop in X-Forwarded-For.
 */
export function getClientIp(
  req: IncomingMessage,
  trustedProxies?: string[] | Set<string>
): string {
  const socketIp = req.socket?.remoteAddress;
  if (!socketIp) return "unknown";

  // Build trusted set from parameter or TRUSTED_PROXIES env var
  let trustedSet: Set<string>;
  if (trustedProxies instanceof Set) {
    trustedSet = trustedProxies;
  } else if (Array.isArray(trustedProxies)) {
    trustedSet = new Set(trustedProxies.map((p) => normalizeIp(p)));
  } else {
    const envProxies = process.env.TRUSTED_PROXIES;
    trustedSet = envProxies
      ? new Set(envProxies.split(",").map((p) => normalizeIp(p.trim())).filter(Boolean))
      : new Set();
  }

  // If no trusted proxies configured, or socket IP is not a trusted proxy,
  // ignore X-Forwarded-For and use socket address directly.
  if (trustedSet.size === 0 || !isIpTrusted(socketIp, trustedSet)) {
    return socketIp;
  }

  // Request arrived from a trusted proxy: parse X-Forwarded-For
  const forwarded = req.headers["x-forwarded-for"];
  if (typeof forwarded !== "string" || !forwarded.trim()) {
    return socketIp;
  }

  const hops = forwarded.split(",").map((h) => h.trim()).filter(Boolean);
  if (hops.length === 0) {
    return socketIp;
  }

  // Walk backwards from right to left to find the rightmost untrusted hop
  for (let i = hops.length - 1; i >= 0; i--) {
    const hop = hops[i];
    if (!isIpTrusted(hop, trustedSet)) {
      return hop;
    }
  }

  // All hops in X-Forwarded-For were trusted; return the leftmost hop
  return hops[0];
}
