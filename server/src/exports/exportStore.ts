/**
 * Issue #1584: Export store for managing credential holder export requests,
 * tracking exports, and supporting multiple export formats.
 */

export type ExportFormat = "json" | "pdf" | "csv";
export type ExportStatus = "pending" | "processing" | "completed" | "failed";

export interface CredentialData {
  credentialId: string;
  holder: string;
  issueDate: number;
  expiryDate: number;
  status: string;
  metadata?: Record<string, unknown>;
}

export interface ExportRequest {
  id: string;
  credentialId: string;
  format: ExportFormat;
  status: ExportStatus;
  requestedAt: number;
  completedAt?: number;
  expiresAt?: number;
  fileSize?: number;
  downloadUrl?: string;
  errorMessage?: string;
  retryCount: number;
  maxRetries: number;
  metadata?: Record<string, unknown>;
}

export interface ScheduledExport {
  id: string;
  credentialId: string;
  format: ExportFormat;
  frequency: "daily" | "weekly" | "monthly";
  startDate: number;
  nextExportAt: number;
  active: boolean;
  createdAt: number;
}

export class ExportStore {
  private exports: Map<string, ExportRequest> = new Map();
  private scheduledExports: Map<string, ScheduledExport> = new Map();
  private credentialData: Map<string, CredentialData> = new Map();
  private exportId = 0;
  private scheduledId = 0;

  /**
   * Create a new export request for a credential.
   */
  createExportRequest(
    credentialId: string,
    format: ExportFormat,
    metadata?: Record<string, unknown>
  ): ExportRequest {
    const request: ExportRequest = {
      id: `export_${++this.exportId}`,
      credentialId,
      format,
      status: "pending",
      requestedAt: Date.now(),
      expiresAt: Date.now() + 7 * 24 * 60 * 60 * 1000, // 7 days
      retryCount: 0,
      maxRetries: 3,
      metadata,
    };
    this.exports.set(request.id, request);
    return request;
  }

  /**
   * Get an export request by ID.
   */
  getExport(exportId: string): ExportRequest | undefined {
    return this.exports.get(exportId);
  }

  /**
   * Get all exports for a credential holder.
   */
  getCredentialExports(credentialId: string, limit: number = 50): ExportRequest[] {
    return [...this.exports.values()]
      .filter((e) => e.credentialId === credentialId)
      .sort((a, b) => b.requestedAt - a.requestedAt)
      .slice(0, limit);
  }

  /**
   * Mark an export as processing.
   */
  markAsProcessing(exportId: string): void {
    const exp = this.exports.get(exportId);
    if (exp) {
      exp.status = "processing";
    }
  }

  /**
   * Mark an export as completed with a download URL.
   */
  markAsCompleted(exportId: string, downloadUrl: string, fileSize: number): void {
    const exp = this.exports.get(exportId);
    if (exp) {
      exp.status = "completed";
      exp.completedAt = Date.now();
      exp.downloadUrl = downloadUrl;
      exp.fileSize = fileSize;
    }
  }

  /**
   * Mark an export as failed and increment retry count.
   */
  markAsFailed(exportId: string, errorMessage: string): void {
    const exp = this.exports.get(exportId);
    if (exp) {
      exp.retryCount++;
      exp.errorMessage = errorMessage;
      if (exp.retryCount >= exp.maxRetries) {
        exp.status = "failed";
      } else {
        exp.status = "pending";
      }
    }
  }

  /**
   * Get pending exports for processing.
   */
  getPendingExports(limit: number = 100): ExportRequest[] {
    return [...this.exports.values()]
      .filter(
        (e) =>
          e.status === "pending" &&
          e.retryCount < e.maxRetries &&
          (!e.expiresAt || e.expiresAt > Date.now())
      )
      .slice(0, limit);
  }

  /**
   * Store credential data for export.
   */
  storeCredentialData(credentialId: string, data: CredentialData): void {
    this.credentialData.set(credentialId, data);
  }

  /**
   * Get credential data for export.
   */
  getCredentialData(credentialId: string): CredentialData | undefined {
    return this.credentialData.get(credentialId);
  }

  /**
   * Create a scheduled export request.
   */
  createScheduledExport(
    credentialId: string,
    format: ExportFormat,
    frequency: "daily" | "weekly" | "monthly",
    startDate: number = Date.now()
  ): ScheduledExport {
    const frequencyMs =
      frequency === "daily"
        ? 24 * 60 * 60 * 1000
        : frequency === "weekly"
          ? 7 * 24 * 60 * 60 * 1000
          : 30 * 24 * 60 * 60 * 1000;

    const scheduled: ScheduledExport = {
      id: `scheduled_export_${++this.scheduledId}`,
      credentialId,
      format,
      frequency,
      startDate,
      nextExportAt: startDate + frequencyMs,
      active: true,
      createdAt: Date.now(),
    };
    this.scheduledExports.set(scheduled.id, scheduled);
    return scheduled;
  }

  /**
   * Get a scheduled export by ID.
   */
  getScheduledExport(scheduledId: string): ScheduledExport | undefined {
    return this.scheduledExports.get(scheduledId);
  }

  /**
   * Get scheduled exports for a credential holder.
   */
  getCredentialScheduledExports(credentialId: string): ScheduledExport[] {
    return [...this.scheduledExports.values()].filter(
      (se) => se.credentialId === credentialId
    );
  }

  /**
   * Get due scheduled exports that need to be triggered.
   */
  getDueScheduledExports(): ScheduledExport[] {
    const now = Date.now();
    return [...this.scheduledExports.values()].filter(
      (se) => se.active && se.nextExportAt <= now
    );
  }

  /**
   * Update next export time for a scheduled export.
   */
  updateNextExportTime(scheduledId: string): void {
    const scheduled = this.scheduledExports.get(scheduledId);
    if (scheduled) {
      const frequencyMs =
        scheduled.frequency === "daily"
          ? 24 * 60 * 60 * 1000
          : scheduled.frequency === "weekly"
            ? 7 * 24 * 60 * 60 * 1000
            : 30 * 24 * 60 * 60 * 1000;
      scheduled.nextExportAt = Date.now() + frequencyMs;
    }
  }

  /**
   * Disable a scheduled export.
   */
  disableScheduledExport(scheduledId: string): void {
    const scheduled = this.scheduledExports.get(scheduledId);
    if (scheduled) {
      scheduled.active = false;
    }
  }

  /**
   * Get export statistics.
   */
  getStatistics(): {
    totalExports: number;
    completedExports: number;
    failedExports: number;
    pendingExports: number;
    totalScheduledExports: number;
    activeScheduledExports: number;
    totalDataExported: number;
  } {
    const exports = [...this.exports.values()];
    const scheduledExports = [...this.scheduledExports.values()];

    return {
      totalExports: exports.length,
      completedExports: exports.filter((e) => e.status === "completed").length,
      failedExports: exports.filter((e) => e.status === "failed").length,
      pendingExports: exports.filter((e) => e.status === "pending").length,
      totalScheduledExports: scheduledExports.length,
      activeScheduledExports: scheduledExports.filter((se) => se.active).length,
      totalDataExported: exports.reduce((sum, e) => sum + (e.fileSize || 0), 0),
    };
  }

  /**
   * Clean up expired exports.
   */
  cleanupExpiredExports(): number {
    const now = Date.now();
    let cleaned = 0;

    for (const [id, exp] of this.exports) {
      if (exp.expiresAt && exp.expiresAt < now && exp.status === "completed") {
        this.exports.delete(id);
        cleaned++;
      }
    }

    return cleaned;
  }
}

// Singleton instance
export const exportStore = new ExportStore();
