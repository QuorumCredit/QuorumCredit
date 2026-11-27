import { describe, expect, it, vi } from "vitest";
import { loadConfig } from "../src/config.js";
import { validateWebhookUrl } from "../src/webhooks/signature.js";

describe("private webhook host configuration", () => {
  it("keeps private webhook hosts disabled when the opt-in is unset", () => {
    const config = loadConfig({
      NODE_ENV: undefined,
      ALLOW_PRIVATE_WEBHOOK_HOSTS: undefined,
      AUTH_SECRET: "a".repeat(32),
    });
    expect(config.allowPrivateWebhookHosts).toBe(false);
  });

  it("rejects http://127.0.0.1/ when NODE_ENV is unset", () => {
    vi.stubEnv("NODE_ENV", undefined);
    vi.stubEnv("ALLOW_PRIVATE_WEBHOOK_HOSTS", undefined);

    try {
      const config = loadConfig({
        ...process.env,
        NODE_ENV: undefined,
        ALLOW_PRIVATE_WEBHOOK_HOSTS: undefined,
        AUTH_SECRET: "a".repeat(32),
      });
      expect(() =>
        validateWebhookUrl("http://127.0.0.1/", {
          allowPrivateHosts: config.allowPrivateWebhookHosts,
        })
      ).toThrow(/blocked host/);
    } finally {
      vi.unstubAllEnvs();
    }
  });

  it("allows explicit opt-in outside production", () => {
    const config = loadConfig({
      NODE_ENV: "development",
      ALLOW_PRIVATE_WEBHOOK_HOSTS: "true",
      AUTH_SECRET: "a".repeat(32),
    });
    expect(config.allowPrivateWebhookHosts).toBe(true);
  });

  it("fails startup configuration when private webhook hosts are enabled in production", () => {
    expect(() =>
      loadConfig({
        NODE_ENV: "production",
        ALLOW_PRIVATE_WEBHOOK_HOSTS: "true",
        AUTH_SECRET: "a".repeat(32),
      })
    ).toThrow(/ALLOW_PRIVATE_WEBHOOK_HOSTS=true.*NODE_ENV=production/);
  });
});
