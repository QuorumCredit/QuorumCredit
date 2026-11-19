import { afterEach, describe, expect, it, vi } from "vitest";
import { PrivacyShield, laplaceNoise, secureUniform } from "../src/credentials/privacyShield.js";

const KEY = "0123456789abcdef-test-shield-key";
const HOLDER = "GHOLDERADDRESS0000000000000000000000000000000000000001";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("privacy shield noise source", () => {
  it("secureUniform returns values strictly inside (0, 1)", () => {
    for (let i = 0; i < 1000; i++) {
      const u = secureUniform();
      expect(u).toBeGreaterThan(0);
      expect(u).toBeLessThan(1);
    }
  });

  it("default releaseCount path does not call Math.random", () => {
    const spy = vi.spyOn(Math, "random");
    const shield = new PrivacyShield({ salt: KEY, now: () => 1 });
    for (let i = 0; i < 50; i++) {
      shield.releaseCount("analyst", "count", 10);
    }
    expect(spy).not.toHaveBeenCalled();
  });

  it("default laplaceNoise path does not call Math.random", () => {
    const spy = vi.spyOn(Math, "random");
    laplaceNoise(1);
    expect(spy).not.toHaveBeenCalled();
  });

  it("injected random option still drives deterministic output for tests", () => {
    const shield = new PrivacyShield({ salt: KEY, now: () => 1, random: () => 0.5 });
    expect(shield.releaseCount("analyst", "count", 7)).toBe(7);
    expect(shield.pseudonymize(HOLDER)).toHaveLength(32);
  });
});
