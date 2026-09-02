/**
 * Webhook Delivery Service for QuorumCredit
 *
 * #1XXX: Real-time webhook delivery with retry + delivery-rate tracking.
 *
 * Third-party integrators previously had to poll REST endpoints for loan
 * status changes. This module pushes events to registered subscriber URLs
 * as they happen, with exponential-backoff retries and per-webhook delivery
 * accounting so operators can see whether a subscriber endpoint is healthy.
 */

import { createSignedWebhookRequest, type WebhookRegistration } from "./signature.js";
import type { Redis } from "ioredis";

/** Canonical event types third-party apps can subscribe to. The legacy
 * dot-separated event names (loan.disbursed, etc.) remain supported in
 * signature.ts's validEvents list for backward compatibility, but new
 * integrations should use these. */
export type WebhookEventType =
  | "loan_issued"
  | "payment_received"
  | "loan_completed"
  | "default_occurred";

export const SUBSCRIBABLE_EVENTS: WebhookEventType[] = [
  "loan_issued",
  "payment_received",
  "loan_completed",
  "default_occurred",
];

export interface DeliveryAttempt {
  attempt: number;
  atMs: number;
  ok: boolean;
  statusCode?: number;
  error?: string;
}

export interface DeliveryRecord {
  id: string;
  webhookId: string;
  event: string;
  createdAt: number;
  completedAt?: number;
  attempts: DeliveryAttempt[];
  status: "pending" | "delivered" | "failed";
}

export interface DeliveryStats {
  webhookId: string;
  totalDeliveries: number;
  delivered: number;
  failed: number;
  successRateBps: number;
  averageAttempts: number;
}

/** Retries after the initial attempt, before a delivery is marked failed. */
export const MAX_RETRIES = 5;

/** Base delay for exponential backoff between delivery attempts (ms). Actual
 * delay for retry N is BASE_DELAY_MS * 2^(N-1), e.g. 500ms, 1s, 2s, 4s, 8s. */
export const BASE_DELAY_MS = 500;

export function backoffDelayMs(retryNumber: number): number {
  return BASE_DELAY_MS * Math.pow(2, retryNumber - 1);
}

/** Function that actually performs the HTTP POST to a subscriber's URL.
 * Injected so this module has no hard dependency on a particular HTTP
 * client and remains easy to unit test. */
export type WebhookSender = (
  url: string,
  headers: Record<string, string>,
  body: unknown
) => Promise<{ ok: boolean; statusCode?: number; error?: string }>;

/** Configuration for bounded in-memory delivery storage. */
export interface DeliveryStoreConfig {
  /** Maximum delivery records to retain per webhook. Oldest records are evicted first. */
  maxRecordsPerWebhook?: number;
  /** Maximum age of delivered/failed records in milliseconds before they are eligible for TTL sweep. */
  deliveredTtlMs?: number;
  /** Maximum age of pending records in milliseconds before they are eligible for TTL sweep. */
  pendingTtlMs?: number;
  /** Interval between automatic TTL sweeps in milliseconds. */
  sweepIntervalMs?: number;
}

const DEFAULT_DELIVERY_STORE_CONFIG: Required<DeliveryStoreConfig> = {
  maxRecordsPerWebhook: 500,
  deliveredTtlMs: 24 * 60 * 60 * 1000, // 24 hours
  pendingTtlMs: 2 * 60 * 60 * 1000, // 2 hours
  sweepIntervalMs: 5 * 60 * 1000, // 5 minutes
};

/**
 * Tracks webhook delivery attempts (with exponential-backoff retry) and
 * aggregate success rates per webhook subscription, in-memory. In production
 * this should be backed by a durable queue (e.g. a database table plus a
 * background worker) so retries survive process restarts, but the retry and
 * accounting logic here is deployment-agnostic.
 *
 * #1488: Bounded in-memory delivery store with configurable max-records cap,
 * TTL-based eviction, and delivery-record count metric.
 */
export class WebhookDeliveryService {
  private readonly deliveries = new Map<string, DeliveryRecord>();
  private readonly byWebhook = new Map<string, string[]>();
  private nextId = 1;
  private readonly config: Required<DeliveryStoreConfig>;
  private sweepTimer?: ReturnType<typeof setInterval>;

  constructor(config: DeliveryStoreConfig = {}) {
    this.config = { ...DEFAULT_DELIVERY_STORE_CONFIG, ...config };
    this.startSweepTimer();
  }

  /** Current number of delivery records held in memory. Exposed as a metric
   * so operators can spot unbounded growth before it becomes an OOM issue. */
  getRecordCount(): number {
    return this.deliveries.size;
  }

  /** Stop the background TTL sweeper. Call this on shutdown to avoid timers
   * keeping the process alive. */
  close(): void {
    if (this.sweepTimer) {
      clearInterval(this.sweepTimer);
      this.sweepTimer = undefined;
    }
  }

  /** Deliver an event to a single subscriber, retrying on failure with
   * exponential backoff up to MAX_RETRIES additional attempts. Returns the
   * final delivery record. */
  async deliver(
    registration: WebhookRegistration,
    event: string,
    data: unknown,
    send: WebhookSender,
    sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms))
  ): Promise<DeliveryRecord> {
    const signed = createSignedWebhookRequest(registration, event, data);

    const record: DeliveryRecord = {
      id: `dlv_${this.nextId++}_${Date.now()}`,
      webhookId: registration.id,
      event,
      createdAt: Date.now(),
      attempts: [],
      status: "pending",
    };

    this.upsertRecord(record);

    for (let attempt = 1; attempt <= MAX_RETRIES + 1; attempt++) {
      let result: { ok: boolean; statusCode?: number; error?: string };
      try {
        result = await send(signed.url, signed.headers, signed.payload);
      } catch (error) {
        result = { ok: false, error: error instanceof Error ? error.message : "unknown error" };
      }

      record.attempts.push({
        attempt,
        atMs: Date.now(),
        ok: result.ok,
        statusCode: result.statusCode,
        error: result.error,
      });

      if (result.ok) {
        record.status = "delivered";
        record.completedAt = Date.now();
        this.upsertRecord(record);
        return record;
      }

      const isLastAttempt = attempt === MAX_RETRIES + 1;
      if (!isLastAttempt) {
        await sleep(backoffDelayMs(attempt));
      }
    }

    record.status = "failed";
    record.completedAt = Date.now();
    this.upsertRecord(record);
    return record;
  }

  getDelivery(id: string): DeliveryRecord | undefined {
    return this.deliveries.get(id);
  }

  listDeliveries(webhookId: string): DeliveryRecord[] {
    const ids = this.byWebhook.get(webhookId) ?? [];
    return ids
      .map((id) => this.deliveries.get(id))
      .filter((d): d is DeliveryRecord => d !== undefined);
  }

  /** Success-rate and attempt accounting for a single webhook subscription,
   * per the "track webhook delivery success rates" requirement. */
  stats(webhookId: string): DeliveryStats {
    const records = this.listDeliveries(webhookId);
    const completed = records.filter((r) => r.status !== "pending");
    const delivered = completed.filter((r) => r.status === "delivered").length;
    const failed = completed.filter((r) => r.status === "failed").length;
    const totalAttempts = completed.reduce((sum, r) => sum + r.attempts.length, 0);

    return {
      webhookId,
      totalDeliveries: completed.length,
      delivered,
      failed,
      successRateBps: completed.length === 0 ? 0 : Math.round((delivered / completed.length) * 10_000),
      averageAttempts: completed.length === 0 ? 0 : totalAttempts / completed.length,
    };
  }

  private upsertRecord(record: DeliveryRecord): void {
    this.deliveries.set(record.id, record);
    const list = this.byWebhook.get(record.webhookId);
    if (list) {
      if (!list.includes(record.id)) list.push(record.id);
    } else {
      this.byWebhook.set(record.webhookId, [record.id]);
    }
    this.evictIfNeeded(record.webhookId);
  }

  private evictIfNeeded(webhookId: string): void {
    const ids = this.byWebhook.get(webhookId);
    if (!ids || ids.length <= this.config.maxRecordsPerWebhook) return;

    const now = Date.now();
    const scored = ids
      .map((id) => {
        const rec = this.deliveries.get(id);
        if (!rec) return { id, score: Infinity };
        const completedAt = rec.completedAt ?? rec.createdAt;
        const age = now - completedAt;
        return { id, score: age };
      })
      .sort((a, b) => b.score - a.score);

    const excess = scored.slice(this.config.maxRecordsPerWebhook);
    for (const entry of excess) {
      this.deliveries.delete(entry.id);
    }
    this.byWebhook.set(
      webhookId,
      ids.filter((id) => !excess.some((e) => e.id === id))
    );
  }

  private startSweepTimer(): void {
    this.sweepTimer = setInterval(() => this.sweepExpired(), this.config.sweepIntervalMs);
  }

  private sweepExpired(): void {
    const now = Date.now();
    const expired: string[] = [];

    for (const [webhookId, ids] of this.byWebhook.entries()) {
      const remaining: string[] = [];
      for (const id of ids) {
        const rec = this.deliveries.get(id);
        if (!rec) continue;

        const completedAt = rec.completedAt ?? rec.createdAt;
        const ttl =
          rec.status === "pending" ? this.config.pendingTtlMs : this.config.deliveredTtlMs;

        if (now - completedAt > ttl) {
          expired.push(id);
        } else {
          remaining.push(id);
        }
      }
      if (remaining.length === 0) {
        this.byWebhook.delete(webhookId);
      } else {
        this.byWebhook.set(webhookId, remaining);
      }
    }

    for (const id of expired) {
      this.deliveries.delete(id);
    }
  }
}

export const webhookDeliveryService = new WebhookDeliveryService();

// ── Redis-backed delivery service (issue #1489) ────────────────────────────────

const REDIS_DELIVERY_PREFIX = "qc:webhook:delivery:";
const REDIS_DELIVERY_INDEX_PREFIX = "qc:webhook:delivery:index:";

/**
 * Redis-backed webhook delivery service for multi-instance deployments.
 *
 * Storage schema:
 *   qc:webhook:delivery:{id} → JSON serialized DeliveryRecord
 *   qc:webhook:delivery:index:{webhookId} → SET of delivery IDs for that webhook
 */
export class RedisWebhookDeliveryService {
  constructor(private readonly redis: Redis, private readonly config: Required<DeliveryStoreConfig> = DEFAULT_DELIVERY_STORE_CONFIG) {}

  async deliver(
    registration: WebhookRegistration,
    event: string,
    data: unknown,
    send: WebhookSender,
    sleep: (ms: number) => Promise<void> = (ms) => new Promise((r) => setTimeout(r, ms))
  ): Promise<DeliveryRecord> {
    const signed = createSignedWebhookRequest(registration, event, data);

    const record: DeliveryRecord = {
      id: `dlv_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
      webhookId: registration.id,
      event,
      createdAt: Date.now(),
      attempts: [],
      status: "pending",
    };

    await this.persistRecord(record);

    for (let attempt = 1; attempt <= MAX_RETRIES + 1; attempt++) {
      let result: { ok: boolean; statusCode?: number; error?: string };
      try {
        result = await send(signed.url, signed.headers, signed.payload);
      } catch (error) {
        result = { ok: false, error: error instanceof Error ? error.message : "unknown error" };
      }

      record.attempts.push({
        attempt,
        atMs: Date.now(),
        ok: result.ok,
        statusCode: result.statusCode,
        error: result.error,
      });

      if (result.ok) {
        record.status = "delivered";
        record.completedAt = Date.now();
        await this.persistRecord(record);
        return record;
      }

      const isLastAttempt = attempt === MAX_RETRIES + 1;
      if (!isLastAttempt) {
        await sleep(backoffDelayMs(attempt));
      }
    }

    record.status = "failed";
    record.completedAt = Date.now();
    await this.persistRecord(record);
    return record;
  }

  async getDelivery(id: string): Promise<DeliveryRecord | undefined> {
    const raw = await this.redis.get(`${REDIS_DELIVERY_PREFIX}${id}`);
    if (!raw) return undefined;
    return JSON.parse(raw) as DeliveryRecord;
  }

  async listDeliveries(webhookId: string): Promise<DeliveryRecord[]> {
    const ids = await this.redis.smembers(`${REDIS_DELIVERY_INDEX_PREFIX}${webhookId}`);
    const records: DeliveryRecord[] = [];
    for (const id of ids) {
      const raw = await this.redis.get(`${REDIS_DELIVERY_PREFIX}${id}`);
      if (raw) {
        records.push(JSON.parse(raw) as DeliveryRecord);
      }
    }
    return records;
  }

  async stats(webhookId: string): Promise<DeliveryStats> {
    const records = await this.listDeliveries(webhookId);
    const completed = records.filter((r) => r.status !== "pending");
    const delivered = completed.filter((r) => r.status === "delivered").length;
    const failed = completed.filter((r) => r.status === "failed").length;
    const totalAttempts = completed.reduce((sum, r) => sum + r.attempts.length, 0);

    return {
      webhookId,
      totalDeliveries: completed.length,
      delivered,
      failed,
      successRateBps: completed.length === 0 ? 0 : Math.round((delivered / completed.length) * 10_000),
      averageAttempts: completed.length === 0 ? 0 : totalAttempts / completed.length,
    };
  }

  async getRecordCount(): Promise<number> {
    const keys = await this.redis.keys(`${REDIS_DELIVERY_PREFIX}*`);
    return keys.length;
  }

  async close(): Promise<void> {
    await this.redis.quit();
  }

  private async persistRecord(record: DeliveryRecord): Promise<void> {
    await this.redis.set(`${REDIS_DELIVERY_PREFIX}${record.id}`, JSON.stringify(record));
    await this.redis.sadd(`${REDIS_DELIVERY_INDEX_PREFIX}${record.webhookId}`, record.id);
    await this.enforceTTL(record.webhookId);
  }

  private async enforceTTL(webhookId: string): Promise<void> {
    const ids = await this.redis.smembers(`${REDIS_DELIVERY_INDEX_PREFIX}${webhookId}`);
    const now = Date.now();
    const expired: string[] = [];

    for (const id of ids) {
      const raw = await this.redis.get(`${REDIS_DELIVERY_PREFIX}${id}`);
      if (!raw) continue;
      const rec = JSON.parse(raw) as DeliveryRecord;
      const completedAt = rec.completedAt ?? rec.createdAt;
      const ttl =
        rec.status === "pending" ? this.config.pendingTtlMs : this.config.deliveredTtlMs;

      if (now - completedAt > ttl) {
        expired.push(id);
      }
    }

    for (const id of expired) {
      await this.redis.del(`${REDIS_DELIVERY_PREFIX}${id}`);
      await this.redis.srem(`${REDIS_DELIVERY_INDEX_PREFIX}${webhookId}`, id);
    }
  }
}

/**
 * Build the appropriate webhook delivery service based on whether Redis is configured.
 * Mirrors buildWebhookRegistry pattern.
 */
export function buildWebhookDeliveryService(redisUrl: string | undefined): WebhookDeliveryService | RedisWebhookDeliveryService {
  if (redisUrl) {
    // Lazily import ioredis so it is not required in local/test environments.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { Redis } = require("ioredis") as typeof import("ioredis");
    const redis = new Redis(redisUrl, { lazyConnect: false });
    return new RedisWebhookDeliveryService(redis);
  }
  return new WebhookDeliveryService();
}
