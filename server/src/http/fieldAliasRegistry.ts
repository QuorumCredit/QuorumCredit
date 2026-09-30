/**
 * Issue #1750: API Field Aliasing for Backward Compatibility
 *
 * Field name changes break clients. This module provides a field alias registry
 * that maps old (deprecated) field names to their new canonical names, allowing
 * the API to transparently support both old and new field names during a migration
 * window. Deprecated alias usage is tracked and alerted.
 *
 * Usage:
 *   - Register aliases: `fieldAliasRegistry.register({ oldField: "stake", newField: "stakeAmount", ... })`
 *   - Apply to a response body: `fieldAliasRegistry.applyAliases(responseBody)`
 *   - Read an aliased field from a request body: `fieldAliasRegistry.resolveField(body, "stake")`
 */

import { metrics } from "./metricsRegistry.js";

/** A single alias mapping from a deprecated field name to its canonical replacement. */
export interface FieldAlias {
  /** The old, deprecated field name that clients may still send/receive. */
  oldField: string;
  /** The current canonical field name in the data model. */
  newField: string;
  /**
   * The ISO-8601 date from which the old field is considered deprecated.
   * Used in alert messages and metrics labels.
   */
  deprecatedAt: Date;
  /**
   * Optional sunset date after which the alias will no longer be honoured.
   * Once this date passes, the registry will emit "critical" level alerts.
   */
  sunsetAt?: Date;
  /**
   * Human-readable migration hint surfaced in alerts and the usage report.
   * e.g. "Rename 'stake' to 'stakeAmount' in all payloads."
   */
  migrationNote?: string;
  /**
   * Which direction(s) the alias applies:
   *  - "response": old name injected into outgoing JSON (backward compat for readers)
   *  - "request":  old name accepted on incoming JSON (backward compat for writers)
   *  - "both":     both directions (default)
   */
  direction?: "response" | "request" | "both";
}

/** Accumulated usage statistics for a single alias. */
export interface FieldAliasUsage {
  oldField: string;
  newField: string;
  /** Total number of times this alias was triggered. */
  hits: number;
  firstSeenAt?: number;
  lastSeenAt?: number;
  /** Hit counts keyed by client identifier (IP or API-key prefix). */
  clients: Record<string, number>;
}

export type AliasAlertLevel = "info" | "warning" | "critical";

export interface FieldAliasAlert {
  level: AliasAlertLevel;
  oldField: string;
  newField: string;
  client: string;
  hits: number;
  sunsetAt?: string;
  message: string;
}

export type FieldAliasAlertHandler = (alert: FieldAliasAlert) => void;

const MAX_TRACKED_CLIENTS = 500;
const SUNSET_WARNING_WINDOW_MS = 30 * 24 * 60 * 60 * 1000; // 30 days

/**
 * Central registry for API field aliases.
 *
 * Lifecycle:
 *   1. Register aliases once at server startup.
 *   2. Middleware calls `applyToResponse()` to inject old field names into outgoing
 *      objects so existing clients keep working.
 *   3. Route handlers call `resolveField()` to read a value regardless of whether
 *      the caller used the old or new field name.
 *   4. Every alias hit is counted; throttled alerts fire when deprecated fields
 *      are still in use.
 */
export class FieldAliasRegistry {
  private readonly aliases = new Map<string, FieldAlias>(); // keyed by oldField
  private readonly usage = new Map<string, FieldAliasUsage>();
  private readonly lastAlertAt = new Map<string, number>();
  private readonly alertHandlers = new Set<FieldAliasAlertHandler>();

  constructor(private readonly alertThrottleMs = 60 * 60 * 1000) {}

  // ---------------------------------------------------------------------------
  // Registration
  // ---------------------------------------------------------------------------

  /**
   * Register a field alias.
   *
   * @throws if `oldField === newField` (no-op alias).
   * @throws if an alias for `oldField` is already registered with a different target.
   */
  register(alias: FieldAlias): void {
    if (alias.oldField === alias.newField) {
      throw new Error(
        `[fieldAliasRegistry] Cannot register a no-op alias: old="${alias.oldField}" and new="${alias.newField}" are identical.`
      );
    }
    const existing = this.aliases.get(alias.oldField);
    if (existing && existing.newField !== alias.newField) {
      throw new Error(
        `[fieldAliasRegistry] Conflict: "${alias.oldField}" is already aliased to "${existing.newField}", cannot re-alias to "${alias.newField}".`
      );
    }
    this.aliases.set(alias.oldField, { direction: "both", ...alias });
    this.usage.set(alias.oldField, {
      oldField: alias.oldField,
      newField: alias.newField,
      hits: 0,
      clients: {},
    });
  }

  /** Unregister an alias (e.g. after its sunset date has fully passed). */
  unregister(oldField: string): boolean {
    this.usage.delete(oldField);
    this.lastAlertAt.delete(oldField);
    return this.aliases.delete(oldField);
  }

  // ---------------------------------------------------------------------------
  // Alert handlers
  // ---------------------------------------------------------------------------

  /** Subscribe to alias-usage alerts. Returns an unsubscribe function. */
  onAlert(handler: FieldAliasAlertHandler): () => void {
    this.alertHandlers.add(handler);
    return () => this.alertHandlers.delete(handler);
  }

  // ---------------------------------------------------------------------------
  // Response serialization — inject old field names into outgoing objects
  // ---------------------------------------------------------------------------

  /**
   * Inject deprecated field names alongside their canonical equivalents in `obj`.
   *
   * For every registered alias with direction "response" or "both", if `obj`
   * contains `newField`, the value is also written under `oldField` so legacy
   * clients that still read the old name continue to work.
   *
   * The mutation is applied **in-place** and `obj` is also returned for chaining.
   *
   * @param obj         - The outgoing response body (mutable).
   * @param clientId    - Optional identifier for metrics / alerting (IP or API-key prefix).
   * @param recursive   - When true, descend into nested plain-objects and arrays.
   */
  applyToResponse(
    obj: Record<string, unknown>,
    clientId = "unknown",
    recursive = false
  ): Record<string, unknown> {
    for (const [oldField, alias] of this.aliases) {
      if (alias.direction === "request") continue; // skip request-only aliases
      if (Object.prototype.hasOwnProperty.call(obj, alias.newField)) {
        obj[oldField] = obj[alias.newField];
        this._trackHit(alias, clientId);
      }
    }

    if (recursive) {
      for (const value of Object.values(obj)) {
        if (Array.isArray(value)) {
          for (const item of value) {
            if (item && typeof item === "object" && !Array.isArray(item)) {
              this.applyToResponse(item as Record<string, unknown>, clientId, true);
            }
          }
        } else if (value && typeof value === "object" && !Array.isArray(value)) {
          this.applyToResponse(value as Record<string, unknown>, clientId, true);
        }
      }
    }

    return obj;
  }

  // ---------------------------------------------------------------------------
  // Request deserialization — resolve old field names on incoming payloads
  // ---------------------------------------------------------------------------

  /**
   * Read a field value from `body` honouring all registered aliases.
   *
   * Resolution order:
   *   1. `canonicalName` itself (the current field name).
   *   2. Any `oldField` whose `newField` equals `canonicalName`.
   *
   * Returns `undefined` when neither the canonical name nor any of its aliases
   * is present in `body`.
   *
   * @param body          - Incoming request payload.
   * @param canonicalName - The current (new) field name to look up.
   * @param clientId      - Optional identifier for metrics / alerting.
   */
  resolveField(
    body: Record<string, unknown>,
    canonicalName: string,
    clientId = "unknown"
  ): unknown {
    // Prefer the canonical name if present.
    if (Object.prototype.hasOwnProperty.call(body, canonicalName)) {
      return body[canonicalName];
    }

    // Fall back to any registered alias that maps to `canonicalName`.
    for (const [oldField, alias] of this.aliases) {
      if (alias.newField !== canonicalName) continue;
      if (alias.direction === "response") continue; // skip response-only aliases
      if (Object.prototype.hasOwnProperty.call(body, oldField)) {
        this._trackHit(alias, clientId);
        return body[oldField];
      }
    }

    return undefined;
  }

  /**
   * Normalise an incoming request body so that all aliased old field names are
   * replaced with their canonical equivalents.  Useful when the rest of the
   * handler can then ignore aliases entirely.
   *
   * Returns a new object; the original `body` is not mutated.
   */
  normaliseRequestBody(
    body: Record<string, unknown>,
    clientId = "unknown"
  ): Record<string, unknown> {
    const out: Record<string, unknown> = { ...body };

    for (const [oldField, alias] of this.aliases) {
      if (alias.direction === "response") continue;
      if (!Object.prototype.hasOwnProperty.call(out, oldField)) continue;
      // Only promote the alias value if the canonical field isn't already set.
      if (!Object.prototype.hasOwnProperty.call(out, alias.newField)) {
        out[alias.newField] = out[oldField];
      }
      delete out[oldField];
      this._trackHit(alias, clientId);
    }

    return out;
  }

  // ---------------------------------------------------------------------------
  // Introspection
  // ---------------------------------------------------------------------------

  /** All currently registered aliases. */
  listAliases(): FieldAlias[] {
    return Array.from(this.aliases.values());
  }

  /** Usage report for all aliases. */
  report(): Array<FieldAliasUsage & {
    deprecatedAt: string;
    sunsetAt?: string;
    sunsetPassed: boolean;
    migrationNote?: string;
    direction: "response" | "request" | "both";
  }> {
    const now = Date.now();
    return Array.from(this.aliases.values()).map((alias) => ({
      ...(this.usage.get(alias.oldField) ?? {
        oldField: alias.oldField,
        newField: alias.newField,
        hits: 0,
        clients: {},
      }),
      deprecatedAt: alias.deprecatedAt.toISOString(),
      sunsetAt: alias.sunsetAt?.toISOString(),
      sunsetPassed: alias.sunsetAt ? alias.sunsetAt.getTime() <= now : false,
      migrationNote: alias.migrationNote,
      direction: alias.direction ?? "both",
    }));
  }

  // ---------------------------------------------------------------------------
  // Private helpers
  // ---------------------------------------------------------------------------

  private _trackHit(alias: FieldAlias, clientId: string): void {
    const now = Date.now();
    const usage = this.usage.get(alias.oldField);
    if (!usage) return;

    usage.hits++;
    usage.firstSeenAt ??= now;
    usage.lastSeenAt = now;

    if (
      clientId in usage.clients ||
      Object.keys(usage.clients).length < MAX_TRACKED_CLIENTS
    ) {
      usage.clients[clientId] = (usage.clients[clientId] ?? 0) + 1;
    }

    // Prometheus counter
    metrics.incLabeledCounter(
      "qc_field_alias_hits_total",
      "old_field",
      alias.oldField
    );

    // Throttled alert
    const alertKey = `${alias.oldField}\n${clientId}`;
    const last = this.lastAlertAt.get(alertKey) ?? 0;
    if (now - last < this.alertThrottleMs) return;
    this.lastAlertAt.set(alertKey, now);

    const level = this._alertLevel(alias, now);
    const alert: FieldAliasAlert = {
      level,
      oldField: alias.oldField,
      newField: alias.newField,
      client: clientId,
      hits: usage.hits,
      sunsetAt: alias.sunsetAt?.toISOString(),
      message: this._alertMessage(alias, clientId, usage.hits),
    };

    metrics.incLabeledCounter("qc_field_alias_alerts_total", "level", level);

    if (this.alertHandlers.size === 0) {
      const log = level === "info" ? console.info : console.warn;
      log(`[fieldAliasRegistry] ${level}: ${alert.message}`);
    }
    for (const handler of this.alertHandlers) {
      try {
        handler(alert);
      } catch (err) {
        console.error("[fieldAliasRegistry] alert handler threw", err);
      }
    }
  }

  private _alertLevel(alias: FieldAlias, now: number): AliasAlertLevel {
    if (!alias.sunsetAt) return "info";
    const remaining = alias.sunsetAt.getTime() - now;
    if (remaining <= 0) return "critical";
    if (remaining <= SUNSET_WARNING_WINDOW_MS) return "warning";
    return "info";
  }

  private _alertMessage(
    alias: FieldAlias,
    client: string,
    hits: number
  ): string {
    let msg =
      `Deprecated field "${alias.oldField}" used by ${client} (${hits} hit(s)). ` +
      `Migrate to "${alias.newField}".`;
    if (alias.sunsetAt) {
      msg += ` Sunset: ${alias.sunsetAt.toISOString().slice(0, 10)}.`;
    }
    if (alias.migrationNote) {
      msg += ` Note: ${alias.migrationNote}`;
    }
    return msg;
  }
}

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

export const fieldAliasRegistry = new FieldAliasRegistry();

// ---------------------------------------------------------------------------
// Pre-registered aliases for known QuorumCredit field renames
// ---------------------------------------------------------------------------

// #1750 — vouch payload: "stake" renamed to "stakeAmount" for clarity
fieldAliasRegistry.register({
  oldField: "stake",
  newField: "stakeAmount",
  deprecatedAt: new Date("2026-09-01T00:00:00Z"),
  sunsetAt: new Date("2027-03-01T00:00:00Z"),
  migrationNote:
    'Rename all occurrences of "stake" to "stakeAmount" in vouch payloads and responses.',
  direction: "both",
});

// #1750 — loan payload: "amount" renamed to "loanAmount"
fieldAliasRegistry.register({
  oldField: "amount",
  newField: "loanAmount",
  deprecatedAt: new Date("2026-09-01T00:00:00Z"),
  sunsetAt: new Date("2027-03-01T00:00:00Z"),
  migrationNote:
    'Rename all occurrences of "amount" to "loanAmount" in loan request/response payloads.',
  direction: "both",
});

// #1750 — holder identifier: "holderId" renamed to "userId"
fieldAliasRegistry.register({
  oldField: "holderId",
  newField: "userId",
  deprecatedAt: new Date("2026-09-15T00:00:00Z"),
  sunsetAt: new Date("2027-06-01T00:00:00Z"),
  migrationNote:
    'Replace "holderId" with "userId" across all credential and verification payloads.',
  direction: "both",
});
