/**
 * Issue #1492: Aggregate per-IP/subject rate limiter for inbound WebSocket messages.
 *
 * Follows the same Local-vs-Redis split used throughout the server (see
 * `auth/rateLimiter.ts`):
 * - Single-instance / local dev / test → `LocalWsAggregateRateLimiter` (in-process Map).
 * - Multi-instance / production        → `RedisWsAggregateRateLimiter` (Redis INCR+PEXPIRE).
 *
 * Each backend replica already has a per-connection `WsRateLimiter`. That keeps
 * any *single* socket well-behaved, but a client that opens N connections spread
 * across M replicas can multiply its effective message budget by N×M. This limiter
 * closes that gap by keying the counter in Redis (or shared local store) on a
 * caller-supplied identifier such as source IP or authenticated subject.
 *
 * Usage:
 *   const limiter = buildWsAggregateRateLimiter(config.redisUrl);
 *   const blocked = await limiter.isBlocked(key);
 *   const nowBlocked = await limiter.recordHit(key);
 */

import type { Redis } from "ioredis";

export interface WsAggregateRateLimitEntry {
  count: number;
  windowStart: number; // ms
}

export interface WsAggregateRateLimiter {
  /** Record an accepted inbound message; returns true when the keyer should now be blocked. */
  recordHit(key: string): Promise<boolean>;
  /** Returns true when the keyer is currently blocked. */
  isBlocked(key: string): Promise<boolean>;
  /** Remove all entries (useful in tests). */
  clear(): Promise<void>;
  /** Close / clean up underlying connections. */
  close(): Promise<void>;
}

export interface WsAggregateRateLimiterOptions {
  /** Max accepted messages per window before blocking. Default 300. */
  maxMessages?: number;
  /** Window duration in milliseconds. Default 60_000 (1 minute). */
  windowMs?: number;
}

// ── Local (single-instance) implementation ─────────────────────────────────────

/**
 * In-process aggregate rate limiter backed by a plain Map.
 * NOT multi-instance-safe — for local dev and unit tests only.
 */
export class LocalWsAggregateRateLimiter implements WsAggregateRateLimiter {
  private readonly maxMessages: number;
  private readonly windowMs: number;
  private readonly store = new Map<string, WsAggregateRateLimitEntry>();

  constructor(opts: WsAggregateRateLimiterOptions = {}) {
    this.maxMessages = opts.maxMessages ?? 300;
    this.windowMs = opts.windowMs ?? 60_000;
  }

  private getEntry(key: string, now: number): WsAggregateRateLimitEntry {
    const existing = this.store.get(key);
    if (!existing || now - existing.windowStart >= this.windowMs) {
      const fresh: WsAggregateRateLimitEntry = { count: 0, windowStart: now };
      this.store.set(key, fresh);
      return fresh;
    }
    return existing;
  }

  async recordHit(key: string): Promise<boolean> {
    const now = Date.now();
    const entry = this.getEntry(key, now);
    entry.count += 1;
    this.store.set(key, entry);
    return entry.count >= this.maxMessages;
  }

  async isBlocked(key: string): Promise<boolean> {
    const now = Date.now();
    const existing = this.store.get(key);
    if (!existing) return false;
    if (now - existing.windowStart >= this.windowMs) {
      this.store.delete(key);
      return false;
    }
    return existing.count >= this.maxMessages;
  }

  async clear(): Promise<void> {
    this.store.clear();
  }

  async close(): Promise<void> {
    this.store.clear();
  }
}

// ── Redis (multi-instance) implementation ──────────────────────────────────────

const REDIS_WS_RATE_LIMIT_PREFIX = "qc:ratelimit:ws:";

/**
 * Redis-backed aggregate rate limiter for multi-instance deployments.
 *
 * Each keyer's message count is stored as `qc:ratelimit:ws:{key}` → integer count,
 * incremented with INCR. The key's TTL is set (via PEXPIRE) only on the increment
 * that creates it (count transitions 0 → 1), so the TTL always reflects time
 * remaining in the *original* window.
 */
export class RedisWsAggregateRateLimiter implements WsAggregateRateLimiter {
  private readonly maxMessages: number;
  private readonly windowMs: number;

  constructor(private readonly redis: Redis, opts: WsAggregateRateLimiterOptions = {}) {
    this.maxMessages = opts.maxMessages ?? 300;
    this.windowMs = opts.windowMs ?? 60_000;
  }

  private key(key: string): string {
    return `${REDIS_WS_RATE_LIMIT_PREFIX}${key}`;
  }

  async recordHit(key: string): Promise<boolean> {
    const k = this.key(key);
    const count = await this.redis.incr(k);
    if (count === 1) {
      await this.redis.pexpire(k, this.windowMs);
    }
    return count >= this.maxMessages;
  }

  async isBlocked(key: string): Promise<boolean> {
    const raw = await this.redis.get(this.key(key));
    if (raw === null) return false;
    return Number.parseInt(raw, 10) >= this.maxMessages;
  }

  async clear(): Promise<void> {
    const keys = await this.redis.keys(`${REDIS_WS_RATE_LIMIT_PREFIX}*`);
    if (keys.length > 0) await this.redis.del(...keys);
  }

  async close(): Promise<void> {
    await this.redis.quit();
  }
}

// ── Factory ────────────────────────────────────────────────────────────────────

/**
 * Build the appropriate aggregate rate limiter based on whether Redis is configured.
 *
 * Tradeoffs:
 * - Local: zero external dependencies, fast, but only safe for single-replica
 *   deployments. Behind a load balancer each instance tracks its own counter, so
 *   a client that spreads connections across replicas multiplies its budget.
 * - Redis: every instance shares the same counter, preserving the configured limit
 *   regardless of replica count. Adds a Redis round-trip per inbound message; in
 *   practice this is negligible compared to the message processing cost.
 */
export function buildWsAggregateRateLimiter(
  redisUrl: string | undefined,
  opts: WsAggregateRateLimiterOptions = {}
): WsAggregateRateLimiter {
  if (redisUrl) {
    // Lazily import ioredis so it is not required in local/test environments.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { Redis } = require("ioredis") as typeof import("ioredis");
    const redis = new Redis(redisUrl, { lazyConnect: false });
    return new RedisWsAggregateRateLimiter(redis, opts);
  }
  console.warn(
    "[quorum-credit-broadcast-server] REDIS_URL not set — using an in-process WS aggregate " +
      "rate limiter. This is NOT multi-instance-safe and must not be used with more " +
      "than one replica in production."
  );
  return new LocalWsAggregateRateLimiter(opts);
}

/** Default singleton aggregate rate limiter (300 messages/minute per key), local-only. */
export const defaultWsAggregateRateLimiter = buildWsAggregateRateLimiter(undefined);
