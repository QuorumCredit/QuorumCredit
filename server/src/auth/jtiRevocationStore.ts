/**
 * Issue #1292: JTI-based token revocation store.
 *
 * Follows the same RedisBus / LocalBus split used throughout the server:
 * - Single-instance / local dev → `LocalRevocationStore` (in-process Map).
 * - Multi-instance / production  → `RedisRevocationStore` (Redis SET with TTL).
 *
 * A revoked JTI is stored until its natural token expiry so memory/Redis space
 * is bounded by the token TTL (default 300 s).  After that the entry is either
 * garbage-collected (LocalRevocationStore) or expired by Redis automatically.
 *
 * Usage:
 *   const store = buildRevocationStore(config.redisUrl, config.tokenTtlSeconds);
 *   await store.revoke(jti, expiresAt);
 *   const isRevoked = await store.isRevoked(jti);
 *   await store.revokeAllForSubject(sub); // invalidates all tokens for an apiKey
 */

import type { Redis } from "ioredis";

export interface RevocationStore {
  /**
   * Mark a specific JTI as revoked.
   * `expiresAtMs` is the token's natural expiry (Unix ms); the store may use it
   * to auto-expire the denylist entry so the store stays bounded.
   */
  revoke(jti: string, expiresAtMs: number): Promise<void>;

  /** True when the given JTI is in the denylist. */
  isRevoked(jti: string): Promise<boolean>;

  /**
   * Revoke all tokens issued to `subject` (apiKey / borrower sub), including
   * ones whose JTI was never individually tracked, by recording a cutoff
   * timestamp rather than requiring a secondary JTI index.
   */
  revokeAllForSubject(subject: string): Promise<void>;

  /**
   * True when `subject` had `revokeAllForSubject` called at-or-after the
   * given token issuance time (`iatSeconds`, Unix seconds). Called by
   * `verifyToken` alongside the per-JTI `isRevoked` check so subject-wide
   * revocation also catches tokens whose JTI was never individually revoked.
   */
  isSubjectRevokedBefore(subject: string, iatSeconds: number): Promise<boolean>;

  /** Close / clean up underlying connections. */
  close(): Promise<void>;
}

// ── Local (single-instance) implementation ─────────────────────────────────────

interface LocalEntry {
  subject: string;
  expiresAtMs: number;
}

/**
 * In-process revocation store backed by a plain Map.
 * NOT multi-instance-safe — for local dev and unit tests only.
 */
export class LocalRevocationStore implements RevocationStore {
  /** jti → { subject, expiresAtMs } */
  private readonly denied = new Map<string, LocalEntry>();
  /** subject → Unix-seconds cutoff; tokens issued at-or-before this are revoked. */
  private readonly subjectRevokedAt = new Map<string, number>();

  async revoke(jti: string, expiresAtMs: number, subject = ""): Promise<void> {
    this.purgeExpired();
    this.denied.set(jti, { subject, expiresAtMs });
  }

  async isRevoked(jti: string): Promise<boolean> {
    const entry = this.denied.get(jti);
    if (!entry) return false;
    if (entry.expiresAtMs <= Date.now()) {
      this.denied.delete(jti);
      return false;
    }
    return true;
  }

  async revokeAllForSubject(subject: string): Promise<void> {
    this.purgeExpired();
    // Record a cutoff timestamp (mirrors RedisRevocationStore's sentinel key)
    // rather than trying to mark each already-tracked JTI revoked in place:
    // this also catches tokens whose JTI was never individually revoke()'d,
    // and isSubjectRevokedBefore is checked on every verify regardless.
    this.subjectRevokedAt.set(subject, Math.floor(Date.now() / 1000));
  }

  async isSubjectRevokedBefore(subject: string, iatSeconds: number): Promise<boolean> {
    const cutoff = this.subjectRevokedAt.get(subject);
    if (cutoff === undefined) return false;
    return iatSeconds <= cutoff;
  }

  private purgeExpired(): void {
    const now = Date.now();
    for (const [jti, entry] of this.denied) {
      if (entry.expiresAtMs <= now) this.denied.delete(jti);
    }
  }

  async close(): Promise<void> {
    this.denied.clear();
    this.subjectRevokedAt.clear();
  }
}

// ── Redis (multi-instance) implementation ──────────────────────────────────────

const REDIS_JTI_PREFIX = "qc:revoked:jti:";
const REDIS_SUB_PREFIX = "qc:revoked:sub:";

/**
 * Redis-backed revocation store for multi-instance deployments.
 *
 * Each revoked JTI is stored as:
 *   `qc:revoked:jti:{jti}` → subject string, with TTL = token remaining lifetime.
 *
 * Subject-level revocation uses:
 *   `qc:revoked:sub:{subject}` → timestamp (Unix s) at which all tokens issued
 *   BEFORE this timestamp are considered revoked.  `verifyToken` compares the
 *   token's `iat` against this value.
 */
export class RedisRevocationStore implements RevocationStore {
  constructor(private readonly redis: Redis) {}

  async revoke(jti: string, expiresAtMs: number, subject = ""): Promise<void> {
    const ttlSeconds = Math.max(1, Math.ceil((expiresAtMs - Date.now()) / 1000));
    await this.redis.set(
      `${REDIS_JTI_PREFIX}${jti}`,
      subject,
      "EX",
      ttlSeconds
    );
  }

  async isRevoked(jti: string): Promise<boolean> {
    const exists = await this.redis.exists(`${REDIS_JTI_PREFIX}${jti}`);
    return exists === 1;
  }

  async revokeAllForSubject(subject: string): Promise<void> {
    // Store the current Unix timestamp (seconds).  Any token with iat < this
    // value will be rejected during verification.
    const now = Math.floor(Date.now() / 1000);
    // Keep the sentinel for 2× the max token TTL to ensure no live token slips through.
    const sentinelTtl = 2 * 24 * 60 * 60; // 2 days
    await this.redis.set(`${REDIS_SUB_PREFIX}${subject}`, String(now), "EX", sentinelTtl);
  }

  /**
   * Check whether all tokens for a given subject issued before a timestamp
   * have been revoked.  Called by verifyToken when the subject field is known.
   */
  async isSubjectRevokedBefore(subject: string, iatSeconds: number): Promise<boolean> {
    const raw = await this.redis.get(`${REDIS_SUB_PREFIX}${subject}`);
    if (!raw) return false;
    return iatSeconds <= Number.parseInt(raw, 10);
  }

  async close(): Promise<void> {
    await this.redis.quit();
  }
}

// ── Factory ────────────────────────────────────────────────────────────────────

/**
 * Build the appropriate revocation store based on whether Redis is configured.
 * Mirrors the `buildBus` pattern in `server/src/index.ts`.
 */
export function buildRevocationStore(redisUrl: string | undefined): RevocationStore {
  if (redisUrl) {
    // Lazily import ioredis so it is not required in local/test environments.
    // eslint-disable-next-line @typescript-eslint/no-require-imports
    const { Redis } = require("ioredis") as typeof import("ioredis");
    const redis = new Redis(redisUrl, { lazyConnect: false });
    return new RedisRevocationStore(redis);
  }
  return new LocalRevocationStore();
}
