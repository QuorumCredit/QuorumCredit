import { describe, it, expect, beforeEach, afterEach } from "vitest";
import {
  LocalRecurringPaymentStore,
  RedisRecurringPaymentStore,
  type RecurringPaymentStore,
} from "../src/recurring/recurringPaymentStore.js";

/**
 * Shared test suite for RecurringPaymentStore implementations.
 * Issue #1362: Tests for persistent, multi-instance-safe scheduling.
 */

function createTestSuite(
  name: string,
  createStore: () => Promise<RecurringPaymentStore>,
  cleanup?: (store: RecurringPaymentStore) => Promise<void>
) {
  describe(name, () => {
    let store: RecurringPaymentStore;

    beforeEach(async () => {
      store = await createStore();
    });

    afterEach(async () => {
      if (cleanup) {
        await cleanup(store);
      } else {
        await store.close();
      }
    });

    it("should setup a recurring payment schedule", async () => {
      const loanId = "loan-1";
      const schedule = await store.setup(loanId, 1000, 86400, Date.now());

      expect(schedule.loanId).toBe(loanId);
      expect(schedule.amount).toBe(1000);
      expect(schedule.frequencySeconds).toBe(86400);
      expect(schedule.active).toBe(true);
      expect(schedule.successCount).toBe(0);
      expect(schedule.failureCount).toBe(0);
    });

    it("should retrieve a setup schedule", async () => {
      const loanId = "loan-2";
      await store.setup(loanId, 2000, 3600, Date.now());
      const retrieved = await store.get(loanId);

      expect(retrieved).toBeDefined();
      expect(retrieved?.loanId).toBe(loanId);
      expect(retrieved?.amount).toBe(2000);
    });

    it("should return undefined for non-existent schedules", async () => {
      const schedule = await store.get("loan-nonexistent");
      expect(schedule).toBeUndefined();
    });

    it("should terminate a schedule", async () => {
      const loanId = "loan-3";
      await store.setup(loanId, 1000, 3600, Date.now());
      const terminated = await store.terminate(loanId);

      expect(terminated).toBe(true);
      const retrieved = await store.get(loanId);
      expect(retrieved?.active).toBe(false);
    });

    it("should return false when terminating non-existent schedule", async () => {
      const terminated = await store.terminate("loan-nonexistent");
      expect(terminated).toBe(false);
    });

    it("should calculate success rate in basis points", async () => {
      const loanId = "loan-4";
      const schedule = await store.setup(loanId, 1000, 3600, Date.now());

      // Before any attempts: 0%
      let rate = await store.successRateBps(loanId);
      expect(rate).toBe(0);

      // Simulate successful attempt
      schedule.successCount = 1;
      schedule.successCount += 1;
      // Should be ~5000 bps (50%)
      rate = await store.successRateBps(loanId);
      expect(rate).toBeGreaterThan(0);
    });

    it("should execute with retry and succeed on first try", async () => {
      const loanId = "loan-5";
      const startDate = Date.now();
      const schedule = await store.setup(loanId, 1000, 3600, startDate);

      let callCount = 0;
      const result = await store.executeWithRetry(
        loanId,
        async () => {
          callCount++;
          return true; // Success on first try
        },
        () => {
          throw new Error("Should not notify borrower on success");
        }
      );

      expect(result.ok).toBe(true);
      expect(result.retriesUsed).toBe(0);
      expect(result.notifiedBorrower).toBe(false);
      expect(callCount).toBe(1);

      // Check schedule was updated
      const updated = await store.get(loanId);
      expect(updated?.successCount).toBe(1);
      expect(updated?.nextPaymentDue).toBe(startDate + 3600);
    });

    it("should execute with retry and fail after max retries", async () => {
      const loanId = "loan-6";
      const schedule = await store.setup(loanId, 1000, 3600, Date.now());

      let callCount = 0;
      let borrowerNotified = false;

      const result = await store.executeWithRetry(
        loanId,
        async () => {
          callCount++;
          return false; // Always fail
        },
        () => {
          borrowerNotified = true;
        }
      );

      expect(result.ok).toBe(false);
      expect(result.retriesUsed).toBe(3); // 3 retries after initial failure
      expect(result.notifiedBorrower).toBe(true);
      expect(borrowerNotified).toBe(true);
      expect(callCount).toBe(4); // initial + 3 retries

      // Check schedule was updated with failure
      const updated = await store.get(loanId);
      expect(updated?.failureCount).toBe(1);
      expect(updated?.retryCount).toBe(3);
    });

    it("should skip execution if schedule is inactive", async () => {
      const loanId = "loan-7";
      const schedule = await store.setup(loanId, 1000, 3600, Date.now());
      await store.terminate(loanId);

      let transferCalled = false;
      const result = await store.executeWithRetry(
        loanId,
        async () => {
          transferCalled = true;
          return true;
        },
        () => {
          throw new Error("Should not notify");
        }
      );

      expect(result.ok).toBe(false);
      expect(transferCalled).toBe(false);
    });

    it("should skip execution if schedule does not exist", async () => {
      let transferCalled = false;
      const result = await store.executeWithRetry(
        "loan-nonexistent",
        async () => {
          transferCalled = true;
          return true;
        },
        () => {
          throw new Error("Should not notify");
        }
      );

      expect(result.ok).toBe(false);
      expect(transferCalled).toBe(false);
    });

    it("should prevent duplicate submissions via idempotency key (same period)", async () => {
      const loanId = "loan-8";
      const startDate = Date.now();
      const schedule = await store.setup(loanId, 1000, 3600, startDate);

      let callCount = 0;

      // Simulate a successful execution
      const result1 = await store.executeWithRetry(
        loanId,
        async () => {
          callCount++;
          return true;
        },
        () => {
          throw new Error("Should not notify");
        }
      );

      expect(result1.ok).toBe(true);
      expect(callCount).toBe(1);

      // Now manually reset nextPaymentDue to simulate a restart before the
      // next period. This tests the real scenario: keeper executed successfully,
      // but crashed before the schedule was persisted. On restart, it loads the
      // old schedule and should not resubmit.
      schedule.nextPaymentDue = startDate; // Revert to original period
      schedule.successCount = 0; // Also revert success count to simulate no persistence

      // Try to execute the same period again — should skip due to idempotency
      const result2 = await store.executeWithRetry(
        loanId,
        async () => {
          callCount++;
          return true;
        },
        () => {
          throw new Error("Should not notify");
        }
      );

      // Should claim success but not actually call transfer (idempotency)
      expect(result2.ok).toBe(true);
      expect(result2.retriesUsed).toBe(0); // No retries because it was skipped
      expect(callCount).toBe(1); // Still only 1 call total
    });
  });
}

// Test LocalRecurringPaymentStore
createTestSuite("LocalRecurringPaymentStore", async () => new LocalRecurringPaymentStore());

// Test RedisRecurringPaymentStore (only if Redis is available)
const REDIS_URL = process.env.REDIS_URL;
if (REDIS_URL) {
  createTestSuite(
    "RedisRecurringPaymentStore",
    async () => {
      // Lazily import ioredis so it's only required if this test runs
      // eslint-disable-next-line @typescript-eslint/no-require-imports
      const { Redis } = require("ioredis") as typeof import("ioredis");
      const redis = new Redis(REDIS_URL, { lazyConnect: false });
      return new RedisRecurringPaymentStore(redis);
    },
    async (store) => {
      // Cleanup Redis keys for this test
      if (store instanceof RedisRecurringPaymentStore) {
        const redis = (store as any).redis;
        await redis.flushdb();
      }
      await store.close();
    }
  );
}
