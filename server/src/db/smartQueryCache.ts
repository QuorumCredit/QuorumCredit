/**
 * Issue #1770 — Implement Smart Caching Strategy for Queries
 *
 * Implements a smart, high-performance query caching engine with:
 * - Query result caching with request deduplication (anti-thundering herd)
 * - Tag-based, pattern-based, and key-based cache invalidation logic
 * - Cache warming capabilities (batch pre-warming, scheduled tasks, stale-while-revalidate)
 * - Hit rate and operational metrics tracking with Prometheus-compatible export
 */

export interface CacheEntry<T = unknown> {
  key: string;
  data: T;
  createdAt: number;
  expiresAt: number;
  tags: Set<string>;
  cost: number;
  lastAccessed: number;
  hits: number;
}

export interface CacheSetOptions {
  ttlMs?: number;
  tags?: string[];
  cost?: number;
}

export interface CacheWrapOptions extends CacheSetOptions {
  staleWhileRevalidate?: boolean;
}

export interface WarmTask<T = unknown> {
  key: string;
  queryFn: () => Promise<T>;
  options?: CacheWrapOptions;
}

export interface CacheMetrics {
  hits: number;
  misses: number;
  hitRate: number;
  sets: number;
  invalidations: number;
  evictions: number;
  activeEntries: number;
  totalMemoryBytesEstimate: number;
}

export interface SmartQueryCacheConfig {
  defaultTtlMs?: number;
  maxEntries?: number;
  cleanupIntervalMs?: number;
}

export class SmartQueryCache {
  private readonly entries = new Map<string, CacheEntry<unknown>>();
  private readonly tagToKeys = new Map<string, Set<string>>();
  private readonly inFlightQueries = new Map<string, Promise<unknown>>();
  private readonly warmTasks = new Map<string, WarmTask<unknown>>();

  private readonly defaultTtlMs: number;
  private readonly maxEntries: number;
  private cleanupTimer: NodeJS.Timeout | null = null;

  private hits = 0;
  private misses = 0;
  private sets = 0;
  private invalidations = 0;
  private evictions = 0;

  constructor(config: SmartQueryCacheConfig = {}) {
    this.defaultTtlMs = config.defaultTtlMs ?? 60_000; // 1 minute default
    this.maxEntries = config.maxEntries ?? 10_000;

    if (config.cleanupIntervalMs && config.cleanupIntervalMs > 0) {
      this.cleanupTimer = setInterval(() => {
        this.evictExpired();
      }, config.cleanupIntervalMs);
      if (this.cleanupTimer.unref) {
        this.cleanupTimer.unref();
      }
    }
  }

  /**
   * Stop background timers.
   */
  public destroy(): void {
    if (this.cleanupTimer) {
      clearInterval(this.cleanupTimer);
      this.cleanupTimer = null;
    }
  }

  /**
   * Get a cached entry if present and not expired.
   */
  public get<T>(key: string): T | undefined {
    const entry = this.entries.get(key) as CacheEntry<T> | undefined;
    const now = Date.now();

    if (!entry) {
      this.misses++;
      return undefined;
    }

    if (now > entry.expiresAt) {
      this.misses++;
      this.deleteEntry(key);
      return undefined;
    }

    this.hits++;
    entry.lastAccessed = now;
    entry.hits++;
    return entry.data;
  }

  /**
   * Store a query result in cache.
   */
  public set<T>(key: string, data: T, options: CacheSetOptions = {}): void {
    const now = Date.now();
    const ttlMs = options.ttlMs ?? this.defaultTtlMs;
    const expiresAt = now + ttlMs;
    const tags = new Set(options.tags ?? []);
    const cost = options.cost ?? 1;

    // Remove existing entry tag mappings
    if (this.entries.has(key)) {
      this.deleteEntry(key);
    } else if (this.entries.size >= this.maxEntries) {
      this.evictLru();
    }

    const entry: CacheEntry<unknown> = {
      key,
      data,
      createdAt: now,
      expiresAt,
      tags,
      cost,
      lastAccessed: now,
      hits: 0,
    };

    this.entries.set(key, entry);

    for (const tag of tags) {
      let keySet = this.tagToKeys.get(tag);
      if (!keySet) {
        keySet = new Set();
        this.tagToKeys.set(tag, keySet);
      }
      keySet.add(key);
    }

    this.sets++;
  }

  /**
   * Query result caching wrapper.
   * Transparently returns cached data or executes the query function.
   * Includes promise deduplication to prevent thundering herd / cache stampede.
   */
  public async wrap<T>(
    key: string,
    queryFn: () => Promise<T>,
    options: CacheWrapOptions = {},
  ): Promise<T> {
    const cached = this.get<T>(key);
    if (cached !== undefined) {
      return cached;
    }

    // Check if stale-while-revalidate applies
    if (options.staleWhileRevalidate) {
      const expiredEntry = this.entries.get(key) as CacheEntry<T> | undefined;
      if (expiredEntry) {
        // Return stale result immediately and revalidate asynchronously in background
        this.revalidateInBackground(key, queryFn, options);
        return expiredEntry.data;
      }
    }

    // Check if an in-flight query already exists for this key
    const inFlight = this.inFlightQueries.get(key) as Promise<T> | undefined;
    if (inFlight) {
      return inFlight;
    }

    const queryPromise = (async () => {
      try {
        const result = await queryFn();
        this.set(key, result, options);
        return result;
      } finally {
        this.inFlightQueries.delete(key);
      }
    })();

    this.inFlightQueries.set(key, queryPromise);
    return queryPromise;
  }

  private revalidateInBackground<T>(
    key: string,
    queryFn: () => Promise<T>,
    options: CacheWrapOptions,
  ): void {
    if (this.inFlightQueries.has(key)) return;

    const promise = queryFn()
      .then((fresh) => {
        this.set(key, fresh, options);
        return fresh;
      })
      .catch((err) => {
        // Silently log or ignore background revalidation errors
        return err;
      })
      .finally(() => {
        this.inFlightQueries.delete(key);
      });

    this.inFlightQueries.set(key, promise);
  }

  // ---------------------------------------------------------------------------
  // Invalidation Logic
  // ---------------------------------------------------------------------------

  /**
   * Invalidate a single cache entry by key.
   */
  public invalidateKey(key: string): boolean {
    if (this.entries.has(key)) {
      this.deleteEntry(key);
      this.invalidations++;
      return true;
    }
    return false;
  }

  /**
   * Invalidate all cache entries associated with a specific tag (e.g. table name, entity ID).
   */
  public invalidateTag(tag: string): number {
    const keys = this.tagToKeys.get(tag);
    if (!keys || keys.size === 0) {
      return 0;
    }

    let count = 0;
    // Copy set to avoid mutation during iteration
    for (const key of Array.from(keys)) {
      if (this.deleteEntry(key)) {
        count++;
      }
    }
    this.tagToKeys.delete(tag);
    this.invalidations += count;
    return count;
  }

  /**
   * Invalidate all cache entries matching a regex or wildcard string pattern.
   * E.g. "users:*" or "^loans:[0-9]+"
   */
  public invalidatePattern(pattern: string | RegExp): number {
    const regex =
      typeof pattern === "string"
        ? new RegExp(
            "^" +
              pattern
                .replace(/[-[\]{}()+?.,\\^$|#\s]/g, "\\$&")
                .replace(/\*/g, ".*") +
              "$",
          )
        : pattern;

    let count = 0;
    for (const key of Array.from(this.entries.keys())) {
      if (regex.test(key)) {
        if (this.deleteEntry(key)) {
          count++;
        }
      }
    }

    this.invalidations += count;
    return count;
  }

  /**
   * Clear entire cache.
   */
  public clear(): void {
    const count = this.entries.size;
    this.entries.clear();
    this.tagToKeys.clear();
    this.inFlightQueries.clear();
    this.invalidations += count;
  }

  private deleteEntry(key: string): boolean {
    const entry = this.entries.get(key);
    if (!entry) return false;

    for (const tag of entry.tags) {
      const keys = this.tagToKeys.get(tag);
      if (keys) {
        keys.delete(key);
        if (keys.size === 0) {
          this.tagToKeys.delete(tag);
        }
      }
    }

    this.entries.delete(key);
    return true;
  }

  /**
   * Evict LRU (least recently used) entries when capacity is reached.
   */
  private evictLru(): void {
    let oldestKey: string | null = null;
    let oldestAccess = Infinity;

    for (const [key, entry] of this.entries.entries()) {
      if (entry.lastAccessed < oldestAccess) {
        oldestAccess = entry.lastAccessed;
        oldestKey = key;
      }
    }

    if (oldestKey) {
      this.deleteEntry(oldestKey);
      this.evictions++;
    }
  }

  /**
   * Evict all expired entries.
   */
  public evictExpired(): number {
    const now = Date.now();
    let count = 0;

    for (const [key, entry] of this.entries.entries()) {
      if (now > entry.expiresAt) {
        this.deleteEntry(key);
        this.evictions++;
        count++;
      }
    }

    return count;
  }

  // ---------------------------------------------------------------------------
  // Cache Warming
  // ---------------------------------------------------------------------------

  /**
   * Pre-warm cache with a batch of queries.
   */
  public async warm(
    queries: Array<WarmTask<unknown>>,
  ): Promise<Map<string, boolean>> {
    const results = new Map<string, boolean>();

    await Promise.allSettled(
      queries.map(async ({ key, queryFn, options }) => {
        try {
          const data = await queryFn();
          this.set(key, data, options);
          results.set(key, true);
        } catch {
          results.set(key, false);
        }
      }),
    );

    return results;
  }

  /**
   * Register a persistent warming task to be refreshed on schedule or on demand.
   */
  public registerWarmTask<T>(task: WarmTask<T>): void {
    this.warmTasks.set(task.key, task as WarmTask<unknown>);
  }

  /**
   * Execute all registered warm tasks.
   */
  public async executeWarmTasks(): Promise<Map<string, boolean>> {
    return this.warm(Array.from(this.warmTasks.values()));
  }

  // ---------------------------------------------------------------------------
  // Metrics & Hit Rate Tracking
  // ---------------------------------------------------------------------------

  /**
   * Get operational statistics including hit rate.
   */
  public getMetrics(): CacheMetrics {
    const totalRequests = this.hits + this.misses;
    const hitRate = totalRequests === 0 ? 0 : this.hits / totalRequests;

    // Approximate memory footprint
    let totalMemoryBytesEstimate = 0;
    for (const [key, entry] of this.entries.entries()) {
      totalMemoryBytesEstimate += key.length * 2 + 128;
      if (typeof entry.data === "string") {
        totalMemoryBytesEstimate += entry.data.length * 2;
      } else if (entry.data !== null && typeof entry.data === "object") {
        totalMemoryBytesEstimate += JSON.stringify(entry.data).length * 2;
      }
    }

    return {
      hits: this.hits,
      misses: this.misses,
      hitRate: Math.round(hitRate * 10_000) / 10_000,
      sets: this.sets,
      invalidations: this.invalidations,
      evictions: this.evictions,
      activeEntries: this.entries.size,
      totalMemoryBytesEstimate,
    };
  }

  /**
   * Reset tracking counters.
   */
  public resetMetrics(): void {
    this.hits = 0;
    this.misses = 0;
    this.sets = 0;
    this.invalidations = 0;
    this.evictions = 0;
  }

  /**
   * Export metrics in Prometheus text format.
   */
  public exportPrometheusMetrics(prefix = "query_cache"): string {
    const m = this.getMetrics();
    return [
      `# HELP ${prefix}_hits_total Total number of cache hits`,
      `# TYPE ${prefix}_hits_total counter`,
      `${prefix}_hits_total ${m.hits}`,
      `# HELP ${prefix}_misses_total Total number of cache misses`,
      `# TYPE ${prefix}_misses_total counter`,
      `${prefix}_misses_total ${m.misses}`,
      `# HELP ${prefix}_hit_rate Cache hit rate ratio`,
      `# TYPE ${prefix}_hit_rate gauge`,
      `${prefix}_hit_rate ${m.hitRate}`,
      `# HELP ${prefix}_active_entries Number of active cached entries`,
      `# TYPE ${prefix}_active_entries gauge`,
      `${prefix}_active_entries ${m.activeEntries}`,
      `# HELP ${prefix}_invalidations_total Total number of cache invalidations`,
      `# TYPE ${prefix}_invalidations_total counter`,
      `${prefix}_invalidations_total ${m.invalidations}`,
      `# HELP ${prefix}_evictions_total Total number of cache evictions`,
      `# TYPE ${prefix}_evictions_total counter`,
      `${prefix}_evictions_total ${m.evictions}`,
    ].join("\n");
  }
}

export const queryCache = new SmartQueryCache();
