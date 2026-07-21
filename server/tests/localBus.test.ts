import { describe, it, expect, vi } from "vitest";
import { LocalBus } from "../src/pubsub/LocalBus.js";

describe("LocalBus", () => {
  it("delivers a published message to a subscribed handler", async () => {
    const bus = new LocalBus();
    const handler = vi.fn();
    await bus.subscribe("ch", handler);
    await bus.publish("ch", "hello");
    expect(handler).toHaveBeenCalledWith("hello");
  });

  it("does not deliver to handlers on other channels", async () => {
    const bus = new LocalBus();
    const handler = vi.fn();
    await bus.subscribe("ch-a", handler);
    await bus.publish("ch-b", "hello");
    expect(handler).not.toHaveBeenCalled();
  });

  it("stops delivering after unsubscribe", async () => {
    const bus = new LocalBus();
    const handler = vi.fn();
    await bus.subscribe("ch", handler);
    await bus.unsubscribe("ch", handler);
    await bus.publish("ch", "hello");
    expect(handler).not.toHaveBeenCalled();
  });

  it("grants a lock to only one holder at a time", async () => {
    const bus = new LocalBus();
    expect(await bus.tryAcquireLock("leader", 5000, "a")).toBe(true);
    expect(await bus.tryAcquireLock("leader", 5000, "b")).toBe(false);
    await bus.releaseLock("leader", "a");
    expect(await bus.tryAcquireLock("leader", 5000, "b")).toBe(true);
  });

  it("allows the current holder to renew its own lock", async () => {
    const bus = new LocalBus();
    expect(await bus.tryAcquireLock("leader", 5000, "a")).toBe(true);
    expect(await bus.tryAcquireLock("leader", 5000, "a")).toBe(true);
  });

  it("allows another holder to acquire once the lock expires", async () => {
    vi.useFakeTimers();
    try {
      const bus = new LocalBus();
      expect(await bus.tryAcquireLock("leader", 100, "a")).toBe(true);
      vi.advanceTimersByTime(200);
      expect(await bus.tryAcquireLock("leader", 100, "b")).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });
});
