/**
 * Issue #1290 / #1374: Per-IP failed-login throttling for `/api/auth/token`.
 *
 * Follows the same Local-vs-Redis split used throughout the server (see
 * `jtiRevocationStore.ts`'s `buildRevocationStore`, `pubsub/RedisBus.ts`):
 * - Single-instance / local dev / test → `LocalAuthRateLimiter` (in-process Map).
 * - Multi-instance / production        → `RedisAuthRateLimiter` (Redis INCR+PEXPIRE).
 *
 * Issue #1290 shipped only the in-process store, with its header explicitly scoping
 * multi-instance support out ("that wiring is outside the scope of this fix"). That
 * left a real gap: behind a load-balanced multi-replica deployment, each backend
 * instance tracks failed attempts in its own Map, so a credential-stuffing attacker
 * gets roughly N × maxAttempts free attempts across N replicas instead of maxAttempts
 * total, since a round-robin/random load balancer spreads requests across instances
 * that never share state (issue #1374). `RedisAuthRateLimiter` closes that gap by
 * keying the counter in Redis, so every instance enforces the same shared count.
 *
 * Usage:
 *   const limiter = buildAuthRateLimiter(config.redisUrl);
 *   const blocked = await limiter.isBlocked(sourceIp);
 *   const nowBlocked = await limiter.recordFailure(sourceIp);
 */

import type { Redis } from "ioredis";

export interface RateLimitEntry {
  count: number;
  windowStart: number; // ms
}

export interface AuthRateLimiter {
  /** Record a failed attempt; returns true when the IP should now be blocked. */
  recordFailure(ip: string): Promise<boolean>;
  /** Returns true when the IP is currently blocked. */
  isBlocked(ip: string): Promise<boolean>;
  /** Remove all entries (useful in tests). */
  clear(): Promise<void>;
  /** Close / clean up underlying connections. */
  close(): Promise<void>;
}

export interface RateLimiterOptions {
  /** Max failed attempts before the IP is blocked. Default 5. */
  maxAttempts?: number;
  /** Window duration in milliseconds. Default 60_000 (1 minute). */
  windowMs?: number;
}

// ── Local (single-instance) implementation ─────────────────────────────────────

/**
 * In-process rate limiter backed by a plain Map.
 * NOT multi-instance-safe — for local dev and unit tests only.
 */
export class LocalAuthRateLimiter implements AuthRateLimiter {
  private readonly maxAttempts: number;
  private readonly windowMs: number;
  private readonly store = new Map<string, RateLimitEntry>();

  constructor(opts: RateLimiterOptions = {}) {
    this.maxAttempts = opts.maxAttempts ?? 5;
    this.windowMs = opts.windowMs ?? 60_000;
  }

  private getEntry(ip: string, now: number): RateLimitEntry {
    const existing = this.store.get(ip);
    if (!existing || now - existing.windowStart >= this.windowMs) {
      const fresh: RateLimitEntry = { count: 0, windowStart: now };
      this.store.set(ip, fresh);
      return fresh;
    }
    return existing;
  }

  async recordFailure(ip: string): Promise<boolean> {
    const now = Date.now();
    const entry = this.getEntry(ip, now);
    entry.count += 1;
    this.store.set(ip, entry);
    return entry.count >= this.maxAttempts;
  }

  async isBlocked(ip: string): Promise<boolean> {
    const now = Date.now();
    const existing = this.store.get(ip);
    if (!existing) return false;
    if (now - existing.windowStart >= this.windowMs) {
      this.store.delete(ip);
      return false;
    }
    return existing.count >= this.maxAttempts;
  }

  async clear(): Promise<void> {
    this.store.clear();
  }

  async close(): Promise<void> {
    this.store.clear();
  }
}

/** Preserved factory name/signature from #1290 — existing callers keep working. */
export function createAuthRateLimiter(opts: RateLimiterOptions = {}): AuthRateLimiter {
  return new LocalAuthRateLimiter(opts);
}

// ── Redis (multi-instance) implementation ──────────────────────────────────────

const REDIS_RATE_LIMIT_PREFIX = "qc:ratelimit:auth:";

/**
 * Redis-backed rate limiter for multi-instance deployments (issue #1374).
 *
 * Each IP's failure count is stored as `qc:ratelimit:auth:{ip}` → integer count,
 * incremented with INCR. The key's TTL is set (via PEXPIRE) only on the increment
 * that creates it (count transitions 0 → 1), so the TTL always reflects time
 * remaining in the *original* window — mirroring `LocalAuthRateLimiter`'s
 * `windowStart`-based reset exactly: once the key naturally expires, Redis removes
 * it and the next failure starts a fresh window, just like the local Map entry
 * being replaced once `now - windowStart >= windowMs`.
 */
export class RedisAuthRateLimiter implements AuthRateLimiter {
  private readonly maxAttempts: number;
  private readonly windowMs: number;

  constructor(private readonly redis: Redis, opts: RateLimiterOptions = {}) {
    this.maxAttempts = opts.maxAttempts ?? 5;
    this.windowMs = opts.windowMs ?? 60_000;
  }

  private key(ip: string): string {
    return `${REDIS_RATE_LIMIT_PREFIX}${ip}`;
  }

  async recordFailure(ip: string): Promise<boolean> {
    const key = this.key(ip);
    const count = await this.redis.incr(key);
    if (count === 1) {
      // First failure in a fresh window — start the TTL now, matching
      // LocalAuthRateLimiter recording `windowStart = now` on entry creation.
      await this.redis.pexpire(key, this.windowMs);
    }
    return count >= this.maxAttempts;
  }

  async isBlocked(ip: string): Promise<boolean> {
    const raw = await this.redis.get(this.key(ip));
    if (raw === null) return false;
    return Number.parseInt(raw, 10) >= this.maxAttempts;
  }

  async clear(): Promise<void> {
    const keys = await this.redis.keys(`${REDIS_RATE_LIMIT_PREFIX}*`);
    if (keys.length > 0) await this.redis.del(...keys);
  }

  async close(): Promise<void> {
    await this.redis.quit();
  }
}

// ── Factory ────────────────────────────────────────────────────────────────────

/**
 * Build the appropriate rate limiter based on whether Redis is configured.
 * Mirrors `buildBus`/`buildRevocationStore`: Redis when a URL is configured
 * (required for any multi-replica deployment), otherwise an in-process store
 * with a loud startup warning — genuinely fine only for single-instance
 * dev/test, never for more than one replica in production.
 */
export function buildAuthRateLimiter(
  redisUrl: string | undefined,
  opts: RateLimiterOptions = {}
): AuthRateLimiter {
  if (redisUrl) {
    // Lazily import ioredis so it is not required in local/test environments.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { Redis } = require("ioredis") as typeof import("ioredis");
    const redis = new Redis(redisUrl, { lazyConnect: false });
    return new RedisAuthRateLimiter(redis, opts);
  }
  console.warn(
    "[quorum-credit-broadcast-server] REDIS_URL not set — using an in-process auth " +
      "rate limiter. This is NOT multi-instance-safe and must not be used with more " +
      "than one replica in production."
  );
  return new LocalAuthRateLimiter(opts);
}

/** Default singleton rate limiter (5 failures per minute per IP), local-only. */
export const defaultAuthRateLimiter = createAuthRateLimiter();
