import { describe, it, expect, afterEach, vi } from "vitest";
import { CircuitBreakerService } from "../src/resilience/circuitBreaker.js";
import { DeadLetterQueue } from "../src/resilience/deadLetterQueue.js";

afterEach(() => {
  vi.useRealTimers();
});

describe("circuit breaker transitions", () => {
  it("opens after the failure threshold, half-opens after the timeout, and closes on success", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
    const breaker = new CircuitBreakerService();
    breaker.register("rpc", { failureThreshold: 2, recoveryTimeoutMs: 1000, successThreshold: 1 });

    breaker.recordFailure("rpc");
    expect(breaker.getState("rpc")).toBe("closed");
    breaker.recordFailure("rpc");
    expect(breaker.getState("rpc")).toBe("open");

    vi.setSystemTime(new Date("2026-01-01T00:00:02Z"));
    expect(breaker.getState("rpc")).toBe("half-open");

    breaker.recordSuccess("rpc");
    expect(breaker.getState("rpc")).toBe("closed");
  });

  it("reopens when a half-open probe fails", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00Z"));
    const breaker = new CircuitBreakerService();
    breaker.register("rpc2", { failureThreshold: 1, recoveryTimeoutMs: 1000, successThreshold: 1 });
    breaker.recordFailure("rpc2");
    vi.setSystemTime(new Date("2026-01-01T00:00:02Z"));
    expect(breaker.getState("rpc2")).toBe("half-open");
    breaker.recordFailure("rpc2");
    expect(breaker.getState("rpc2")).toBe("open");
  });
});

describe("dead letter queue", () => {
  it("moves a job to dead once its replay attempts are used up", async () => {
    const dlq = new DeadLetterQueue();
    dlq.enqueue("job-exhaust", "notify", { to: "holder" }, "first failure", 2);
    const failing = async () => {
      throw new Error("still down");
    };

    expect(await dlq.replay("job-exhaust", failing)).toBe(false);
    expect(dlq.getJob("job-exhaust")?.status).toBe("pending");
    expect(await dlq.replay("job-exhaust", failing)).toBe(false);
    expect(dlq.getJob("job-exhaust")?.status).toBe("dead");
    expect(dlq.listJobs({ status: "dead" }).map((j) => j.id)).toContain("job-exhaust");
  });

  it("marks a job replayed when the handler succeeds", async () => {
    const dlq = new DeadLetterQueue();
    dlq.enqueue("job-ok", "notify", { to: "holder" }, "first failure", 3);
    expect(await dlq.replay("job-ok", async () => {})).toBe(true);
    expect(dlq.getJob("job-ok")?.status).toBe("replayed");
  });
});
