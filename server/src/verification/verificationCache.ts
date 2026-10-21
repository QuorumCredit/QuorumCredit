/**
 * Credential verification result cache (issue #1773).
 *
 * Caches verification outcomes so repeated checks for the same credential do not
 * re-run the full verification path. Behaviour:
 * - entries expire after a configurable TTL (default 60 s, `VERIFICATION_CACHE_TTL_MS`);
 * - entries carry tags (e.g. the credential ID), so a change to a credential
 *   invalidates every cached result that depends on it in one call;
 * - the cache is bounded (LRU eviction), so memory cannot grow without limit;
 * - hit, miss, expiry and eviction counters are exposed to track effectiveness.
 */

export const DEFAULT_VERIFICATION_CACHE_TTL_MS = 60_000;
export const DEFAULT_VERIFICATION_CACHE_MAX_ENTRIES = 10_000;

export interface VerificationCacheOptions {
  ttlMs?: number;
  maxEntries?: number;
  now?: () => number;
}

export interface VerificationCacheStats {
  hits: number;
  misses: number;
  expirations: number;
  evictions: number;
  size: number;
  hitRate: number;
}

interface CacheEntry<V> {
  value: V;
  expiresAt: number;
  tags: string[];
}

export class VerificationCache<V> {
  private readonly entries = new Map<string, CacheEntry<V>>();
  private readonly tagIndex = new Map<string, Set<string>>();
  private readonly ttlMs: number;
  private readonly maxEntries: number;
  private readonly now: () => number;
  private hits = 0;
  private misses = 0;
  private expirations = 0;
  private evictions = 0;

  constructor(options: VerificationCacheOptions = {}) {
    const ttlMs = options.ttlMs ?? DEFAULT_VERIFICATION_CACHE_TTL_MS;
    const maxEntries = options.maxEntries ?? DEFAULT_VERIFICATION_CACHE_MAX_ENTRIES;
    if (!Number.isFinite(ttlMs) || ttlMs <= 0) throw new Error("ttlMs must be a positive number");
    if (!Number.isInteger(maxEntries) || maxEntries < 1) {
      throw new Error("maxEntries must be a positive integer");
    }
    this.ttlMs = ttlMs;
    this.maxEntries = maxEntries;
    this.now = options.now ?? Date.now;
  }

  /** Returns the cached value, or `undefined` on a miss or expired entry. */
  get(key: string): V | undefined {
    const entry = this.entries.get(key);
    if (!entry) {
      this.misses += 1;
      return undefined;
    }
    if (this.now() >= entry.expiresAt) {
      this.removeEntry(key);
      this.expirations += 1;
      this.misses += 1;
      return undefined;
    }
    this.hits += 1;
    // Re-insert to mark as most recently used (Map preserves insertion order).
    this.entries.delete(key);
    this.entries.set(key, entry);
    return entry.value;
  }

  set(key: string, value: V, tags: string[] = []): void {
    if (this.entries.has(key)) this.removeEntry(key);
    this.entries.set(key, { value, expiresAt: this.now() + this.ttlMs, tags: [...tags] });
    for (const tag of tags) {
      let keys = this.tagIndex.get(tag);
      if (!keys) {
        keys = new Set();
        this.tagIndex.set(tag, keys);
      }
      keys.add(key);
    }
    this.evictIfNeeded();
  }

  /** Return the cached value, or compute, cache and return it. */
  async getOrCompute(key: string, compute: () => Promise<V>, tags: string[] = []): Promise<V> {
    const cached = this.get(key);
    if (cached !== undefined) return cached;
    const value = await compute();
    this.set(key, value, tags);
    return value;
  }

  invalidate(key: string): boolean {
    return this.removeEntry(key);
  }

  /** Invalidate every entry carrying `tag` (e.g. a credential ID). Returns the count removed. */
  invalidateTag(tag: string): number {
    const keys = this.tagIndex.get(tag);
    if (!keys) return 0;
    let removed = 0;
    for (const key of Array.from(keys)) {
      if (this.removeEntry(key)) removed += 1;
    }
    return removed;
  }

  stats(): VerificationCacheStats {
    const lookups = this.hits + this.misses;
    return {
      hits: this.hits,
      misses: this.misses,
      expirations: this.expirations,
      evictions: this.evictions,
      size: this.entries.size,
      hitRate: lookups === 0 ? 0 : Math.round((this.hits / lookups) * 10_000) / 10_000,
    };
  }

  private evictIfNeeded(): void {
    while (this.entries.size > this.maxEntries) {
      const oldest = this.entries.keys().next().value;
      if (oldest === undefined) break;
      this.removeEntry(oldest);
      this.evictions += 1;
    }
  }

  private removeEntry(key: string): boolean {
    const entry = this.entries.get(key);
    if (!entry) return false;
    this.entries.delete(key);
    for (const tag of entry.tags) {
      const keys = this.tagIndex.get(tag);
      if (!keys) continue;
      keys.delete(key);
      if (keys.size === 0) this.tagIndex.delete(tag);
    }
    return true;
  }
}

/** Read the TTL from `VERIFICATION_CACHE_TTL_MS`, falling back to the default when unset or invalid. */
export function verificationCacheTtlFromEnv(env: NodeJS.ProcessEnv = process.env): number {
  const raw = env.VERIFICATION_CACHE_TTL_MS;
  if (raw === undefined || raw.trim() === "") return DEFAULT_VERIFICATION_CACHE_TTL_MS;
  const parsed = Number(raw);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : DEFAULT_VERIFICATION_CACHE_TTL_MS;
}
