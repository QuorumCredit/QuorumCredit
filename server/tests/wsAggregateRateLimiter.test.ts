import { describe, it, expect, beforeEach } from "vitest";
import {
  LocalWsAggregateRateLimiter,
  RedisWsAggregateRateLimiter,
  buildWsAggregateRateLimiter,
} from "../../src/ws/wsAggregateRateLimiter.js";

describe("LocalWsAggregateRateLimiter", () => {
  let limiter: LocalWsAggregateRateLimiter;

  beforeEach(() => {
    limiter = new LocalWsAggregateRateLimiter({ maxMessages: 3, windowMs: 60_000 });
  });

  it("allows hits under the limit", async () => {
    for (let i = 0; i < 3; i++) {
      const blocked = await limiter.recordHit("key");
      expect(blocked).toBe(false);
    }
  });

  it("blocks after exceeding the limit", async () => {
    for (let i = 0; i < 3; i++) {
      await limiter.recordHit("key");
    }
    const blocked = await limiter.recordHit("key");
    expect(blocked).toBe(true);
  });

  it("resets after the window expires", async () => {
    const now = Date.now();
    limiter = new LocalWsAggregateRateLimiter({ maxMessages: 2, windowMs: 1000 });

    await limiter.recordHit("key");
    await limiter.recordHit("key");

    // Manually advance time by setting windowStart in the past.
    const entry = (limiter as any).store.get("key") as { count: number; windowStart: number };
    entry.windowStart = now - 2000;
    (limiter as any).store.set("key", entry);

    const blocked = await limiter.recordHit("key");
    expect(blocked).toBe(false);
  });

  it("tracks keys independently", async () => {
    await limiter.recordHit("a");
    await limiter.recordHit("a");
    await limiter.recordHit("a");
    expect(await limiter.isBlocked("a")).toBe(true);
    expect(await limiter.isBlocked("b")).toBe(false);
  });

  it("clear removes all entries", async () => {
    await limiter.recordHit("key");
    await limiter.clear();
    expect(await limiter.isBlocked("key")).toBe(false);
  });
});

describe("buildWsAggregateRateLimiter", () => {
  it("returns a local limiter when redisUrl is undefined", () => {
    const limiter = buildWsAggregateRateLimiter(undefined);
    expect(limiter).toBeInstanceOf(LocalWsAggregateRateLimiter);
  });

  it("returns a redis limiter when redisUrl is set", () => {
    const limiter = buildWsAggregateRateLimiter("redis://localhost:6379");
    expect(limiter).toBeInstanceOf(RedisWsAggregateRateLimiter);
  });
});
