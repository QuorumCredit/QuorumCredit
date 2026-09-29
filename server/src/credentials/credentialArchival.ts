/**
 * Issue #1753: Credential Archival Service
 *
 * No automatic archival of old credentials. Archival reduces active storage by
 * moving credentials that have exceeded their archival threshold into a separate
 * archive store, while still supporting retrieval when needed.
 *
 * Responsibilities:
 *   1. Archival policy configuration — rules for when a credential becomes
 *      eligible for archival (age, status, etc.).
 *   2. Background archival job — a periodic sweep that identifies and archives
 *      eligible credentials from the active store.
 *   3. Archive retrieval — look up archived credentials by ID or holder.
 *   4. Archival statistics — counts, sizes, and timing for operational visibility.
 */

import type { Credential } from "./credentialStore.js";
import { metrics } from "../http/metricsRegistry.js";

// ---------------------------------------------------------------------------
// Policy configuration
// ---------------------------------------------------------------------------

/**
 * A single archival policy rule. A credential is eligible for archival when
 * ALL conditions listed in this rule evaluate to true.
 */
export interface ArchivalPolicyRule {
  /** Human-readable name used in logs and the statistics report. */
  name: string;

  /**
   * Archive credentials whose status matches any of these values.
   * Leave empty / omit to match any status.
   */
  eligibleStatuses?: Array<Credential["status"]>;

  /**
   * Archive credentials older than this many seconds (based on `issuedAt`).
   * e.g. 90 * 24 * 60 * 60  → 90 days
   */
  maxAgeSeconds?: number;

  /**
   * Archive credentials that expired more than this many seconds ago
   * (based on `expiresAt`).
   * e.g. 30 * 24 * 60 * 60  → 30 days past expiry
   */
  expiredGracePeriodSeconds?: number;

  /**
   * Only archive credentials whose type matches one of these values.
   * Leave empty / omit to match any type.
   */
  eligibleTypes?: Array<Credential["type"]>;
}

export interface ArchivalPolicy {
  /** One or more rules — a credential is archived when ANY rule matches it. */
  rules: ArchivalPolicyRule[];

  /**
   * How often the background sweep runs (milliseconds).
   * Default: 6 hours (21_600_000 ms).
   */
  sweepIntervalMs?: number;

  /**
   * Maximum number of credentials to archive in a single sweep pass to
   * avoid overwhelming the system.  Default: 1000.
   */
  batchSize?: number;
}

/** Default policy applied if no custom policy is supplied. */
const DEFAULT_POLICY: Required<ArchivalPolicy> = {
  rules: [
    {
      name: "revoked-and-old",
      eligibleStatuses: ["revoked"],
      maxAgeSeconds: 90 * 24 * 60 * 60, // 90 days
    },
    {
      name: "expired-past-grace",
      eligibleStatuses: ["expired"],
      expiredGracePeriodSeconds: 30 * 24 * 60 * 60, // 30 days
    },
    {
      name: "very-old-any-status",
      maxAgeSeconds: 365 * 24 * 60 * 60, // 1 year regardless of status
    },
  ],
  sweepIntervalMs: 6 * 60 * 60 * 1000, // 6 hours
  batchSize: 1000,
};

// ---------------------------------------------------------------------------
// Archive record
// ---------------------------------------------------------------------------

/** A credential that has been moved out of the active store into the archive. */
export interface ArchivedCredential {
  /** Original credential data, unchanged. */
  credential: Credential;
  /** ISO-8601 timestamp at which the credential was archived. */
  archivedAt: number;
  /** Which policy rule triggered the archival. */
  archivedByRule: string;
}

// ---------------------------------------------------------------------------
// Statistics
// ---------------------------------------------------------------------------

export interface ArchivalStats {
  /** Total credentials currently in the archive. */
  totalArchived: number;
  /** Credentials archived during the most recent sweep. */
  lastSweepArchived: number;
  /** ISO-8601 timestamp of the last completed sweep, or null if never run. */
  lastSweepAt: number | null;
  /** Duration of the last sweep in milliseconds. */
  lastSweepDurationMs: number;
  /** Cumulative credentials archived across all sweeps since server start. */
  totalArchivedAllTime: number;
  /** Breakdown by rule name — how many credentials each rule has archived. */
  byRule: Record<string, number>;
  /** Breakdown by credential type. */
  byType: Record<string, number>;
  /** Breakdown by credential status at the time of archival. */
  byStatus: Record<string, number>;
  /** Whether the background sweep job is currently active. */
  jobRunning: boolean;
}

// ---------------------------------------------------------------------------
// Credential Archival Service
// ---------------------------------------------------------------------------

/**
 * Manages the archival lifecycle for credentials.
 *
 * Typical usage in server startup:
 * ```ts
 * const archivalService = new CredentialArchivalService(credentialStore);
 * archivalService.start();                  // launches background sweep
 * // ...
 * await archivalService.stop();             // graceful shutdown
 * ```
 */
export class CredentialArchivalService {
  private readonly policy: Required<ArchivalPolicy>;
  private readonly archive = new Map<string, ArchivedCredential>();

  // Per-run stats
  private lastSweepAt: number | null = null;
  private lastSweepArchived = 0;
  private lastSweepDurationMs = 0;
  private totalArchivedAllTime = 0;
  private readonly byRule: Record<string, number> = {};
  private readonly byType: Record<string, number> = {};
  private readonly byStatus: Record<string, number> = {};

  private sweepTimer: ReturnType<typeof setInterval> | null = null;
  private running = false;

  /**
   * @param getActiveCredentials  - Function that returns the current active
   *   credential list.  Injected to avoid a circular dependency on credentialStore.
   * @param removeActiveCredential - Function that removes a credential from the
   *   active store after archival.
   * @param policy - Optional custom archival policy.
   */
  constructor(
    private readonly getActiveCredentials: () => Credential[],
    private readonly removeActiveCredential: (id: string) => void,
    policy?: Partial<ArchivalPolicy>
  ) {
    this.policy = {
      ...DEFAULT_POLICY,
      ...policy,
      rules: policy?.rules ?? DEFAULT_POLICY.rules,
    };
  }

  // ---------------------------------------------------------------------------
  // Lifecycle
  // ---------------------------------------------------------------------------

  /**
   * Start the background archival sweep job.
   * Calling start() when already running is a no-op.
   */
  start(): void {
    if (this.running) return;
    this.running = true;
    this.sweepTimer = setInterval(() => {
      this._runSweep();
    }, this.policy.sweepIntervalMs);
    console.info(
      `[credentialArchival] Background sweep started (interval=${this.policy.sweepIntervalMs}ms, ` +
        `batchSize=${this.policy.batchSize}, rules=${this.policy.rules.length}).`
    );
  }

  /**
   * Stop the background archival sweep job and wait for any in-progress sweep
   * to complete.
   */
  stop(): void {
    if (!this.running) return;
    this.running = false;
    if (this.sweepTimer !== null) {
      clearInterval(this.sweepTimer);
      this.sweepTimer = null;
    }
    console.info("[credentialArchival] Background sweep stopped.");
  }

  // ---------------------------------------------------------------------------
  // Manual / on-demand sweep
  // ---------------------------------------------------------------------------

  /**
   * Run a single archival sweep synchronously and return the number of
   * credentials archived in this pass.  Safe to call manually (e.g. from an
   * admin endpoint or integration test) regardless of whether the background
   * job is running.
   */
  runSweepNow(): number {
    return this._runSweep();
  }

  // ---------------------------------------------------------------------------
  // Archive retrieval
  // ---------------------------------------------------------------------------

  /**
   * Retrieve an archived credential by its original credential ID.
   * Returns `undefined` when the credential is not in the archive.
   */
  getArchivedCredential(credentialId: string): ArchivedCredential | undefined {
    return this.archive.get(credentialId);
  }

  /**
   * Retrieve all archived credentials for a given holder.
   */
  getArchivedCredentialsForHolder(holderId: string): ArchivedCredential[] {
    return Array.from(this.archive.values()).filter(
      (a) => a.credential.holderId === holderId
    );
  }

  /**
   * List all archived credentials, optionally filtered by type or status.
   */
  listArchived(opts?: {
    type?: Credential["type"];
    status?: Credential["status"];
    limit?: number;
    offset?: number;
  }): ArchivedCredential[] {
    let results = Array.from(this.archive.values());

    if (opts?.type) {
      results = results.filter((a) => a.credential.type === opts.type);
    }
    if (opts?.status) {
      results = results.filter((a) => a.credential.status === opts.status);
    }

    const offset = opts?.offset ?? 0;
    const limit = opts?.limit ?? results.length;
    return results.slice(offset, offset + limit);
  }

  /**
   * Check whether a given credential ID is currently in the archive.
   */
  isArchived(credentialId: string): boolean {
    return this.archive.has(credentialId);
  }

  /**
   * Manually archive a single credential by ID, bypassing the sweep policy.
   * Returns false if the credential was already archived.
   */
  archiveCredential(credential: Credential, ruleName = "manual"): boolean {
    if (this.archive.has(credential.id)) return false;

    this.archive.set(credential.id, {
      credential,
      archivedAt: Math.floor(Date.now() / 1000),
      archivedByRule: ruleName,
    });
    this.removeActiveCredential(credential.id);
    this._incrementCounters(ruleName, credential);
    metrics.incCounter("qc_credentials_archived_total");
    return true;
  }

  // ---------------------------------------------------------------------------
  // Statistics
  // ---------------------------------------------------------------------------

  getStats(): ArchivalStats {
    return {
      totalArchived: this.archive.size,
      lastSweepArchived: this.lastSweepArchived,
      lastSweepAt: this.lastSweepAt,
      lastSweepDurationMs: this.lastSweepDurationMs,
      totalArchivedAllTime: this.totalArchivedAllTime,
      byRule: { ...this.byRule },
      byType: { ...this.byType },
      byStatus: { ...this.byStatus },
      jobRunning: this.running,
    };
  }

  // ---------------------------------------------------------------------------
  // Private helpers
  // ---------------------------------------------------------------------------

  private _runSweep(): number {
    const sweepStart = Date.now();
    const nowSec = Math.floor(sweepStart / 1000);
    let archived = 0;

    try {
      const candidates = this.getActiveCredentials();
      let processed = 0;

      for (const cred of candidates) {
        if (processed >= this.policy.batchSize) break;

        const matchedRule = this._matchesPolicy(cred, nowSec);
        if (!matchedRule) continue;

        this.archive.set(cred.id, {
          credential: cred,
          archivedAt: nowSec,
          archivedByRule: matchedRule,
        });
        this.removeActiveCredential(cred.id);
        this._incrementCounters(matchedRule, cred);
        archived++;
        processed++;
      }

      metrics.incCounter("qc_credentials_archived_total", archived);
      if (archived > 0) {
        console.info(
          `[credentialArchival] Sweep archived ${archived} credential(s) in ${Date.now() - sweepStart}ms.`
        );
      }
    } catch (err) {
      console.error("[credentialArchival] Sweep encountered an error:", err);
    }

    this.lastSweepAt = Math.floor(sweepStart / 1000);
    this.lastSweepArchived = archived;
    this.lastSweepDurationMs = Date.now() - sweepStart;
    this.totalArchivedAllTime += archived;

    return archived;
  }

  /**
   * Returns the name of the first matching policy rule, or `null` when the
   * credential is not eligible.
   */
  private _matchesPolicy(cred: Credential, nowSec: number): string | null {
    for (const rule of this.policy.rules) {
      if (!this._ruleMatches(rule, cred, nowSec)) continue;
      return rule.name;
    }
    return null;
  }

  private _ruleMatches(
    rule: ArchivalPolicyRule,
    cred: Credential,
    nowSec: number
  ): boolean {
    // Status filter
    if (rule.eligibleStatuses && rule.eligibleStatuses.length > 0) {
      if (!rule.eligibleStatuses.includes(cred.status)) return false;
    }

    // Type filter
    if (rule.eligibleTypes && rule.eligibleTypes.length > 0) {
      if (!rule.eligibleTypes.includes(cred.type)) return false;
    }

    // Age check
    if (rule.maxAgeSeconds !== undefined) {
      const ageSeconds = nowSec - cred.issuedAt;
      if (ageSeconds < rule.maxAgeSeconds) return false;
    }

    // Expiry grace period check
    if (rule.expiredGracePeriodSeconds !== undefined) {
      const expiredAgoSeconds = nowSec - cred.expiresAt;
      if (expiredAgoSeconds < rule.expiredGracePeriodSeconds) return false;
    }

    return true;
  }

  private _incrementCounters(ruleName: string, cred: Credential): void {
    this.byRule[ruleName] = (this.byRule[ruleName] ?? 0) + 1;
    this.byType[cred.type] = (this.byType[cred.type] ?? 0) + 1;
    this.byStatus[cred.status] = (this.byStatus[cred.status] ?? 0) + 1;
  }
}

// ---------------------------------------------------------------------------
// Factory / singleton wiring
// ---------------------------------------------------------------------------

/**
 * Build a CredentialArchivalService wired to the shared credentialStore
 * singleton.  Called from `server/src/index.ts` at startup.
 *
 * Importing credentialStore here would create a circular dependency through
 * credentialStore → credentialArchival → credentialStore, so the caller
 * injects the accessor and remover functions directly.
 */
export function buildCredentialArchivalService(
  getActiveCredentials: () => Credential[],
  removeActiveCredential: (id: string) => void,
  policy?: Partial<ArchivalPolicy>
): CredentialArchivalService {
  return new CredentialArchivalService(
    getActiveCredentials,
    removeActiveCredential,
    policy
  );
}
