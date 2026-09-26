import { describe, expect, it, vi } from "vitest";
import { RetryExecutor } from "../src/resilience/retry.js";

describe("RetryExecutor", () => {
  it("uses capped exponential delays with full jitter and records retry statistics", async () => {
    const delays: number[] = [];
    const sleep = vi.fn(async (delay: number) => {
      delays.push(delay);
    });
    const executor = new RetryExecutor(() => 0.5, sleep);
    executor.setPolicy("rpc", {
      maxRetries: 3,
      baseDelayMs: 100,
      maxDelayMs: 250,
      jitter: true,
    });
    let calls = 0;

    const result = await executor.execute("rpc", async () => ++calls, (value) => value < 3);

    expect(result).toEqual({ value: 3, attempts: 3, exhaustedRetries: false });
    expect(sleep).toHaveBeenCalledTimes(2);
    expect(delays).toEqual([50, 100]);
    expect(executor.getStatistics().rpc).toEqual({
      attempts: 3,
      retries: 2,
      succeeded: 1,
      failed: 0,
      totalDelayMs: 150,
    });
  });

  it("stops after the configured retry count and records exhaustion", async () => {
    const executor = new RetryExecutor(() => 0, async () => undefined);
    executor.setPolicy("import", {
      maxRetries: 1,
      baseDelayMs: 10,
      maxDelayMs: 10,
      jitter: false,
    });
    const attempt = vi.fn(async () => false);

    const result = await executor.execute("import", attempt, (ok) => !ok);

    expect(result).toEqual({ value: false, attempts: 2, exhaustedRetries: true });
    expect(attempt).toHaveBeenCalledTimes(2);
    expect(executor.getStatistics().import.failed).toBe(1);
  });

  it("does not retry non-retryable exceptions", async () => {
    const executor = new RetryExecutor();
    executor.setPolicy("rpc", {
      maxRetries: 4,
      baseDelayMs: 1,
      maxDelayMs: 1,
      jitter: false,
    });
    const error = new Error("bad request");
    const attempt = vi.fn(async () => {
      throw error;
    });

    await expect(
      executor.execute("rpc", attempt, () => false, {
        retryOnError: () => false,
      })
    ).rejects.toBe(error);
    expect(attempt).toHaveBeenCalledTimes(1);
    expect(executor.getStatistics().rpc.failed).toBe(1);
  });
});
