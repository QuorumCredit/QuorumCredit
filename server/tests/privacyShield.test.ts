import { describe, expect, it } from "vitest";
import { PrivacyShield, laplaceNoise } from "../src/credentials/privacyShield.js";

const KEY = "0123456789abcdef-test-shield-key";
const HOLDER = "GHOLDERADDRESS0000000000000000000000000000000000000001";

function shield(overrides: Partial<ConstructorParameters<typeof PrivacyShield>[0]> = {}) {
  let now = 10;
  return new PrivacyShield({ salt: KEY, now: () => now++, random: () => 0.5, ...overrides });
}

describe("PrivacyShield", () => {
  it("rejects weak keys and invalid epsilon", () => {
    expect(() => new PrivacyShield({ salt: "short" })).toThrow(/at least 16 bytes/);
    expect(() => new PrivacyShield({ salt: KEY, epsilon: 0 })).toThrow(/epsilon/);
  });

  it("pseudonymizes holders stably, without exposing the address, and keyed by secret", () => {
    const a = shield();
    const b = shield();
    expect(a.pseudonymize(HOLDER)).toBe(b.pseudonymize(HOLDER));
    expect(a.pseudonymize(HOLDER)).not.toContain("GHOLDER");
    const other = new PrivacyShield({ salt: "a-different-secret-key-value" });
    expect(other.pseudonymize(HOLDER)).not.toBe(a.pseudonymize(HOLDER));
  });

  it("shields a record by replacing the holder and stripping requested fields", () => {
    const s = shield();
    const record = { holder: HOLDER, score: 720, metadata: "passport 123" };
    const shielded = s.shieldRecord(record, {
      holderField: "holder",
      stripFields: ["metadata"],
      actor: "analyst",
    });
    expect(shielded).toEqual({ holder: s.pseudonymize(HOLDER), score: 720 });
    expect(record.holder).toBe(HOLDER); // input not mutated
  });

  it("rejects records without a holder value", () => {
    expect(() =>
      shield().shieldRecord({ holder: "" } as Record<string, unknown>, { holderField: "holder", actor: "x" })
    ).toThrow(/missing holder/);
  });

  it("releases counts clamped to zero, with noise that is deterministic under a fixed sampler", () => {
    const s = shield({ epsilon: 1, random: () => 0.5 });
    expect(s.releaseCount("analyst", "active-loans", 42)).toBe(42);
    const low = shield({ random: () => 0.0001 });
    expect(low.releaseCount("analyst", "active-loans", 0)).toBe(0);
  });

  it("rejects negative or fractional true counts", () => {
    expect(() => shield().releaseCount("a", "x", -1)).toThrow(/non-negative integer/);
    expect(() => shield().releaseCount("a", "x", 1.5)).toThrow(/non-negative integer/);
  });

  it("records disclosures in an audit trail filterable by holder pseudonym", () => {
    const s = shield();
    s.shieldRecord({ holder: HOLDER }, { holderField: "holder", actor: "analyst" });
    s.releaseCount("analyst", "count", 3);
    const subject = s.pseudonymize(HOLDER);
    expect(s.auditTrail().map((e) => e.action)).toEqual(["holder-disclosure", "aggregate-release"]);
    expect(s.auditTrail(subject)).toHaveLength(1);
  });

  it("bounds the audit trail to its configured limit, keeping the newest entries", () => {
    const s = shield({ auditLimit: 2 });
    s.releaseCount("a", "one", 1);
    s.releaseCount("a", "two", 2);
    s.releaseCount("a", "three", 3);
    expect(s.auditTrail().map((e) => e.detail)).toEqual(["two", "three"]);
  });
});

describe("laplaceNoise", () => {
  it("is zero at the sampler midpoint and stays finite at the boundary", () => {
    expect(laplaceNoise(1, () => 0.5)).toBeCloseTo(0, 12);
    expect(Number.isFinite(laplaceNoise(1, () => 0))).toBe(true);
    expect(Number.isFinite(laplaceNoise(1, () => 0.999999))).toBe(true);
  });

  it("has mean near zero and the expected spread over many samples", () => {
    let seed = 42;
    const random = () => {
      seed = (seed * 1103515245 + 12345) & 0x7fffffff;
      return (seed + 1) / 0x80000001;
    };
    const samples = Array.from({ length: 20_000 }, () => laplaceNoise(1, random));
    const mean = samples.reduce((sum, v) => sum + v, 0) / samples.length;
    const meanAbs = samples.reduce((sum, v) => sum + Math.abs(v), 0) / samples.length;
    expect(Math.abs(mean)).toBeLessThan(0.05);
    // E|X| = scale for Laplace(0, scale)
    expect(Math.abs(meanAbs - 1)).toBeLessThan(0.05);
  });
});
