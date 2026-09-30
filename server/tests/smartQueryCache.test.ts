import { describe, expect, it, vi, beforeEach } from "vitest";
import { SmartQueryCache } from "../src/db/smartQueryCache.js";

describe("SmartQueryCache (#1770)", () => {
  let cache: SmartQueryCache;

  beforeEach(() => {
    cache = new SmartQueryCache({ defaultTtlMs: 5000, maxEntries: 10 });
  });

  describe("Query Result Caching", () => {
    it("stores and retrieves cached query results", () => {
      cache.set("query:users:1", { id: "1", name: "Alice" });
      const result = cache.get<{ id: string; name: string }>("query:users:1");
      expect(result).toEqual({ id: "1", name: "Alice" });
    });

    it("returns undefined for cache misses", () => {
      const result = cache.get("non-existent");
      expect(result).toBeUndefined();
    });

    it("transparently caches query execution via wrap()", async () => {
      let dbCalls = 0;
      const queryFn = async () => {
        dbCalls++;
        return { count: 42 };
      };

      const res1 = await cache.wrap("query:stats", queryFn);
      const res2 = await cache.wrap("query:stats", queryFn);

      expect(res1).toEqual({ count: 42 });
      expect(res2).toEqual({ count: 42 });
      expect(dbCalls).toBe(1); // Only called once, second was cached
    });

    it("deduplicates concurrent in-flight requests (prevents thundering herd)", async () => {
      let dbCalls = 0;
      const slowQuery = async () => {
        dbCalls++;
        await new Promise((resolve) => setTimeout(resolve, 50));
        return "data";
      };

      // Launch 5 simultaneous requests
      const promises = Array.from({ length: 5 }, () =>
        cache.wrap("query:heavy", slowQuery),
      );
      const results = await Promise.all(promises);

      expect(results).toEqual(["data", "data", "data", "data", "data"]);
      expect(dbCalls).toBe(1); // Only a single database execution occurred
    });
  });

  describe("Cache Invalidation Logic", () => {
    it("invalidates by specific key", () => {
      cache.set("key1", "val1");
      expect(cache.get("key1")).toBe("val1");

      const invalidated = cache.invalidateKey("key1");
      expect(invalidated).toBe(true);
      expect(cache.get("key1")).toBeUndefined();
    });

    it("invalidates by tag (e.g. table / entity invalidation)", () => {
      cache.set("q1", "result1", { tags: ["loans", "borrower:100"] });
      cache.set("q2", "result2", { tags: ["loans", "borrower:200"] });
      cache.set("q3", "result3", { tags: ["users"] });

      // Invalidate borrower:100
      const count = cache.invalidateTag("borrower:100");
      expect(count).toBe(1);
      expect(cache.get("q1")).toBeUndefined();
      expect(cache.get("q2")).toBe("result2");
      expect(cache.get("q3")).toBe("result3");

      // Invalidate all loans
      const loanCount = cache.invalidateTag("loans");
      expect(loanCount).toBe(1); // Only q2 remaining with tag loans
      expect(cache.get("q2")).toBeUndefined();
      expect(cache.get("q3")).toBe("result3");
    });

    it("invalidates by pattern using wildcards", () => {
      cache.set("events:block:100", "e100");
      cache.set("events:block:101", "e101");
      cache.set("events:tx:5", "tx5");
      cache.set("other:key", "val");

      const removed = cache.invalidatePattern("events:block:*");
      expect(removed).toBe(2);
      expect(cache.get("events:block:100")).toBeUndefined();
      expect(cache.get("events:block:101")).toBeUndefined();
      expect(cache.get("events:tx:5")).toBe("tx5");
      expect(cache.get("other:key")).toBe("val");
    });

    it("expires entries after TTL", async () => {
      cache.set("short-lived", "temp", { ttlMs: 30 });
      expect(cache.get("short-lived")).toBe("temp");

      await new Promise((resolve) => setTimeout(resolve, 45));
      expect(cache.get("short-lived")).toBeUndefined();
    });

    it("evicts oldest entries when max capacity is reached", () => {
      const smallCache = new SmartQueryCache({ maxEntries: 2 });
      smallCache.set("k1", "v1");
      smallCache.set("k2", "v2");
      smallCache.set("k3", "v3"); // triggers LRU eviction

      expect(smallCache.getMetrics().evictions).toBe(1);
      expect(smallCache.get("k1")).toBeUndefined();
      expect(smallCache.get("k2")).toBe("v2");
      expect(smallCache.get("k3")).toBe("v3");
    });
  });

  describe("Cache Warming", () => {
    it("warms cache in batch", async () => {
      const queries = [
        { key: "warm:1", queryFn: async () => 100 },
        { key: "warm:2", queryFn: async () => 200 },
      ];

      const results = await cache.warm(queries);
      expect(results.get("warm:1")).toBe(true);
      expect(results.get("warm:2")).toBe(true);

      expect(cache.get("warm:1")).toBe(100);
      expect(cache.get("warm:2")).toBe(200);
    });

    it("executes registered warm tasks on demand", async () => {
      cache.registerWarmTask({
        key: "task:active_loans",
        queryFn: async () => [{ loanId: 1 }],
      });

      const results = await cache.executeWarmTasks();
      expect(results.get("task:active_loans")).toBe(true);
      expect(cache.get("task:active_loans")).toEqual([{ loanId: 1 }]);
    });
  });

  describe("Hit Rate & Metrics Tracking", () => {
    it("tracks hits, misses, and calculates hit rate ratio", () => {
      cache.set("k", "v");

      cache.get("k"); // Hit 1
      cache.get("k"); // Hit 2
      cache.get("k"); // Hit 3
      cache.get("miss1"); // Miss 1
      cache.get("miss2"); // Miss 2

      const metrics = cache.getMetrics();
      expect(metrics.hits).toBe(3);
      expect(metrics.misses).toBe(2);
      expect(metrics.hitRate).toBe(0.6); // 3 / (3 + 2)
      expect(metrics.activeEntries).toBe(1);
    });

    it("resets metrics counters", () => {
      cache.set("k", "v");
      cache.get("k");
      expect(cache.getMetrics().hits).toBe(1);

      cache.resetMetrics();
      expect(cache.getMetrics().hits).toBe(0);
      expect(cache.getMetrics().misses).toBe(0);
      expect(cache.getMetrics().hitRate).toBe(0);
    });

    it("exports Prometheus formatted metrics string", () => {
      cache.set("item", "data");
      cache.get("item"); // hit

      const prom = cache.exportPrometheusMetrics();
      expect(prom).toContain("query_cache_hits_total 1");
      expect(prom).toContain("query_cache_misses_total 0");
      expect(prom).toContain("query_cache_hit_rate 1");
      expect(prom).toContain("query_cache_active_entries 1");
    });
  });
});
