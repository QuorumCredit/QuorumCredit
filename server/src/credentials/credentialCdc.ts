/**
 * Credential Change Data Capture (CDC)
 *
 * #1761: No CDC for credentials. This module captures every credential
 * mutation, publishes change events, and tracks CDC lag.
 *
 * Design:
 * - `CredentialCdc` wraps `CredentialStore` and intercepts status changes
 *   via the existing `onStatusChange` listener hook.
 * - Additional `record` calls capture INSERT / UPDATE / DELETE operations.
 * - Change events are written to an in-memory change log and optionally
 *   published to a `PubSubBus` channel for downstream consumers.
 * - `getLag` returns the age (ms) of the oldest unpublished change event,
 *   which is the standard CDC lag metric.
 */

import type { PubSubBus } from "../pubsub/PubSubBus.js";
import type { CredentialStore, Credential } from "./credentialStore.js";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/** Default pub/sub channel for CDC events. */
export const CREDENTIAL_CDC_CHANNEL = "qc:credential:cdc";

/** Maximum in-memory change log size (ring-buffer eviction). */
const MAX_CHANGE_LOG_SIZE = 50_000;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type CdcOperation = "INSERT" | "UPDATE" | "DELETE";

export interface CdcChangeEvent {
  /** Monotonically incrementing sequence number within this process. */
  seq: number;
  /** Millisecond UTC timestamp when the change was captured. */
  capturedAt: number;
  /** True once the event has been published to the pub/sub bus. */
  published: boolean;
  /** Millisecond UTC timestamp when the event was published (undefined if not yet). */
  publishedAt?: number;
  operation: CdcOperation;
  credentialId: string;
  holderId: string;
  /** Full credential snapshot after the change (undefined for DELETE). */
  after?: Credential;
  /** Credential status before the change (undefined for INSERT). */
  previousStatus?: Credential["status"];
}

export interface CdcLagReport {
  /** Age (ms) of the oldest unacknowledged / unpublished change event. 0 if none. */
  lagMs: number;
  /** Number of change events not yet published. */
  unpublishedCount: number;
  /** Total change events captured since process start. */
  totalCaptured: number;
  /** Total change events published since process start. */
  totalPublished: number;
}

// ---------------------------------------------------------------------------
// CredentialCdc
// ---------------------------------------------------------------------------

/**
 * Change Data Capture service for credentials.
 *
 * Attach to a `CredentialStore` instance at startup:
 *
 *   const cdc = new CredentialCdc(credentialStore, redisBus);
 *
 * All subsequent credential mutations are captured and published.
 */
export class CredentialCdc {
  private readonly changeLog: CdcChangeEvent[] = [];
  private seq = 0;
  private totalPublished = 0;
  private readonly unsubscribe: () => void;

  constructor(
    private readonly store: CredentialStore,
    private readonly bus?: PubSubBus,
    private readonly channel: string = CREDENTIAL_CDC_CHANNEL
  ) {
    // Subscribe to credential status changes from the store
    this.unsubscribe = this.store.onStatusChange((change) => {
      this._capture({
        operation: "UPDATE",
        credentialId: change.credentialId,
        holderId: change.holderId,
        after: this.store.getCredential(change.credentialId),
        previousStatus: change.previousStatus,
      });
    });
  }

  // -------------------------------------------------------------------------
  // Manual capture
  // -------------------------------------------------------------------------

  /**
   * Capture a credential INSERT (called after issueCredential).
   */
  captureInsert(credential: Credential): void {
    this._capture({
      operation: "INSERT",
      credentialId: credential.id,
      holderId: credential.holderId,
      after: credential,
    });
  }

  /**
   * Capture a credential DELETE (called before the credential is removed).
   */
  captureDelete(credential: Credential): void {
    this._capture({
      operation: "DELETE",
      credentialId: credential.id,
      holderId: credential.holderId,
      previousStatus: credential.status,
    });
  }

  // -------------------------------------------------------------------------
  // Publish pipeline
  // -------------------------------------------------------------------------

  /**
   * Publish all unpublished change events to the configured pub/sub bus.
   *
   * If no bus is configured this is a no-op (events stay in the log and can
   * be polled via `listChanges`).
   *
   * @returns Number of events published.
   */
  async publishPending(): Promise<number> {
    if (!this.bus) return 0;

    const pending = this.changeLog.filter((e) => !e.published);
    for (const event of pending) {
      await this.bus.publish(this.channel, JSON.stringify(event));
      event.published = true;
      event.publishedAt = Date.now();
      this.totalPublished += 1;
    }
    return pending.length;
  }

  // -------------------------------------------------------------------------
  // Change log access
  // -------------------------------------------------------------------------

  /**
   * List captured change events.
   *
   * @param limit - Maximum events to return (most-recent first).
   * @param onlyUnpublished - When true, returns only events not yet published.
   */
  listChanges(limit: number = 100, onlyUnpublished: boolean = false): CdcChangeEvent[] {
    let events = this.changeLog.slice();
    if (onlyUnpublished) events = events.filter((e) => !e.published);
    return events.slice(-limit).reverse();
  }

  /**
   * Return all change events for a specific credential.
   */
  getChangesForCredential(credentialId: string): CdcChangeEvent[] {
    return this.changeLog.filter((e) => e.credentialId === credentialId);
  }

  // -------------------------------------------------------------------------
  // Lag monitoring
  // -------------------------------------------------------------------------

  /**
   * CDC lag report.
   *
   * Lag is the age of the oldest unpublished event — a standard operational
   * metric for data pipelines. Zero lag means all changes have been published.
   */
  getLag(): CdcLagReport {
    const unpublished = this.changeLog.filter((e) => !e.published);
    const oldest = unpublished.length > 0 ? unpublished[0] : undefined;
    const lagMs = oldest ? Date.now() - oldest.capturedAt : 0;

    return {
      lagMs,
      unpublishedCount: unpublished.length,
      totalCaptured: this.seq,
      totalPublished: this.totalPublished,
    };
  }

  // -------------------------------------------------------------------------
  // Lifecycle
  // -------------------------------------------------------------------------

  /**
   * Detach from the credential store and stop capturing changes.
   */
  close(): void {
    this.unsubscribe();
  }

  // -------------------------------------------------------------------------
  // Internal helpers
  // -------------------------------------------------------------------------

  private _capture(
    params: Pick<CdcChangeEvent, "operation" | "credentialId" | "holderId"> & {
      after?: Credential;
      previousStatus?: Credential["status"];
    }
  ): void {
    const event: CdcChangeEvent = {
      seq: ++this.seq,
      capturedAt: Date.now(),
      published: false,
      operation: params.operation,
      credentialId: params.credentialId,
      holderId: params.holderId,
      after: params.after,
      previousStatus: params.previousStatus,
    };

    this.changeLog.push(event);

    // Ring-buffer: evict oldest entries once we exceed the cap
    if (this.changeLog.length > MAX_CHANGE_LOG_SIZE) {
      this.changeLog.splice(0, this.changeLog.length - MAX_CHANGE_LOG_SIZE);
    }

    // Fire-and-forget publish to bus if one is configured
    if (this.bus) {
      this.bus
        .publish(this.channel, JSON.stringify(event))
        .then(() => {
          event.published = true;
          event.publishedAt = Date.now();
          this.totalPublished += 1;
        })
        .catch((err: unknown) => {
          // Leave published=false; caller can retry via publishPending()
          console.error("[credentialCdc] publish failed", err);
        });
    }
  }
}
