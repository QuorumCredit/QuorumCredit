import { describe, it, expect } from "vitest";
import {
  validateWebhookUrl,
  type WebhookUrlValidationOptions,
} from "../src/webhooks/signature.js";

describe("validateWebhookUrl", () => {
  const cases: Array<{
    url: string;
    ok: boolean;
    options?: WebhookUrlValidationOptions;
    contains?: string;
  }> = [
    { url: "https://example.com/hook", ok: true },
    { url: "http://example.com/hook", ok: true },
    { url: "https://sub.example.com/hook", ok: true },
    { url: "ftp://example.com/hook", ok: false, contains: "unsupported scheme" },
    { url: "file:///etc/passwd", ok: false, contains: "unsupported scheme" },
    { url: "http://localhost/hook", ok: false, contains: "blocked host" },
    { url: "http://127.0.0.1/hook", ok: false, contains: "blocked host" },
    { url: "http://169.254.169.254/latest/meta-data", ok: false, contains: "blocked host" },
    { url: "http://10.0.0.1/hook", ok: false, contains: "private IP range" },
    { url: "http://172.16.0.1/hook", ok: false, contains: "private IP range" },
    { url: "http://192.168.1.1/hook", ok: false, contains: "private IP range" },
    { url: "http://169.254.1.1/hook", ok: false, contains: "private IP range" },
    { url: "http://0.0.0.0/hook", ok: false, contains: "loopback" },
    { url: "http://metadata.google.internal/hook", ok: false, contains: "blocked host" },
  ];

  for (const c of cases) {
    it(`url=${c.url} => ${c.ok ? "accept" : "reject"}`, () => {
      const result = validateWebhookUrl(c.url, c.options);
      if (c.ok) {
        expect(result.toString()).toBe(new URL(c.url).toString());
      } else {
        expect(() => validateWebhookUrl(c.url, c.options)).toThrow(c.contains ?? "unsupported scheme");
      }
    });
  }

  it("allows private hosts when allowPrivateHosts=true", () => {
    expect(() => validateWebhookUrl("http://localhost/hook", { allowPrivateHosts: true })).not.toThrow();
    expect(() => validateWebhookUrl("http://10.0.0.1/hook", { allowPrivateHosts: true })).not.toThrow();
  });
});
