import { describe, expect, it } from "vitest";
import {
  VerificationFallbackChain,
  VerificationUnavailableError,
  type Attestor,
} from "../src/verification/fallbackChain.js";

const noSleep = async () => {};

function attestor(id: string, behaviour: Array<boolean | Error>): Attestor & { calls: number } {
  let index = 0;
  const entry = {
    id,
    calls: 0,
    async verify(): Promise<boolean> {
      entry.calls++;
      const next = behaviour[Math.min(index, behaviour.length - 1)]!;
      index++;
      if (next instanceof Error) throw next;
      return next;
    },
  };
  return entry;
}

describe("VerificationFallbackChain", () => {
  it("uses the primary attestor when it answers", async () => {
    const primary = attestor("primary", [true]);
    const backup = attestor("backup", [true]);
    const chain = new VerificationFallbackChain([primary, backup], { sleep: noSleep });

    const result = await chain.verify("cred_1");
    expect(result).toMatchObject({ verified: true, attestorId: "primary", fallbackUsed: false });
    expect(backup.calls).toBe(0);
  });

  it("falls back to the next attestor once the primary is exhausted", async () => {
    const primary = attestor("primary", [new Error("down"), new Error("still down")]);
    const backup = attestor("backup", [true]);
    const chain = new VerificationFallbackChain([primary, backup], {
      maxAttempts: 2,
      sleep: noSleep,
    });

    const result = await chain.verify("cred_1");
    expect(result).toMatchObject({ verified: true, attestorId: "backup", fallbackUsed: true });
    expect(result.attempts.map((a) => [a.attestorId, a.attempt, a.outcome])).toEqual([
      ["primary", 1, "error"],
      ["primary", 2, "error"],
      ["backup", 1, "verified"],
    ]);
    expect(result.attempts[0]!.error).toBe("down");
  });

  it("retries a flaky primary before falling back", async () => {
    const primary = attestor("primary", [new Error("blip"), false]);
    const backup = attestor("backup", [true]);
    const chain = new VerificationFallbackChain([primary, backup], { sleep: noSleep });

    const result = await chain.verify("cred_1");
    expect(result).toMatchObject({ verified: false, attestorId: "primary", fallbackUsed: false });
    expect(backup.calls).toBe(0);
  });

  it("never overturns a definitive rejection with a fallback attestor", async () => {
    const primary = attestor("primary", [false]);
    const backup = attestor("backup", [true]);
    const chain = new VerificationFallbackChain([primary, backup], { sleep: noSleep });

    const result = await chain.verify("cred_1");
    expect(result).toMatchObject({ verified: false, attestorId: "primary" });
    expect(backup.calls).toBe(0);
  });

  it("throws VerificationUnavailableError with the full attempt log when all attestors fail", async () => {
    const chain = new VerificationFallbackChain(
      [
        attestor("a", [new Error("x")]),
        attestor("b", [new Error("y")]),
      ],
      { maxAttempts: 1, sleep: noSleep }
    );

    const error = await chain.verify("cred_1").catch((e: unknown) => e);
    expect(error).toBeInstanceOf(VerificationUnavailableError);
    expect((error as VerificationUnavailableError).attempts).toHaveLength(2);
  });

  it("waits according to the backoff schedule between retries", async () => {
    const waits: number[] = [];
    const primary = attestor("primary", [new Error("a"), new Error("b"), true]);
    const chain = new VerificationFallbackChain([primary], {
      maxAttempts: 3,
      backoffMs: (attempt) => attempt * 100,
      sleep: async (ms) => {
        waits.push(ms);
      },
    });

    await chain.verify("cred_1");
    expect(waits).toEqual([100, 200]);
  });

  it("tracks per-attestor stats and fallback usage", async () => {
    const primary = attestor("primary", [new Error("down")]);
    const backup = attestor("backup", [true, true]);
    const chain = new VerificationFallbackChain([primary, backup], {
      maxAttempts: 1,
      sleep: noSleep,
    });

    await chain.verify("cred_1");
    await chain.verify("cred_2");

    expect(chain.getStats()).toEqual({
      fallbackUsed: 2,
      attestors: [
        { attestorId: "primary", attempts: 2, successes: 0, failures: 2 },
        { attestorId: "backup", attempts: 2, successes: 2, failures: 0 },
      ],
    });
  });

  it("validates the attestor configuration", () => {
    expect(() => new VerificationFallbackChain([])).toThrow("at least one attestor");
    expect(
      () => new VerificationFallbackChain([attestor("dup", [true]), attestor("dup", [true])])
    ).toThrow("attestor ids must be unique");
    expect(
      () => new VerificationFallbackChain([attestor("a", [true])], { maxAttempts: 0 })
    ).toThrow("maxAttempts must be a positive integer");
  });
});
