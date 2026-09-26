import { describe, expect, it } from "vitest";
import { createHash } from "node:crypto";
import { buildApiKeyStore, loadApiKeyStore } from "../src/auth/apiKeyStore.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";

describe("API user tiers", () => {
  it("assigns provisioned tiers by API-key hash without exposing raw keys", () => {
    const key = "enterprise-api-key";
    const hash = createHash("sha256").update(key).digest("hex");
    const store = buildApiKeyStore(hash, { [hash]: "enterprise" });

    expect(store.getTier(key)).toBe("enterprise");
    expect(store.getTier("unknown-key")).toBeUndefined();
    expect(store.getTier(key)).not.toBe(key);
  });

  it("loads tier assignments from environment using key hashes", () => {
    const key = "configured-api-key";
    const hash = createHash("sha256").update(key).digest("hex");
    const store = loadApiKeyStore({
      API_KEY_HASHES: hash,
      API_KEY_TIERS: JSON.stringify({ [hash]: "pro" }),
    });
    expect(store.isValid(key)).toBe(true);
    expect(store.getTier(key)).toBe("pro");
  });

  it("rejects malformed tier configuration rather than silently downgrading keys", () => {
    expect(() =>
      loadApiKeyStore({
        API_KEY_HASHES: "",
        API_KEY_TIERS: JSON.stringify({ "not-a-hash": "enterprise" }),
      })
    ).toThrow("SHA-256 hashes");
  });

  it("tracks usage by identity and enforces each tier limit independently", async () => {
    const limiter = new LocalApiRateLimiter({
      windowMs: 60_000,
      tiers: { free: 2, pro: 5, enterprise: 10 },
    });

    expect(await limiter.consume("free-user", "free")).toMatchObject({
      limit: 2,
      remaining: 1,
      allowed: true,
    });
    expect(await limiter.consume("free-user", "free")).toMatchObject({
      limit: 2,
      remaining: 0,
      allowed: true,
    });
    expect(await limiter.consume("free-user", "free")).toMatchObject({
      limit: 2,
      remaining: 0,
      allowed: false,
    });
    expect(await limiter.consume("pro-user", "pro")).toMatchObject({
      limit: 5,
      remaining: 4,
      allowed: true,
    });
    await limiter.close();
  });
});
