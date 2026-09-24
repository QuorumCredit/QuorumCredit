/**
 * Issue #1374: proves RedisAuthRateLimiter closes the multi-instance bypass left
 * open by #1290's in-process-only rate limiter (see rateLimiter.ts's header).
 *
 * `FakeRedisClient` below implements only the handful of ioredis commands
 * RedisAuthRateLimiter actually issues (incr/pexpire/get/keys/del/quit), backed by
 * a plain in-memory Map. It stands in for a real Redis server the same way
 * `test/multi-instance/multiInstance.test.ts` uses a relay process instead of a
 * real `redis-server` binary (not installable in this sandbox) — see that file's
 * docstring. Unlike the pub/sub case, proving the rate-limit fix doesn't require
 * real network separation: the bug being fixed is "two instances, two independent
 * stores", and the fix is "two instances, one shared store" — a single shared
 * `FakeRedisClient` instance handed to two separate `RedisAuthRateLimiter` objects
 * exercises exactly that mechanism, deterministically and without spawning
 * processes, so this lives in the fast `npm test` suite rather than
 * `test:multi-instance`.
 */

import { describe, it, expect } from "vitest";
import type { Redis } from "ioredis";
import { RedisAuthRateLimiter, createAuthRateLimiter } from "../src/auth/rateLimiter.js";

class FakeRedisClient {
  private readonly store = new Map<string, { count: number; expiresAt: number | null }>();

  private prune(key: string): void {
    const entry = this.store.get(key);
    if (entry && entry.expiresAt !== null && entry.expiresAt <= Date.now()) {
      this.store.delete(key);
    }
  }

  async incr(key: string): Promise<number> {
    this.prune(key);
    const existing = this.store.get(key);
    if (!existing) {
      this.store.set(key, { count: 1, expiresAt: null });
      return 1;
    }
    existing.count += 1;
    return existing.count;
  }

  async pexpire(key: string, ms: number): Promise<number> {
    const entry = this.store.get(key);
    if (!entry) return 0;
    entry.expiresAt = Date.now() + ms;
    return 1;
  }

  async get(key: string): Promise<string | null> {
    this.prune(key);
    const entry = this.store.get(key);
    return entry ? String(entry.count) : null;
  }

  async keys(pattern: string): Promise<string[]> {
    const prefix = pattern.replace(/\*$/, "");
    return [...this.store.keys()].filter((k) => k.startsWith(prefix));
  }

  async del(...keys: string[]): Promise<number> {
    let removed = 0;
    for (const key of keys) if (this.store.delete(key)) removed += 1;
    return removed;
  }

  async quit(): Promise<"OK"> {
    return "OK";
  }
}

function fakeRedis(): Redis {
  return new FakeRedisClient() as unknown as Redis;
}

describe("RedisAuthRateLimiter", () => {
  it("does not block on first failure", async () => {
    const limiter = new RedisAuthRateLimiter(fakeRedis(), { maxAttempts: 3, windowMs: 5_000 });
    expect(await limiter.recordFailure("1.2.3.4")).toBe(false);
    expect(await limiter.isBlocked("1.2.3.4")).toBe(false);
  });

  it("blocks IP after maxAttempts failures", async () => {
    const limiter = new RedisAuthRateLimiter(fakeRedis(), { maxAttempts: 3, windowMs: 5_000 });
    await limiter.recordFailure("1.2.3.4");
    await limiter.recordFailure("1.2.3.4");
    const blocked = await limiter.recordFailure("1.2.3.4"); // 3rd = threshold
    expect(blocked).toBe(true);
    expect(await limiter.isBlocked("1.2.3.4")).toBe(true);
  });

  it("does not block a different IP", async () => {
    const limiter = new RedisAuthRateLimiter(fakeRedis(), { maxAttempts: 3, windowMs: 5_000 });
    await limiter.recordFailure("1.2.3.4");
    await limiter.recordFailure("1.2.3.4");
    await limiter.recordFailure("1.2.3.4");
    expect(await limiter.isBlocked("9.9.9.9")).toBe(false);
  });

  it("clears all entries", async () => {
    const limiter = new RedisAuthRateLimiter(fakeRedis(), { maxAttempts: 3, windowMs: 5_000 });
    await limiter.recordFailure("1.2.3.4");
    await limiter.recordFailure("1.2.3.4");
    await limiter.recordFailure("1.2.3.4");
    await limiter.clear();
    expect(await limiter.isBlocked("1.2.3.4")).toBe(false);
  });

  it("resets count after window expires", async () => {
    const limiter = new RedisAuthRateLimiter(fakeRedis(), { maxAttempts: 2, windowMs: 10 });
    await limiter.recordFailure("5.5.5.5");
    await limiter.recordFailure("5.5.5.5"); // blocked
    expect(await limiter.isBlocked("5.5.5.5")).toBe(true);
    await new Promise((r) => setTimeout(r, 20));
    expect(await limiter.isBlocked("5.5.5.5")).toBe(false);
  });

  describe("multi-instance enforcement (issue #1374)", () => {
    it("makes failures recorded on one instance visible to and enforced by a second instance sharing the same Redis backend", async () => {
      const redis = fakeRedis();
      const opts = { maxAttempts: 3, windowMs: 5_000 };
      const instanceA = new RedisAuthRateLimiter(redis, opts);
      const instanceB = new RedisAuthRateLimiter(redis, opts);

      expect(await instanceA.recordFailure("203.0.113.5")).toBe(false);
      expect(await instanceB.recordFailure("203.0.113.5")).toBe(false);
      // 3rd failure recorded on instance A trips the *shared* threshold.
      expect(await instanceA.recordFailure("203.0.113.5")).toBe(true);
      // Instance B directly recorded only one of the three failures, but must
      // still see the IP as blocked because the count lives in the shared store.
      expect(await instanceB.isBlocked("203.0.113.5")).toBe(true);
    });

    it("blocks the maxAttempts-th request when an attacker distributes exactly maxAttempts requests round-robin across two instances, unlike two independent in-process limiters", async () => {
      const maxAttempts = 5;
      const ip = "198.51.100.9";

      // First, demonstrate the bug this fix closes: two independent in-process
      // limiters (the pre-#1374 topology — one Map per backend instance) let the
      // attacker's maxAttempts-request burst through untouched, because each
      // instance only ever observes its own half of the traffic.
      const bypassInstances = [
        createAuthRateLimiter({ maxAttempts, windowMs: 5_000 }),
        createAuthRateLimiter({ maxAttempts, windowMs: 5_000 }),
      ];
      let bypassedBlock = false;
      for (let i = 0; i < maxAttempts; i++) {
        const blocked = await bypassInstances[i % 2].recordFailure(ip);
        if (blocked) bypassedBlock = true;
      }
      expect(bypassedBlock).toBe(false); // confirms the pre-fix bypass is real

      // Now the identical request distribution against RedisAuthRateLimiter
      // instances that share one Redis backend.
      const redis = fakeRedis();
      const opts = { maxAttempts, windowMs: 5_000 };
      const sharedInstances = [new RedisAuthRateLimiter(redis, opts), new RedisAuthRateLimiter(redis, opts)];
      let blockedAtIndex = -1;
      for (let i = 0; i < maxAttempts; i++) {
        const blocked = await sharedInstances[i % 2].recordFailure(ip);
        if (blocked) {
          blockedAtIndex = i;
          break;
        }
      }
      // Blocked on the maxAttempts-th request (0-indexed: maxAttempts - 1) against
      // the shared limiter — not allowed through as it would be without sharing.
      expect(blockedAtIndex).toBe(maxAttempts - 1);
    });
  });
});
