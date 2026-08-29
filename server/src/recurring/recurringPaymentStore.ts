/**
 * Issue #1362: Persistent, multi-instance-safe recurring payment store.
 *
 * Follows the same Local/Redis split used in jtiRevocationStore:
 * - Single-instance / local dev → `LocalRecurringPaymentStore` (in-process Map).
 * - Multi-instance / production  → `RedisRecurringPaymentStore` (Redis hashes + idempotency tracking).
 *
 * Stores schedule state (nextPaymentDue, retryCount, etc.) so a process restart
 * or instance failover does not lose progress. Idempotency keys prevent double-submission
 * if execution is interrupted mid-retry.
 */

import type { Redis } from "ioredis";

export interface RecurringPaymentSchedule {
  loanId: string;
  amount: number;
  frequencySeconds: number;
  startDate: number;
  nextPaymentDue: number;
  active: boolean;
  successCount: number;
  failureCount: number;
  retryCount: number;
  createdAt: number;
}

export interface RecurringPaymentAttemptResult {
  ok: boolean;
  retriesUsed: number;
  notifiedBorrower: boolean;
}

/** Number of retry attempts after an initial failed transfer, before the
 * schedule gives up on that period and notifies the borrower. */
const MAX_RETRIES = 3;

export interface RecurringPaymentStore {
  setup(
    loanId: string,
    amount: number,
    frequencySeconds: number,
    startDate: number
  ): Promise<RecurringPaymentSchedule>;
  get(loanId: string): Promise<RecurringPaymentSchedule | undefined>;
  terminate(loanId: string): Promise<boolean>;
  successRateBps(loanId: string): Promise<number>;
  executeWithRetry(
    loanId: string,
    transfer: () => Promise<boolean>,
    notifyBorrower: (loanId: string, schedule: RecurringPaymentSchedule) => void
  ): Promise<RecurringPaymentAttemptResult>;
  close(): Promise<void>;
}

// ── Local (single-instance) implementation ─────────────────────────────────────

/**
 * In-process recurring-payment store backed by a plain Map.
 * NOT multi-instance-safe — for local dev and unit tests only.
 */
export class LocalRecurringPaymentStore implements RecurringPaymentStore {
  private readonly byLoan = new Map<string, RecurringPaymentSchedule>();
  /** Track recently-submitted idempotency keys to prevent double-submission during replay. */
  private readonly recentlySubmitted = new Map<string, number>();
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  private readonly recentlySubmittedTtlMs = 60_000; // 1 minute cleanup interval (for future cleanup logic)

  async setup(
    loanId: string,
    amount: number,
    frequencySeconds: number,
    startDate: number
  ): Promise<RecurringPaymentSchedule> {
    const schedule: RecurringPaymentSchedule = {
      loanId,
      amount,
      frequencySeconds,
      startDate,
      nextPaymentDue: startDate,
      active: true,
      successCount: 0,
      failureCount: 0,
      retryCount: 0,
      createdAt: Date.now(),
    };
    this.byLoan.set(loanId, schedule);
    return schedule;
  }

  async get(loanId: string): Promise<RecurringPaymentSchedule | undefined> {
    return this.byLoan.get(loanId);
  }

  async terminate(loanId: string): Promise<boolean> {
    const schedule = this.byLoan.get(loanId);
    if (!schedule) return false;
    schedule.active = false;
    return true;
  }

  async successRateBps(loanId: string): Promise<number> {
    const schedule = this.byLoan.get(loanId);
    if (!schedule) return 0;
    const attempts = schedule.successCount + schedule.failureCount;
    return attempts === 0 ? 0 : Math.round((schedule.successCount / attempts) * 10_000);
  }

  async executeWithRetry(
    loanId: string,
    transfer: () => Promise<boolean>,
    notifyBorrower: (loanId: string, schedule: RecurringPaymentSchedule) => void
  ): Promise<RecurringPaymentAttemptResult> {
    const schedule = this.byLoan.get(loanId);
    if (!schedule || !schedule.active) {
      return { ok: false, retriesUsed: 0, notifiedBorrower: false };
    }

    // Compute the idempotency key for THIS execution attempt (based on current nextPaymentDue).
    const key = this.computeIdempotencyKey(loanId, schedule.nextPaymentDue);

    // Check if this payment was recently submitted (idempotency).
    if (this.recentlySubmitted.has(key)) {
      console.warn(
        `[quorum-credit] skipping duplicate submission for loan=${loanId} (idempotency key=${key})`
      );
      return { ok: true, retriesUsed: 0, notifiedBorrower: false };
    }

    let retriesUsed = 0;
    for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
      retriesUsed = attempt;
      const ok = await transfer();
      if (ok) {
        schedule.successCount += 1;
        schedule.retryCount = 0;
        schedule.nextPaymentDue += schedule.frequencySeconds;
        // Record successful submission to prevent replay of THIS period.
        this.recentlySubmitted.set(key, Date.now());
        return { ok: true, retriesUsed, notifiedBorrower: false };
      }
    }

    schedule.retryCount = retriesUsed;
    schedule.failureCount += 1;
    notifyBorrower(loanId, schedule);
    return { ok: false, retriesUsed, notifiedBorrower: true };
  }

  async close(): Promise<void> {
    this.byLoan.clear();
    this.recentlySubmitted.clear();
  }

  private computeIdempotencyKey(loanId: string, periodTimestamp: number): string {
    // Hash of (loanId + period) to detect retries for the same period.
    return `${loanId}:${periodTimestamp}`;
  }
}

// ── Redis (multi-instance) implementation ──────────────────────────────────────

const REDIS_SCHEDULE_PREFIX = "qc:recurring:schedule:";
const REDIS_SUBMITTED_PREFIX = "qc:recurring:submitted:";

/**
 * Redis-backed recurring-payment store for multi-instance deployments.
 *
 * Each schedule is stored as a Redis hash:
 *   `qc:recurring:schedule:{loanId}` → {loanId, amount, frequencySeconds, ...}
 *
 * Idempotency is tracked via Redis sets with TTL:
 *   `qc:recurring:submitted:{idempotencyKey}` → presence in set (with TTL)
 */
export class RedisRecurringPaymentStore implements RecurringPaymentStore {
  constructor(private readonly redis: Redis) {}

  async setup(
    loanId: string,
    amount: number,
    frequencySeconds: number,
    startDate: number
  ): Promise<RecurringPaymentSchedule> {
    const schedule: RecurringPaymentSchedule = {
      loanId,
      amount,
      frequencySeconds,
      startDate,
      nextPaymentDue: startDate,
      active: true,
      successCount: 0,
      failureCount: 0,
      retryCount: 0,
      createdAt: Date.now(),
    };

    const key = `${REDIS_SCHEDULE_PREFIX}${loanId}`;
    const payload = JSON.stringify(schedule);
    await this.redis.set(key, payload);
    return schedule;
  }

  async get(loanId: string): Promise<RecurringPaymentSchedule | undefined> {
    const key = `${REDIS_SCHEDULE_PREFIX}${loanId}`;
    const payload = await this.redis.get(key);
    if (!payload) return undefined;
    try {
      return JSON.parse(payload);
    } catch {
      console.error(`[quorum-credit] failed to parse schedule for ${loanId}`);
      return undefined;
    }
  }

  async terminate(loanId: string): Promise<boolean> {
    const schedule = await this.get(loanId);
    if (!schedule) return false;
    schedule.active = false;
    const key = `${REDIS_SCHEDULE_PREFIX}${loanId}`;
    const payload = JSON.stringify(schedule);
    await this.redis.set(key, payload);
    return true;
  }

  async successRateBps(loanId: string): Promise<number> {
    const schedule = await this.get(loanId);
    if (!schedule) return 0;
    const attempts = schedule.successCount + schedule.failureCount;
    return attempts === 0 ? 0 : Math.round((schedule.successCount / attempts) * 10_000);
  }

  async executeWithRetry(
    loanId: string,
    transfer: () => Promise<boolean>,
    notifyBorrower: (loanId: string, schedule: RecurringPaymentSchedule) => void
  ): Promise<RecurringPaymentAttemptResult> {
    const schedule = await this.get(loanId);
    if (!schedule || !schedule.active) {
      return { ok: false, retriesUsed: 0, notifiedBorrower: false };
    }

    // Compute the idempotency key for THIS execution attempt (based on current nextPaymentDue).
    const key = this.computeIdempotencyKey(loanId, schedule.nextPaymentDue);

    // Check if this payment was recently submitted (idempotency).
    if (await this.isRecentlySubmitted(key)) {
      console.warn(
        `[quorum-credit] skipping duplicate submission for loan=${loanId} (idempotency key=${key})`
      );
      return { ok: true, retriesUsed: 0, notifiedBorrower: false };
    }

    let retriesUsed = 0;
    for (let attempt = 0; attempt <= MAX_RETRIES; attempt++) {
      retriesUsed = attempt;
      const ok = await transfer();
      if (ok) {
        schedule.successCount += 1;
        schedule.retryCount = 0;
        schedule.nextPaymentDue += schedule.frequencySeconds;
        // Persist schedule update.
        const scheduleKey = `${REDIS_SCHEDULE_PREFIX}${loanId}`;
        const payload = JSON.stringify(schedule);
        await this.redis.set(scheduleKey, payload);
        // Record successful submission to prevent replay of THIS period.
        await this.recordSubmitted(key);
        return { ok: true, retriesUsed, notifiedBorrower: false };
      }
    }

    schedule.retryCount = retriesUsed;
    schedule.failureCount += 1;
    // Persist failure state.
    const scheduleKey = `${REDIS_SCHEDULE_PREFIX}${loanId}`;
    const payload = JSON.stringify(schedule);
    await this.redis.set(scheduleKey, payload);
    notifyBorrower(loanId, schedule);
    return { ok: false, retriesUsed, notifiedBorrower: true };
  }

  async close(): Promise<void> {
    await this.redis.quit();
  }

  private async recordSubmitted(idempotencyKey: string): Promise<void> {
    const key = `${REDIS_SUBMITTED_PREFIX}${idempotencyKey}`;
    const ttlSeconds = 60; // 1 minute; enough to survive a process restart
    await this.redis.set(key, "1", "EX", ttlSeconds);
  }

  private async isRecentlySubmitted(idempotencyKey: string): Promise<boolean> {
    const key = `${REDIS_SUBMITTED_PREFIX}${idempotencyKey}`;
    const exists = await this.redis.exists(key);
    return exists === 1;
  }

  private computeIdempotencyKey(loanId: string, periodTimestamp: number): string {
    return `${loanId}:${periodTimestamp}`;
  }
}

// ── Factory ────────────────────────────────────────────────────────────────

/**
 * Build the appropriate store based on whether Redis is configured.
 * Mirrors the `buildBus` pattern in `server/src/index.ts`.
 */
export function buildRecurringPaymentStore(redisUrl: string | undefined): RecurringPaymentStore {
  if (redisUrl) {
    // Lazily import ioredis so it is not required in local/test environments.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { Redis } = require("ioredis") as typeof import("ioredis");
    const redis = new Redis(redisUrl, { lazyConnect: false });
    return new RedisRecurringPaymentStore(redis);
  }
  return new LocalRecurringPaymentStore();
}

// Default singleton for backward compatibility during transition.
export const recurringPaymentStore: RecurringPaymentStore = new LocalRecurringPaymentStore();
