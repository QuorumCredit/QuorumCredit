/**
 * Credential holder privacy shield (issue #1772).
 *
 * Gives query results a privacy layer so that holders are not exposed by default:
 * - holder identifiers are replaced with a keyed pseudonym (HMAC-SHA256), so the
 *   same holder stays joinable across results without revealing the address;
 * - caller-specified fields (e.g. free-text metadata) are stripped from records;
 * - aggregate counts are released with Laplace noise (epsilon-differential privacy,
 *   sensitivity 1), so a single holder's presence cannot be inferred from a count;
 * - every disclosure is written to a bounded, append-only audit trail.
 */
import { createHmac, randomBytes } from "node:crypto";

export const MIN_SHIELD_SALT_BYTES = 16;
export const DEFAULT_AUDIT_LIMIT = 1_000;

export interface PrivacyShieldOptions {
  /** Secret key for holder pseudonyms. Must not be shared with query consumers. */
  salt: Buffer | string;
  /** Privacy budget per released count. Smaller is more private. */
  epsilon?: number;
  auditLimit?: number;
  now?: () => number;
  /** Uniform sampler in (0, 1). Injected for deterministic tests. */
  random?: () => number;
}

export interface PrivacyAuditEntry {
  at: number;
  actor: string;
  action: "holder-disclosure" | "aggregate-release";
  subject?: string;
  detail: string;
}

export class PrivacyShield {
  private readonly key: Buffer;
  private readonly epsilon: number;
  private readonly now: () => number;
  private readonly random: () => number;
  private readonly auditLimit: number;
  private readonly audit: PrivacyAuditEntry[] = [];

  constructor(options: PrivacyShieldOptions) {
    const key = typeof options.salt === "string" ? Buffer.from(options.salt, "utf8") : options.salt;
    if (key.length < MIN_SHIELD_SALT_BYTES) {
      throw new Error(`privacy shield key must be at least ${MIN_SHIELD_SALT_BYTES} bytes`);
    }
    const epsilon = options.epsilon ?? 1;
    if (!Number.isFinite(epsilon) || epsilon <= 0) {
      throw new Error("epsilon must be a positive number");
    }
    this.key = key;
    this.epsilon = epsilon;
    this.now = options.now ?? Date.now;
    this.random = options.random ?? Math.random;
    this.auditLimit = options.auditLimit ?? DEFAULT_AUDIT_LIMIT;
  }

  /** Keyed, stable pseudonym for a holder identifier. */
  pseudonymize(holder: string): string {
    return createHmac("sha256", this.key).update(holder).digest("hex").slice(0, 32);
  }

  /**
   * Copy a record with its holder replaced by a pseudonym and the given fields removed.
   * The `holderField` value is replaced; the original record is not mutated.
   */
  shieldRecord<T extends Record<string, unknown>>(
    record: T,
    options: { holderField: keyof T & string; stripFields?: string[]; actor: string }
  ): Record<string, unknown> {
    const holder = record[options.holderField];
    if (typeof holder !== "string" || holder.length === 0) {
      throw new Error(`record is missing holder field "${options.holderField}"`);
    }
    const pseudonym = this.pseudonymize(holder);
    const shielded: Record<string, unknown> = {};
    const strip = new Set(options.stripFields ?? []);
    for (const [field, value] of Object.entries(record)) {
      if (field === options.holderField || strip.has(field)) continue;
      shielded[field] = value;
    }
    shielded[options.holderField] = pseudonym;
    this.record({ actor: options.actor, action: "holder-disclosure", subject: pseudonym, detail: "record" });
    return shielded;
  }

  /**
   * Release a count with Laplace noise (scale = 1 / epsilon). The result is clamped
   * to zero and rounded to an integer so it reads as a plausible count.
   */
  releaseCount(actor: string, label: string, trueCount: number): number {
    if (!Number.isInteger(trueCount) || trueCount < 0) {
      throw new Error("trueCount must be a non-negative integer");
    }
    const noisy = Math.max(0, Math.round(trueCount + laplaceNoise(1 / this.epsilon, this.random)));
    this.record({ actor, action: "aggregate-release", detail: label });
    return noisy;
  }

  /** Audit entries, oldest first. Pass a subject pseudonym to filter to one holder. */
  auditTrail(subject?: string): PrivacyAuditEntry[] {
    const entries = subject ? this.audit.filter((entry) => entry.subject === subject) : this.audit;
    return entries.map((entry) => ({ ...entry }));
  }

  private record(entry: Omit<PrivacyAuditEntry, "at">): void {
    this.audit.push({ ...entry, at: this.now() });
    if (this.audit.length > this.auditLimit) {
      this.audit.splice(0, this.audit.length - this.auditLimit);
    }
  }
}

/** Sample Laplace(0, scale) by inverse CDF. `random` must return values in (0, 1). */
export function laplaceNoise(scale: number, random: () => number = Math.random): number {
  const u = random() - 0.5;
  // Guard the log argument: random() == 0 would otherwise yield -Infinity.
  const tail = Math.max(1 - 2 * Math.abs(u), Number.MIN_VALUE);
  return -scale * Math.sign(u) * Math.log(tail);
}

/** Random key for deployments that do not configure one. Regenerates on every restart. */
export function generateShieldKey(): Buffer {
  return randomBytes(32);
}
