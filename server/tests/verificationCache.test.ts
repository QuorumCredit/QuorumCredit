import { describe, expect, it } from "vitest";
import {
  DEFAULT_VERIFICATION_CACHE_TTL_MS,
  VerificationCache,
  verificationCacheTtlFromEnv,
} from "../src/verification/verificationCache.js";

function clock(start = 1_000) {
  let t = start;
  return { now: () => t, advance: (ms: number) => (t += ms) };
}

describe("VerificationCache", () => {
  it("validates ttl and capacity", () => {
    expect(() => new VerificationCache({ ttlMs: 0 })).toThrow(/ttlMs/);
    expect(() => new VerificationCache({ maxEntries: 0 })).toThrow(/maxEntries/);
  });

  it("returns cached values until the TTL elapses, then expires them", () => {
    const c = clock();
    const cache = new VerificationCache<string>({ ttlMs: 100, now: c.now });
    cache.set("cred-1", "valid");
    expect(cache.get("cred-1")).toBe("valid");
    c.advance(99);
    expect(cache.get("cred-1")).toBe("valid");
    c.advance(1);
    expect(cache.get("cred-1")).toBeUndefined();
    expect(cache.stats().expirations).toBe(1);
  });

  it("invalidates all entries sharing a tag in one call", () => {
    const cache = new VerificationCache<string>({ now: clock().now });
    cache.set("verify:cred-1:issuer-a", "valid", ["cred-1"]);
    cache.set("verify:cred-1:issuer-b", "valid", ["cred-1"]);
    cache.set("verify:cred-2", "revoked", ["cred-2"]);
    expect(cache.invalidateTag("cred-1")).toBe(2);
    expect(cache.get("verify:cred-1:issuer-a")).toBeUndefined();
    expect(cache.get("verify:cred-2")).toBe("revoked");
    expect(cache.invalidateTag("cred-1")).toBe(0);
  });

  it("removes tag index entries when a tagged key is invalidated directly", () => {
    const cache = new VerificationCache<string>({ now: clock().now });
    cache.set("k", "v", ["t"]);
    expect(cache.invalidate("k")).toBe(true);
    expect(cache.invalidate("k")).toBe(false);
    expect(cache.invalidateTag("t")).toBe(0);
  });

  it("evicts least-recently-used entries beyond capacity", () => {
    const cache = new VerificationCache<number>({ maxEntries: 2, now: clock().now });
    cache.set("a", 1);
    cache.set("b", 2);
    expect(cache.get("a")).toBe(1); // a is now most recently used
    cache.set("c", 3); // evicts b
    expect(cache.get("b")).toBeUndefined();
    expect(cache.get("a")).toBe(1);
    expect(cache.get("c")).toBe(3);
    expect(cache.stats()).toMatchObject({ evictions: 1, size: 2 });
  });

  it("tracks hits, misses and hit rate", () => {
    const cache = new VerificationCache<string>({ now: clock().now });
    expect(cache.stats().hitRate).toBe(0);
    cache.set("k", "v");
    cache.get("k");
    cache.get("missing");
    expect(cache.stats()).toMatchObject({ hits: 1, misses: 1, hitRate: 0.5, size: 1 });
  });

  it("computes once and serves subsequent lookups from cache", async () => {
    const cache = new VerificationCache<string>({ now: clock().now });
    let calls = 0;
    const compute = async () => {
      calls += 1;
      return "valid";
    };
    expect(await cache.getOrCompute("k", compute, ["cred"])).toBe("valid");
    expect(await cache.getOrCompute("k", compute, ["cred"])).toBe("valid");
    expect(calls).toBe(1);
    cache.invalidateTag("cred");
    await cache.getOrCompute("k", compute, ["cred"]);
    expect(calls).toBe(2);
  });
});

describe("verificationCacheTtlFromEnv", () => {
  it("uses the default when unset or invalid, and the configured value otherwise", () => {
    expect(verificationCacheTtlFromEnv({})).toBe(DEFAULT_VERIFICATION_CACHE_TTL_MS);
    expect(verificationCacheTtlFromEnv({ VERIFICATION_CACHE_TTL_MS: "abc" })).toBe(
      DEFAULT_VERIFICATION_CACHE_TTL_MS
    );
    expect(verificationCacheTtlFromEnv({ VERIFICATION_CACHE_TTL_MS: "-5" })).toBe(
      DEFAULT_VERIFICATION_CACHE_TTL_MS
    );
    expect(verificationCacheTtlFromEnv({ VERIFICATION_CACHE_TTL_MS: "5000" })).toBe(5000);
  });
});
