import { describe, it, expect, afterEach, vi } from "vitest";
import { RegionHealthChecker } from "../src/http/crossRegionReplication.js";
import { loadRegionConfigFromEnv } from "../src/http/regionConfig.js";

afterEach(() => {
  vi.useRealTimers();
});

describe("region health checker", () => {
  it("clears its interval timer on stop", () => {
    vi.useFakeTimers();
    const checker = new RegionHealthChecker(1000, "local");
    checker.start();
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    checker.stop();
    expect(vi.getTimerCount()).toBe(0);
  });
});

describe("region config fallback", () => {
  it("falls back to the default routing strategy when no region keys are set", () => {
    const config = loadRegionConfigFromEnv({});
    expect(config.strategy).toBe("primary-first");
    expect(Array.isArray(config.regions)).toBe(true);
  });
});
