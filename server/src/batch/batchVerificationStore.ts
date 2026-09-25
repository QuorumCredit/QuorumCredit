/**
 * Batch Credential Verification Status Tracking (Issue #1594)
 *
 * Tracks batch verification operations, their progress, and results.
 * Supports polling, caching, and webhook notifications upon completion.
 */

export interface VerificationItem {
  credentialId: string;
  status: 'pending' | 'verified' | 'failed';
  result?: {
    isValid: boolean;
    timestamp: number;
    errorCode?: string;
    errorMessage?: string;
  };
  addedAt: number;
}

export interface BatchVerification {
  batchId: string;
  borrowerId: string;
  items: VerificationItem[];
  status: 'in_progress' | 'completed' | 'cancelled';
  totalItems: number;
  verifiedCount: number;
  failedCount: number;
  createdAt: number;
  completedAt?: number;
  webhookUrl?: string;
}

export interface BatchVerificationCache {
  data: Map<string, BatchVerification>;
  ttlMs: number;
  lastCleanup: number;
}

const DEFAULT_CACHE_TTL_MS = 86400000; // 24 hours
const CLEANUP_INTERVAL_MS = 3600000; // 1 hour

/**
 * In-memory batch verification status store with caching and webhook support.
 * Tracks the progress of credential verification batches.
 */
export class BatchVerificationStore {
  private readonly batches = new Map<string, BatchVerification>();
  private readonly cache: BatchVerificationCache = {
    data: new Map(),
    ttlMs: DEFAULT_CACHE_TTL_MS,
    lastCleanup: Date.now(),
  };
  private readonly webhookCallbacks = new Map<string, (batch: BatchVerification) => Promise<void>>();

  /**
   * Create a new batch verification operation.
   */
  createBatch(
    batchId: string,
    borrowerId: string,
    credentialIds: string[],
    webhookUrl?: string
  ): BatchVerification {
    const items = credentialIds.map((id) => ({
      credentialId: id,
      status: 'pending' as const,
      addedAt: Date.now(),
    }));

    const batch: BatchVerification = {
      batchId,
      borrowerId,
      items,
      status: 'in_progress',
      totalItems: credentialIds.length,
      verifiedCount: 0,
      failedCount: 0,
      createdAt: Date.now(),
      webhookUrl,
    };

    this.batches.set(batchId, batch);
    this.cache.data.set(batchId, batch);
    return batch;
  }

  /**
   * Update the status of a verification item.
   */
  updateItemStatus(
    batchId: string,
    credentialId: string,
    status: VerificationItem['status'],
    result?: VerificationItem['result']
  ): BatchVerification | null {
    const batch = this.batches.get(batchId);
    if (!batch) return null;

    const item = batch.items.find((i) => i.credentialId === credentialId);
    if (!item) return null;

    item.status = status;
    if (result) {
      item.result = result;
    }

    // Update batch counts
    batch.verifiedCount = batch.items.filter((i) => i.status === 'verified').length;
    batch.failedCount = batch.items.filter((i) => i.status === 'failed').length;

    // Check if batch is complete
    if (batch.verifiedCount + batch.failedCount === batch.totalItems) {
      this.completeBatch(batchId);
    }

    this.cache.data.set(batchId, batch);
    return batch;
  }

  /**
   * Get batch status.
   */
  getBatch(batchId: string): BatchVerification | null {
    // Try cache first
    const cachedBatch = this.cache.data.get(batchId);
    if (cachedBatch) {
      return cachedBatch;
    }

    return this.batches.get(batchId) || null;
  }

  /**
   * Get all batches for a borrower.
   */
  getBorrowerbatches(borrowerId: string, limit: number = 50): BatchVerification[] {
    return Array.from(this.batches.values())
      .filter((b) => b.borrowerId === borrowerId)
      .sort((a, b) => b.createdAt - a.createdAt)
      .slice(0, limit);
  }

  /**
   * Get batch progress as a percentage.
   */
  getProgress(batchId: string): number | null {
    const batch = this.getBatch(batchId);
    if (!batch || batch.totalItems === 0) return null;
    return Math.round(((batch.verifiedCount + batch.failedCount) / batch.totalItems) * 100);
  }

  /**
   * Register a webhook callback for batch completion.
   */
  registerWebhookCallback(
    batchId: string,
    callback: (batch: BatchVerification) => Promise<void>
  ): void {
    this.webhookCallbacks.set(batchId, callback);
  }

  /**
   * Mark batch as completed and trigger webhook.
   */
  private async completeBatch(batchId: string): Promise<void> {
    const batch = this.batches.get(batchId);
    if (!batch) return;

    batch.status = 'completed';
    batch.completedAt = Date.now();
    this.cache.data.set(batchId, batch);

    // Trigger webhook callback if registered
    const callback = this.webhookCallbacks.get(batchId);
    if (callback) {
      try {
        await callback(batch);
      } catch (err) {
        console.error(`Failed to execute webhook for batch ${batchId}:`, err);
      }
      this.webhookCallbacks.delete(batchId);
    }
  }

  /**
   * Cancel a batch operation.
   */
  cancelBatch(batchId: string): BatchVerification | null {
    const batch = this.batches.get(batchId);
    if (!batch) return null;

    batch.status = 'cancelled';
    this.cache.data.set(batchId, batch);
    return batch;
  }

  /**
   * Clean up expired cache entries (older than TTL).
   */
  cleanup(): void {
    const now = Date.now();
    if (now - this.cache.lastCleanup < CLEANUP_INTERVAL_MS) {
      return;
    }

    const toDelete: string[] = [];
    this.cache.data.forEach((batch, batchId) => {
      if (batch.status === 'completed' && now - (batch.completedAt || 0) > this.cache.ttlMs) {
        toDelete.push(batchId);
      }
    });

    toDelete.forEach((id) => this.cache.data.delete(id));
    this.cache.lastCleanup = now;
  }

  /**
   * Get cache statistics.
   */
  getCacheStats() {
    return {
      totalBatches: this.batches.size,
      cachedBatches: this.cache.data.size,
      webhookCallbacksRegistered: this.webhookCallbacks.size,
    };
  }

  /**
   * Export batch results for reporting.
   */
  exportBatchResults(batchId: string): {
    batch: BatchVerification;
    successRate: number;
    failureRate: number;
  } | null {
    const batch = this.getBatch(batchId);
    if (!batch) return null;

    const successRate =
      batch.totalItems > 0 ? (batch.verifiedCount / batch.totalItems) * 100 : 0;
    const failureRate =
      batch.totalItems > 0 ? (batch.failedCount / batch.totalItems) * 100 : 0;

    return {
      batch,
      successRate: Math.round(successRate),
      failureRate: Math.round(failureRate),
    };
  }
}

// Singleton instance
export const batchVerificationStore = new BatchVerificationStore();
