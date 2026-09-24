import { describe, it, expect, vi, beforeEach } from "vitest";
import { WebhookDeliveryService, MAX_RETRIES, BASE_DELAY_MS, backoffDelayMs } from "../../src/webhooks/delivery.js";

describe("webhook delivery rate limiting", () => {
  it("backoffDelayMs returns increasing delays", () => {
    expect(backoffDelayMs(1)).toBe(BASE_DELAY_MS);
    expect(backoffDelayMs(2)).toBe(BASE_DELAY_MS * 2);
    expect(backoffDelayMs(3)).toBe(BASE_DELAY_MS * 4);
  });

  it("delivery respects MAX_RETRIES", async () => {
    const service = new WebhookDeliveryService();
    let attempts = 0;
    const sender = async () => {
      attempts++;
      return { ok: false, error: "always fails" };
    };

    const registration = {
      id: "wh-1",
      url: "https://example.com",
      events: [],
      secret: "secret",
    };

    const record = await service.deliver(registration, "test", {}, sender, (ms) => Promise.resolve());
    expect(record.status).toBe("failed");
    expect(record.attempts.length).toBe(MAX_RETRIES + 1);
    expect(attempts).toBe(MAX_RETRIES + 1);
  });

  it("succeeds on first attempt", async () => {
    const service = new WebhookDeliveryService();
    let attempts = 0;
    const sender = async () => {
      attempts++;
      return { ok: true, statusCode: 200 };
    };

    const registration = {
      id: "wh-1",
      url: "https://example.com",
      events: [],
      secret: "secret",
    };

    const record = await service.deliver(registration, "test", {}, sender, (ms) => Promise.resolve());
    expect(record.status).toBe("delivered");
    expect(attempts).toBe(1);
  });

  it("stats compute success rate correctly", () => {
    const service = new WebhookDeliveryService();

    // Manually inject records for stats testing.
    (service as any).deliveries.set("dlv_1_1", {
      id: "dlv_1_1",
      webhookId: "wh-1",
      event: "test",
      createdAt: 1,
      completedAt: 2,
      attempts: [{ attempt: 1, atMs: 1, ok: true }],
      status: "delivered",
    });
    (service as any).deliveries.set("dlv_1_2", {
      id: "dlv_1_2",
      webhookId: "wh-1",
      event: "test",
      createdAt: 1,
      completedAt: 2,
      attempts: [{ attempt: 1, atMs: 1, ok: false }],
      status: "failed",
    });
    (service as any).byWebhook.set("wh-1", ["dlv_1_1", "dlv_1_2"]);

    const stats = service.stats("wh-1");
    expect(stats.totalDeliveries).toBe(2);
    expect(stats.delivered).toBe(1);
    expect(stats.failed).toBe(1);
    expect(stats.successRateBps).toBe(5000);
  });
});
