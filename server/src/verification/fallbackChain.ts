/**
 * Issue #1781: Credential Verification Fallback Chain
 *
 * Removes the single point of failure in credential verification by asking an
 * ordered list of attestors. The first attestor that gives a definitive answer
 * (verified or rejected) wins. An attestor that errors is retried up to
 * `maxAttempts` times and then skipped in favour of the next one.
 *
 * Only transient failures (thrown errors) trigger fallback. A definitive
 * "rejected" answer is final and is never retried against another attestor,
 * so a fallback cannot be used to overturn a rejection.
 */
import { metrics } from "../http/metricsRegistry.js";

export interface Attestor {
  /** Stable identifier used in metrics and results. */
  id: string;
  /** Resolves to true (verified) or false (rejected). Throws when unavailable. */
  verify(credentialId: string): Promise<boolean>;
}

export interface AttemptRecord {
  attestorId: string;
  attempt: number;
  outcome: "verified" | "rejected" | "error";
  error?: string;
}

export interface FallbackVerificationResult {
  verified: boolean;
  attestorId: string;
  /** True when an attestor other than the primary produced the answer. */
  fallbackUsed: boolean;
  attempts: AttemptRecord[];
}

export interface AttestorStats {
  attestorId: string;
  attempts: number;
  successes: number;
  failures: number;
}

export interface FallbackChainOptions {
  /** Attempts per attestor before moving on. Defaults to 2. */
  maxAttempts?: number;
  /** Delay before retry `attempt` (1-based). Defaults to no delay. */
  backoffMs?: (attempt: number) => number;
  /** Injectable for tests. */
  sleep?: (ms: number) => Promise<void>;
}

export class VerificationUnavailableError extends Error {
  constructor(readonly attempts: AttemptRecord[]) {
    super("all verification attestors are unavailable");
  }
}

const DEFAULT_MAX_ATTEMPTS = 2;

const defaultSleep = (ms: number): Promise<void> =>
  ms <= 0 ? Promise.resolve() : new Promise((resolve) => setTimeout(resolve, ms));

export class VerificationFallbackChain {
  private readonly attestors: readonly Attestor[];
  private readonly maxAttempts: number;
  private readonly backoffMs: (attempt: number) => number;
  private readonly sleep: (ms: number) => Promise<void>;
  private readonly stats = new Map<string, AttestorStats>();
  private fallbackCount = 0;

  constructor(attestors: Attestor[], options: FallbackChainOptions = {}) {
    if (attestors.length === 0) {
      throw new Error("fallback chain needs at least one attestor");
    }
    const ids = new Set(attestors.map((a) => a.id));
    if (ids.size !== attestors.length) {
      throw new Error("attestor ids must be unique");
    }
    const maxAttempts = options.maxAttempts ?? DEFAULT_MAX_ATTEMPTS;
    if (!Number.isInteger(maxAttempts) || maxAttempts < 1) {
      throw new Error("maxAttempts must be a positive integer");
    }

    this.attestors = [...attestors];
    this.maxAttempts = maxAttempts;
    this.backoffMs = options.backoffMs ?? (() => 0);
    this.sleep = options.sleep ?? defaultSleep;
    for (const attestor of attestors) {
      this.stats.set(attestor.id, {
        attestorId: attestor.id,
        attempts: 0,
        successes: 0,
        failures: 0,
      });
    }
  }

  /** Ordered attestor ids, primary first. */
  attestorOrder(): string[] {
    return this.attestors.map((a) => a.id);
  }

  async verify(credentialId: string): Promise<FallbackVerificationResult> {
    const attempts: AttemptRecord[] = [];
    const primaryId = this.attestors[0]!.id;

    for (const attestor of this.attestors) {
      for (let attempt = 1; attempt <= this.maxAttempts; attempt++) {
        if (attempt > 1) await this.sleep(this.backoffMs(attempt - 1));
        const stat = this.stats.get(attestor.id)!;
        stat.attempts++;
        metrics.incCounter("qc_verification_attempts_total");

        try {
          const verified = await attestor.verify(credentialId);
          stat.successes++;
          attempts.push({
            attestorId: attestor.id,
            attempt,
            outcome: verified ? "verified" : "rejected",
          });
          const fallbackUsed = attestor.id !== primaryId;
          if (fallbackUsed) {
            this.fallbackCount++;
            metrics.incCounter("qc_verification_fallback_total");
          }
          return { verified, attestorId: attestor.id, fallbackUsed, attempts };
        } catch (error) {
          stat.failures++;
          attempts.push({
            attestorId: attestor.id,
            attempt,
            outcome: "error",
            error: error instanceof Error ? error.message : String(error),
          });
        }
      }
    }

    metrics.incCounter("qc_verification_unavailable_total");
    throw new VerificationUnavailableError(attempts);
  }

  /** Per-attestor counters plus how many verifications were served by a fallback. */
  getStats(): { attestors: AttestorStats[]; fallbackUsed: number } {
    return {
      attestors: this.attestorOrder().map((id) => ({ ...this.stats.get(id)! })),
      fallbackUsed: this.fallbackCount,
    };
  }
}
