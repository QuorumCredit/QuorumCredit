import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  WebhookDeliveryService,
  RedisWebhookDeliveryService,
  buildWebhookDeliveryService,
  backoffDelayMs,
  MAX_RETRIES,
  type DeliveryRecord,
  type WebhookSender,
  type WebhookRegistration,
} from "../src/webhooks/delivery.js";

function createRegistration(overrides: Partial<WebhookRegistration> = {}): WebhookRegistration {
  return {
    id: "wh_test",
    url: "https://example.com/webhook",
    secret: "secret",
    createdAt: new Date(),
    events: [],
    enabled: true,
    ...overrides,
  };
}

describe("WebhookDeliveryService", () => {
  let service: WebhookDeliveryService;

  beforeEach(() => {
    service = new WebhookDeliveryService({
      maxRecordsPerWebhook: 10,
      deliveredTtlMs: 1_000,
      pendingTtlMs: 1_000,
      sweepIntervalMs: 100,
    });
  });

  it("records a successful delivery", async () => {
    const sender: WebhookSender = vi.fn().mockResolvedValue({ ok: true, statusCode: 200 });
    const reg = createRegistration();
    const record = await service.deliver(reg, "test", {}, sender);

    expect(record.status).toBe("delivered");
    expect(record.attempts).toHaveLength(1);
    expect(sender).toHaveBeenCalledTimes(1);
  });

  it("retries on failure up to MAX_RETRIES", async () => {
    const sender: WebhookSender = vi.fn().mockResolvedValue({ ok: false, statusCode: 500 });
    const reg = createRegistration();
    const record = await service.deliver(reg, "test", {}, sender, (ms) => new Promise((r) => setTimeout(r, ms)));

    expect(record.status).toBe("failed");
    expect(record.attempts).toHaveLength(MAX_RETRIES + 1);
    expect(sender).toHaveBeenCalledTimes(MAX_RETRIES + 1);
  });

  it("evicts oldest records when per-webhook cap is exceeded", async () => {
    const sender: WebhookSender = vi.fn().mockResolvedValue({ ok: true, statusCode: 200 });
    const reg = createRegistration({ id: "wh_eviction" });

    const records: DeliveryRecord[] = [];
    for (let i = 0; i < 12; i++) {
      const rec = await service.deliver(reg, "test", {}, sender, (ms) => new Promise((r) => setTimeout(r, ms)));
      records.push(rec);
    }

    const listed = service.listDeliveries("wh_eviction");
    expect(listed.length).toBeLessThanOrEqual(10);
  });

  it("sweeps expired records after TTL", async () => {
    const sender: WebhookSender = vi.fn().mockResolvedValue({ ok: true, statusCode: 200 });
    const reg = createRegistration({ id: "wh_ttl" });

    await service.deliver(reg, "test", {}, sender, (ms) => new Promise((r) => setTimeout(r, ms)));
    expect(service.listDeliveries("wh_ttl")).toHaveLength(1);

    await new Promise((r) => setTimeout(r, 1200));
    expect(service.listDeliveries("wh_ttl")).toHaveLength(0);
  });

  it("exposes current record count", async () => {
    const sender: WebhookSender = vi.fn().mockResolvedValue({ ok: true, statusCode: 200 });
    const reg = createRegistration({ id: "wh_count" });

    expect(service.getRecordCount()).toBe(0);
    await service.deliver(reg, "test", {}, sender, (ms) => new Promise((r) => setTimeout(r, ms)));
    expect(service.getRecordCount()).toBe(1);
  });
});

describe("RedisWebhookDeliveryService", () => {
  it("persists and retrieves delivery records", async () => {
    const mockRedis = {
      set: vi.fn().mockResolvedValue(undefined),
      sadd: vi.fn().mockResolvedValue(1),
      get: vi.fn().mockResolvedValue(JSON.stringify({
        id: "dlv_1",
        webhookId: "wh_1",
        event: "test",
        createdAt: Date.now(),
        attempts: [],
        status: "delivered",
        completedAt: Date.now(),
      })),
      smembers: vi.fn().mockResolvedValue(["dlv_1"]),
      keys: vi.fn().mockResolvedValue(["qc:webhook:delivery:dlv_1"]),
      quit: vi.fn().mockResolvedValue(undefined),
    } as any;

    const service = new RedisWebhookDeliveryService(mockRedis);
    const sender: WebhookSender = vi.fn().mockResolvedValue({ ok: true, statusCode: 200 });
    const reg = createRegistration({ id: "wh_1" });

    const record = await service.deliver(reg, "test", { foo: "bar" }, sender);
    expect(record.status).toBe("delivered");
    expect(mockRedis.set).toHaveBeenCalled();
    expect(mockRedis.sadd).toHaveBeenCalled();

    const retrieved = await service.getDelivery("dlv_1");
    expect(retrieved).toBeDefined();
    expect(retrieved!.event).toBe("test");
  });

  it("enforces TTL eviction", async () => {
    const oldTimestamp = Date.now() - 2_000_000;
    const mockRedis = {
      set: vi.fn().mockResolvedValue(undefined),
      sadd: vi.fn().mockResolvedValue(1),
      get: vi.fn().mockResolvedValue(JSON.stringify({
        id: "dlv_old",
        webhookId: "wh_old",
        event: "test",
        createdAt: oldTimestamp,
        attempts: [],
        status: "delivered",
        completedAt: oldTimestamp,
      })),
      smembers: vi.fn().mockResolvedValue(["dlv_old"]),
      keys: vi.fn().mockResolvedValue(["qc:webhook:delivery:dlv_old"]),
      srem: vi.fn().mockResolvedValue(1),
      del: vi.fn().mockResolvedValue(1),
      quit: vi.fn().mockResolvedValue(undefined),
    } as any;

    const service = new RedisWebhookDeliveryService(mockRedis, {
      deliveredTtlMs: 1_000,
      pendingTtlMs: 1_000,
      sweepIntervalMs: 100,
      maxRecordsPerWebhook: 100,
    });

    await service.deliver(createRegistration({ id: "wh_old" }), "test", {}, vi.fn().mockResolvedValue({ ok: true }));
    expect(mockRedis.del).toHaveBeenCalled();
  });
});

describe("buildWebhookDeliveryService", () => {
  it("returns local service when redisUrl is undefined", () => {
    const service = buildWebhookDeliveryService(undefined);
    expect(service).toBeInstanceOf(WebhookDeliveryService);
  });

  it("returns Redis service when redisUrl is provided", () => {
    const service = buildWebhookDeliveryService("redis://localhost:6379");
    expect(service).toBeInstanceOf(RedisWebhookDeliveryService);
  });
});

describe("backoffDelayMs", () => {
  it("doubles delay for each retry", () => {
    expect(backoffDelayMs(1)).toBe(500);
    expect(backoffDelayMs(2)).toBe(1000);
    expect(backoffDelayMs(3)).toBe(2000);
  });
});
