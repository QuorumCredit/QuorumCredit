import { afterEach, expect, it, vi } from "vitest";
import { LocalAuthRateLimiter } from "../src/auth/rateLimiter.js";

afterEach(() => vi.useRealTimers());

it("caps 100,000 distinct IPs and sweeps expired entries without another request", async () => {
  vi.useFakeTimers();
  const limiter = new LocalAuthRateLimiter({ maxAttempts: 1, maxEntries: 10_000, windowMs: 1000 });
  try {
    for (let n = 0; n < 100_000; n++) {
      await limiter.recordFailure(`10.${n >> 16}.${(n >> 8) & 255}.${n & 255}`);
    }
    expect(limiter["store"].size).toBe(10_000);
    expect(await limiter.isBlocked("10.0.0.0")).toBe(false);
    expect(await limiter.isBlocked("10.1.134.159")).toBe(true);

    vi.advanceTimersByTime(1000);
    expect(limiter["store"].size).toBe(0);
  } finally {
    await limiter.close();
  }
  expect(vi.getTimerCount()).toBe(0);
});

it("keeps a newer window when sweeping an expired one", async () => {
  vi.useFakeTimers();
  const limiter = new LocalAuthRateLimiter({ maxAttempts: 1, windowMs: 1000 });
  try {
    await limiter.recordFailure("10.0.0.1");
    vi.advanceTimersByTime(500);
    await limiter.recordFailure("10.0.0.2");
    vi.advanceTimersByTime(500);
    expect(limiter["store"].size).toBe(1);
    expect(await limiter.isBlocked("10.0.0.2")).toBe(true);
  } finally {
    await limiter.close();
  }
});
