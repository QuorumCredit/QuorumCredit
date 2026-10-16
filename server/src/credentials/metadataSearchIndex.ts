/**
 * Issue #1775: Credential metadata search index.
 *
 * An inverted (full-text) index over credential metadata. Every metadata key path
 * and leaf value is tokenized into lowercase terms and mapped to the credentials
 * that contain them, so a query is an intersection of postings lists rather than a
 * scan over every credential.
 *
 * The index is kept current in two ways:
 *   - incrementally, by subscribing to the credential store's issue events;
 *   - in bulk, via `rebuild()` (on attach and on demand).
 * Maintenance (`runMaintenance`) prunes postings for credentials that have left the
 * store, and `startMaintenance` runs that job on an interval. Query latency is sampled
 * so the index's performance can be tracked through `stats()`.
 *
 * Results are filtered and ordered against the live store, so status changes need no
 * re-indexing. The index itself is in-memory, like the credential store it mirrors.
 */

import { credentialStore, type Credential, type CredentialStore } from "./credentialStore.js";

export const MAX_QUERY_TERMS = 16;
export const MAX_TOKEN_LENGTH = 64;
const MAX_METADATA_DEPTH = 5;
const MAX_TOKENS_PER_CREDENTIAL = 500;
const LATENCY_SAMPLE_SIZE = 1000;
const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 500;

export interface MetadataSearchOptions {
  status?: Credential["status"];
  type?: Credential["type"];
  limit?: number;
  offset?: number;
}

export interface MetadataSearchResult {
  query: string;
  terms: string[];
  total: number;
  limit: number;
  offset: number;
  credentials: Credential[];
}

export interface MetadataIndexMaintenanceReport {
  removedCredentials: number;
  removedTokens: number;
  durationMs: number;
  runAt: number;
}

export interface MetadataIndexStats {
  indexedCredentials: number;
  distinctTokens: number;
  postingEntries: number;
  queryCount: number;
  avgLatencyMs: number;
  p95LatencyMs: number;
  lastRebuildAt?: number;
  lastMaintenanceAt?: number;
  lastMaintenance?: MetadataIndexMaintenanceReport;
}

/** Split text into lowercase searchable terms. Non-alphanumeric characters are separators. */
export function tokenize(text: string): string[] {
  const terms = new Set<string>();
  for (const part of text.toLowerCase().split(/[^\p{L}\p{N}]+/u)) {
    if (part.length > 0 && part.length <= MAX_TOKEN_LENGTH) terms.add(part);
  }
  return Array.from(terms);
}

/**
 * Flatten metadata into the terms that should be indexed: each key segment and each
 * leaf value, so `{ course: { level: "advanced" } }` yields `course`, `level`, `advanced`.
 */
export function extractMetadataTerms(metadata: Record<string, unknown>): string[] {
  const terms = new Set<string>();

  const visit = (value: unknown, depth: number): void => {
    if (terms.size >= MAX_TOKENS_PER_CREDENTIAL || depth > MAX_METADATA_DEPTH) return;
    if (value === null || value === undefined) return;
    if (Array.isArray(value)) {
      for (const item of value) visit(item, depth + 1);
      return;
    }
    if (typeof value === "object") {
      for (const [key, child] of Object.entries(value as Record<string, unknown>)) {
        for (const term of tokenize(key)) terms.add(term);
        visit(child, depth + 1);
      }
      return;
    }
    if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
      for (const term of tokenize(String(value))) terms.add(term);
    }
  };

  visit(metadata, 0);
  return Array.from(terms).slice(0, MAX_TOKENS_PER_CREDENTIAL);
}

type IndexSource = Pick<CredentialStore, "getCredential" | "getAllCredentials" | "onIssue">;

export class MetadataSearchIndex {
  /** term -> ids of credentials whose metadata contains it */
  private postings = new Map<string, Set<string>>();
  /** credential id -> terms it was indexed under (for removal) */
  private documentTerms = new Map<string, string[]>();
  private latencySamples: number[] = [];
  private queryCount = 0;
  private lastRebuildAt?: number;
  private lastMaintenanceAt?: number;
  private lastMaintenance?: MetadataIndexMaintenanceReport;
  private detach?: () => void;
  private maintenanceTimer?: ReturnType<typeof setInterval>;

  constructor(private readonly source: IndexSource) {}

  /** Index all current credentials and keep the index updated as new ones are issued. */
  attach(): void {
    if (this.detach) return;
    this.rebuild(this.source.getAllCredentials());
    this.detach = this.source.onIssue((credential) => this.indexCredential(credential));
  }

  /** Stop following the source store and stop any maintenance timer. */
  dispose(): void {
    this.detach?.();
    this.detach = undefined;
    this.stopMaintenance();
  }

  indexCredential(credential: Credential): void {
    this.removeCredential(credential.id);
    const terms = extractMetadataTerms(credential.metadata ?? {});
    for (const term of terms) {
      let ids = this.postings.get(term);
      if (!ids) {
        ids = new Set();
        this.postings.set(term, ids);
      }
      ids.add(credential.id);
    }
    this.documentTerms.set(credential.id, terms);
  }

  removeCredential(credentialId: string): void {
    const terms = this.documentTerms.get(credentialId);
    if (!terms) return;
    for (const term of terms) {
      const ids = this.postings.get(term);
      if (!ids) continue;
      ids.delete(credentialId);
      if (ids.size === 0) this.postings.delete(term);
    }
    this.documentTerms.delete(credentialId);
  }

  /** Replace the whole index with the given credentials. */
  rebuild(credentials: Iterable<Credential>): void {
    this.postings.clear();
    this.documentTerms.clear();
    for (const credential of credentials) this.indexCredential(credential);
    this.lastRebuildAt = Date.now();
  }

  /**
   * Maintenance: drop postings for credentials that no longer exist in the store and
   * remove terms left with no postings.
   */
  runMaintenance(): MetadataIndexMaintenanceReport {
    const started = performance.now();
    let removedCredentials = 0;
    for (const credentialId of Array.from(this.documentTerms.keys())) {
      if (!this.source.getCredential(credentialId)) {
        this.removeCredential(credentialId);
        removedCredentials++;
      }
    }

    let removedTokens = 0;
    for (const [term, ids] of Array.from(this.postings.entries())) {
      if (ids.size === 0) {
        this.postings.delete(term);
        removedTokens++;
      }
    }

    const report: MetadataIndexMaintenanceReport = {
      removedCredentials,
      removedTokens,
      durationMs: performance.now() - started,
      runAt: Date.now(),
    };
    this.lastMaintenanceAt = report.runAt;
    this.lastMaintenance = report;
    return report;
  }

  /** Run `runMaintenance` every `intervalMs`. The timer is unref'd so it never holds the process open. */
  startMaintenance(intervalMs: number): () => void {
    if (!Number.isFinite(intervalMs) || intervalMs <= 0) {
      throw new RangeError("maintenance interval must be a positive number of milliseconds");
    }
    this.stopMaintenance();
    this.maintenanceTimer = setInterval(() => this.runMaintenance(), intervalMs);
    this.maintenanceTimer.unref?.();
    return () => this.stopMaintenance();
  }

  stopMaintenance(): void {
    if (this.maintenanceTimer) clearInterval(this.maintenanceTimer);
    this.maintenanceTimer = undefined;
  }

  /**
   * Find credentials whose metadata contains every term in `query` (AND semantics).
   * Returns an error for queries with no searchable terms or too many of them.
   */
  search(
    query: string,
    options: MetadataSearchOptions = {}
  ): MetadataSearchResult | { error: string } {
    const terms = tokenize(query);
    if (terms.length === 0) return { error: "q must contain at least one searchable term" };
    if (terms.length > MAX_QUERY_TERMS) {
      return { error: `q cannot contain more than ${MAX_QUERY_TERMS} terms` };
    }

    const started = performance.now();
    const matchingIds = this.intersect(terms);
    let credentials: Credential[] = [];
    for (const id of matchingIds) {
      const credential = this.source.getCredential(id);
      if (!credential) continue;
      if (options.status && credential.status !== options.status) continue;
      if (options.type && credential.type !== options.type) continue;
      credentials.push(credential);
    }
    credentials.sort((a, b) => b.issuedAt - a.issuedAt);

    const limit = Math.min(Math.max(options.limit ?? DEFAULT_LIMIT, 1), MAX_LIMIT);
    const offset = Math.max(options.offset ?? 0, 0);
    const total = credentials.length;
    credentials = credentials.slice(offset, offset + limit);

    this.recordLatency(performance.now() - started);
    return { query, terms, total, limit, offset, credentials };
  }

  stats(): MetadataIndexStats {
    const sorted = [...this.latencySamples].sort((a, b) => a - b);
    const avg =
      sorted.length === 0 ? 0 : sorted.reduce((sum, value) => sum + value, 0) / sorted.length;
    const p95 = sorted.length === 0 ? 0 : sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * 0.95))]!;
    let postingEntries = 0;
    for (const ids of this.postings.values()) postingEntries += ids.size;
    return {
      indexedCredentials: this.documentTerms.size,
      distinctTokens: this.postings.size,
      postingEntries,
      queryCount: this.queryCount,
      avgLatencyMs: round(avg),
      p95LatencyMs: round(p95),
      lastRebuildAt: this.lastRebuildAt,
      lastMaintenanceAt: this.lastMaintenanceAt,
      lastMaintenance: this.lastMaintenance,
    };
  }

  private intersect(terms: string[]): string[] {
    const sets: Array<Set<string>> = [];
    for (const term of terms) {
      const ids = this.postings.get(term);
      if (!ids) return [];
      sets.push(ids);
    }
    sets.sort((a, b) => a.size - b.size);
    const [smallest, ...rest] = sets;
    if (!smallest) return [];
    return Array.from(smallest).filter((id) => rest.every((ids) => ids.has(id)));
  }

  private recordLatency(ms: number): void {
    this.queryCount++;
    this.latencySamples.push(ms);
    if (this.latencySamples.length > LATENCY_SAMPLE_SIZE) this.latencySamples.shift();
  }
}

function round(value: number): number {
  return Math.round(value * 1000) / 1000;
}

export const metadataSearchIndex = new MetadataSearchIndex(credentialStore);
metadataSearchIndex.attach();
