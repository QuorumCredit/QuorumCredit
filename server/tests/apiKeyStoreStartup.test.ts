import { describe, it, expect } from "vitest";
import { loadApiKeyStore } from "../src/auth/apiKeyStore.js";

describe("API key store startup", () => {
  it("refuses to start in production without key hashes", () => {
    expect(() => loadApiKeyStore({ NODE_ENV: "production" })).toThrow(/API_KEY_HASHES/);
  });

  it("allows an empty key store outside production", () => {
    expect(() => loadApiKeyStore({ NODE_ENV: "development" })).not.toThrow();
  });
});
