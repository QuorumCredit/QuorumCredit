import { describe, it, expect } from "vitest";
import { WsRateLimiter } from "../src/ws/wsRateLimiter.js";

// ---------------------------------------------------------------------------
// Unit tests for the WsRateLimiter sliding-window module
// ---------------------------------------------------------------------------

describe("WsRateLimiter — sliding window", () => {
  it("allows messages up to the limit", () => {
    const rl = new WsRateLimiter({ windowMs: 1_000, maxMessages: 5, maxViolations: 3 });
    for (let i = 0; i < 5; i++) {
      expect(rl.check(1_000)).toBe("ok");
    }
  });

  it("returns 'throttled' on the first violation", () => {
    const rl = new WsRateLimiter({ windowMs: 1_000, maxMessages: 3, maxViolations: 3 });
    rl.check(0);
    rl.check(0);
    rl.check(0); // fills the window
    expect(rl.check(0)).toBe("throttled"); // 1st violation
  });

  it("returns 'disconnect' after maxViolations consecutive throttle hits", () => {
    const rl = new WsRateLimiter({ windowMs: 1_000, maxMessages: 2, maxViolations: 3 });
    const t = 0;
    rl.check(t); // ok
    rl.check(t); // ok — window full
    expect(rl.check(t)).toBe("throttled"); // violation 1
    expect(rl.check(t)).toBe("throttled"); // violation 2
    expect(rl.check(t)).toBe("disconnect"); // violation 3 → disconnect
  });

  it("resets the violation counter when a message is accepted after the window slides", () => {
    const rl = new WsRateLimiter({ windowMs: 1_000, maxMessages: 2, maxViolations: 3 });
    // Fill window at t=0
    rl.check(0);
    rl.check(0);
    // Two violations (stays below disconnect threshold of 3)
    expect(rl.check(0)).toBe("throttled"); // violation 1
    expect(rl.check(0)).toBe("throttled"); // violation 2
    // Slide window past the original entries — the two t=0 messages are now evicted
    expect(rl.check(1_001)).toBe("ok"); // violations reset
    // Should be below limit again
    expect(rl.check(1_001)).toBe("ok");
  });

  it("does not count stale timestamps when deciding throttle", () => {
    const rl = new WsRateLimiter({ windowMs: 500, maxMessages: 3, maxViolations: 3 });
    // Fill at t=0
    rl.check(0);
    rl.check(0);
    rl.check(0);
    // Advance time past the window; all previous entries evicted → ok
    expect(rl.check(501)).toBe("ok");
  });

  it("reset() clears log and violations", () => {
    const rl = new WsRateLimiter({ windowMs: 1_000, maxMessages: 1, maxViolations: 2 });
    rl.check(0); // ok
    rl.check(0); // throttled (violation 1)
    rl.reset();
    expect(rl.check(0)).toBe("ok"); // fresh start
  });
});

// ---------------------------------------------------------------------------
// Flood simulation: rapid-fire subscribe events
// ---------------------------------------------------------------------------

describe("WsRateLimiter — subscribe flood scenario", () => {
  /**
   * Simulates a malicious client hammering subscribe in a tight loop.
   * With maxMessages=5 and maxViolations=3:
   *   - Messages 1-5: accepted (ok)
   *   - Messages 6-7: throttled
   *   - Message 8: disconnect (3rd violation)
   */
  it("throttles then disconnects a subscribe-flooding client", () => {
    const rl = new WsRateLimiter({ windowMs: 10_000, maxMessages: 5, maxViolations: 3 });
    const t = Date.now();

    const decisions: string[] = [];
    for (let i = 0; i < 10; i++) {
      decisions.push(rl.check(t));
    }

    // First 5 accepted
    expect(decisions.slice(0, 5)).toEqual(["ok", "ok", "ok", "ok", "ok"]);
    // Next 2 throttled
    expect(decisions[5]).toBe("throttled");
    expect(decisions[6]).toBe("throttled");
    // 3rd violation → disconnect
    expect(decisions[7]).toBe("disconnect");
    // All subsequent calls are also disconnect (violations keep accumulating)
    expect(decisions[8]).toBe("disconnect");
    expect(decisions[9]).toBe("disconnect");
  });

  it("a client that backs off and retries after window expiry is NOT disconnected", () => {
    const rl = new WsRateLimiter({ windowMs: 1_000, maxMessages: 3, maxViolations: 3 });
    const t0 = 0;

    // Flood inside one window
    rl.check(t0); // ok
    rl.check(t0); // ok
    rl.check(t0); // ok — full
    expect(rl.check(t0)).toBe("throttled"); // violation 1
    expect(rl.check(t0)).toBe("throttled"); // violation 2

    // Client backs off; window slides — old messages evict, violations reset
    const t1 = 1_001;
    expect(rl.check(t1)).toBe("ok"); // accepted, violations reset
    expect(rl.check(t1)).toBe("ok");
    expect(rl.check(t1)).toBe("ok"); // full again
    // One more violation, but still below disconnect threshold
    expect(rl.check(t1)).toBe("throttled");
  });

  it("separate limiter instances are fully independent", () => {
    const rl1 = new WsRateLimiter({ windowMs: 1_000, maxMessages: 2, maxViolations: 2 });
    const rl2 = new WsRateLimiter({ windowMs: 1_000, maxMessages: 2, maxViolations: 2 });

    const t = 0;
    rl1.check(t);
    rl1.check(t);
    // rl1 is full; rl2 is still pristine
    expect(rl1.check(t)).toBe("throttled");
    expect(rl2.check(t)).toBe("ok");
  });
});
