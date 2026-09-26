import { createHash } from "node:crypto";
import type { Redis } from "ioredis";

export type ApiTier = "free" | "pro" | "enterprise";

export interface ApiRateLimits {
  windowMs: number;
  tiers: Record<ApiTier, number>;
}

export interface ApiRateLimitResult {
  tier: ApiTier;
  limit: number;
  used: number;
  remaining: number;
  resetAt: number;
  windowMs: number;
  allowed: boolean;
}

export interface ApiRateLimiter {
  consume(identity: string, tier: ApiTier): Promise<ApiRateLimitResult>;
  close(): Promise<void>;
}

export const DEFAULT_API_RATE_LIMITS: ApiRateLimits = {
  windowMs: 60_000,
  tiers: { free: 100, pro: 1_000, enterprise: 10_000 },
};

export class LocalApiRateLimiter implements ApiRateLimiter {
  private readonly windows = new Map<
    string,
    { count: number; resetAt: number }
  >();
  private requestCount = 0;

  constructor(private readonly config: ApiRateLimits = DEFAULT_API_RATE_LIMITS) {
    validateConfig(config);
  }

  async consume(identity: string, tier: ApiTier): Promise<ApiRateLimitResult> {
    const now = Date.now();
    const limit = this.config.tiers[tier];
    const key = `${tier}:${createHash("sha256").update(identity).digest("hex")}`;
    let window = this.windows.get(key);
    if (!window || window.resetAt <= now) {
      window = { count: 0, resetAt: now + this.config.windowMs };
      this.windows.set(key, window);
    }
    window.count += 1;

    this.requestCount += 1;
    if (this.requestCount % 256 === 0) {
      for (const [entryKey, entry] of this.windows) {
        if (entry.resetAt <= now) this.windows.delete(entryKey);
      }
    }

    return {
      tier,
      limit,
      used: window.count,
      remaining: Math.max(0, limit - window.count),
      resetAt: window.resetAt,
      windowMs: this.config.windowMs,
      allowed: window.count <= limit,
    };
  }

  async close(): Promise<void> {
    this.windows.clear();
  }
}

const REDIS_API_RATE_LIMIT_SCRIPT = `
local count = redis.call('INCR', KEYS[1])
if count == 1 then redis.call('PEXPIRE', KEYS[1], ARGV[1]) end
return { count, redis.call('PTTL', KEYS[1]) }
`;

export class RedisApiRateLimiter implements ApiRateLimiter {
  constructor(
    private readonly redis: Redis,
    private readonly config: ApiRateLimits = DEFAULT_API_RATE_LIMITS
  ) {
    validateConfig(config);
  }

  async consume(identity: string, tier: ApiTier): Promise<ApiRateLimitResult> {
    const digest = createHash("sha256").update(identity).digest("hex");
    const key = `qc:api:ratelimit:${tier}:${digest}`;
    const raw = await this.redis.eval(
      REDIS_API_RATE_LIMIT_SCRIPT,
      1,
      key,
      String(this.config.windowMs)
    );
    if (!Array.isArray(raw) || raw.length !== 2) {
      throw new Error("Redis returned an invalid API rate-limit result");
    }
    const count = Number(raw[0]);
    const ttl = Number(raw[1]);
    if (!Number.isFinite(count) || !Number.isFinite(ttl) || ttl < 0) {
      throw new Error("Redis returned invalid API rate-limit counters");
    }
    const limit = this.config.tiers[tier];
    return {
      tier,
      limit,
      used: count,
      remaining: Math.max(0, limit - count),
      resetAt: Date.now() + ttl,
      windowMs: this.config.windowMs,
      allowed: count <= limit,
    };
  }

  async close(): Promise<void> {
    await this.redis.quit();
  }
}

export function buildApiRateLimiter(
  redisUrl: string | undefined,
  config: ApiRateLimits = DEFAULT_API_RATE_LIMITS
): ApiRateLimiter {
  if (!redisUrl) return new LocalApiRateLimiter(config);
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const { Redis } = require("ioredis") as typeof import("ioredis");
  return new RedisApiRateLimiter(new Redis(redisUrl, { lazyConnect: false }), config);
}

function validateConfig(config: ApiRateLimits): void {
  if (!Number.isInteger(config.windowMs) || config.windowMs <= 0) {
    throw new Error("API rate-limit window must be a positive integer");
  }
  for (const tier of ["free", "pro", "enterprise"] as const) {
    if (!Number.isInteger(config.tiers[tier]) || config.tiers[tier] <= 0) {
      throw new Error(`API rate limit for ${tier} must be a positive integer`);
    }
  }
}
