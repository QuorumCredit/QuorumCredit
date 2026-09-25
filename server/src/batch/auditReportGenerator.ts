/**
 * Credential Verification Audit Report Generation (Issue #1595)
 *
 * Generates compliance-grade audit reports for credential verification operations.
 * Supports multiple formats (JSON, CSV, PDF) and scheduled report generation.
 */

import type { BatchVerification } from "./batchVerificationStore.js";

export type ReportFormat = "json" | "csv" | "pdf";

export interface VerificationAuditReport {
  reportId: string;
  generatedAt: number;
  periodStart: number;
  periodEnd: number;
  format: ReportFormat;
  totalBatches: number;
  successCount: number;
  failureCount: number;
  averageSuccessRate: number;
  averageFailureRate: number;
  totalCredentialsVerified: number;
  totalCredentialsFailed: number;
  byBorrower: Map<string, BorrowerAuditSummary>;
  issues?: string[];
}

export interface BorrowerAuditSummary {
  borrowerId: string;
  batchCount: number;
  totalCredentials: number;
  successCount: number;
  failureCount: number;
  successRate: number;
  lastBatchDate: number;
}

export interface ReportSchedule {
  scheduleId: string;
  format: ReportFormat;
  frequency: "daily" | "weekly" | "monthly";
  recipients: string[];
  enabled: boolean;
  createdAt: number;
  lastRun?: number;
  nextRun?: number;
}

/**
 * Generates audit reports for credential verification operations.
 * Supports multiple output formats and compliance-grade reporting.
 */
export class AuditReportGenerator {
  private readonly schedules = new Map<string, ReportSchedule>();
  private readonly reports = new Map<string, VerificationAuditReport>();

  /**
   * Generate an audit report for a time period.
   */
  generateReport(
    batches: BatchVerification[],
    format: ReportFormat = "json",
    periodStart?: number,
    periodEnd?: number
  ): VerificationAuditReport {
    const now = Date.now();
    const start = periodStart || now - 30 * 24 * 60 * 60 * 1000; // Last 30 days
    const end = periodEnd || now;

    const filteredBatches = batches.filter(
      (b) => b.createdAt >= start && b.createdAt <= end
    );

    const byBorrower = new Map<string, BorrowerAuditSummary>();
    let totalCredentialsVerified = 0;
    let totalCredentialsFailed = 0;
    let totalSuccessCount = 0;
    let totalFailureCount = 0;

    // Aggregate batch data
    for (const batch of filteredBatches) {
      if (!byBorrower.has(batch.borrowerId)) {
        byBorrower.set(batch.borrowerId, {
          borrowerId: batch.borrowerId,
          batchCount: 0,
          totalCredentials: 0,
          successCount: 0,
          failureCount: 0,
          successRate: 0,
          lastBatchDate: 0,
        });
      }

      const borrowerSummary = byBorrower.get(batch.borrowerId)!;
      borrowerSummary.batchCount += 1;
      borrowerSummary.totalCredentials += batch.totalItems;
      borrowerSummary.successCount += batch.verifiedCount;
      borrowerSummary.failureCount += batch.failedCount;
      borrowerSummary.lastBatchDate = Math.max(
        borrowerSummary.lastBatchDate,
        batch.createdAt
      );

      totalCredentialsVerified += batch.verifiedCount;
      totalCredentialsFailed += batch.failedCount;
      totalSuccessCount += batch.verifiedCount;
      totalFailureCount += batch.failedCount;
    }

    // Calculate success rates
    for (const summary of byBorrower.values()) {
      if (summary.totalCredentials > 0) {
        summary.successRate = Math.round(
          (summary.successCount / summary.totalCredentials) * 100
        );
      }
    }

    const totalCredentials = totalSuccessCount + totalFailureCount;
    const averageSuccessRate =
      totalCredentials > 0
        ? Math.round((totalSuccessCount / totalCredentials) * 100)
        : 0;
    const averageFailureRate = 100 - averageSuccessRate;

    const reportId = `report_${Date.now()}_${Math.random()
      .toString(36)
      .substr(2, 9)}`;

    const report: VerificationAuditReport = {
      reportId,
      generatedAt: now,
      periodStart: start,
      periodEnd: end,
      format,
      totalBatches: filteredBatches.length,
      successCount: totalSuccessCount,
      failureCount: totalFailureCount,
      averageSuccessRate,
      averageFailureRate,
      totalCredentialsVerified,
      totalCredentialsFailed,
      byBorrower,
    };

    this.reports.set(reportId, report);
    return report;
  }

  /**
   * Format report as JSON.
   */
  formatAsJson(report: VerificationAuditReport): string {
    const byBorrowerObj = Object.fromEntries(report.byBorrower);
    return JSON.stringify(
      {
        ...report,
        byBorrower: byBorrowerObj,
      },
      null,
      2
    );
  }

  /**
   * Format report as CSV.
   */
  formatAsCsv(report: VerificationAuditReport): string {
    const lines: string[] = [];

    // Header with report metadata
    lines.push("Credential Verification Audit Report");
    lines.push(`Report ID,${report.reportId}`);
    lines.push(`Generated At,${new Date(report.generatedAt).toISOString()}`);
    lines.push(
      `Period,"${new Date(report.periodStart).toISOString()} to ${new Date(
        report.periodEnd
      ).toISOString()}"`
    );
    lines.push("");

    // Summary section
    lines.push("Summary Metrics");
    lines.push(`Total Batches,${report.totalBatches}`);
    lines.push(`Total Credentials Verified,${report.totalCredentialsVerified}`);
    lines.push(`Total Credentials Failed,${report.totalCredentialsFailed}`);
    lines.push(`Average Success Rate,${report.averageSuccessRate}%`);
    lines.push(`Average Failure Rate,${report.averageFailureRate}%`);
    lines.push("");

    // By Borrower section
    lines.push("By Borrower");
    lines.push(
      "Borrower ID,Batch Count,Total Credentials,Success Count,Failure Count,Success Rate (%),Last Batch Date"
    );
    for (const [, summary] of report.byBorrower) {
      lines.push(
        `${summary.borrowerId},${summary.batchCount},${summary.totalCredentials},${summary.successCount},${summary.failureCount},${summary.successRate},${new Date(
          summary.lastBatchDate
        ).toISOString()}`
      );
    }

    return lines.join("\n");
  }

  /**
   * Format report as PDF (returns base64 encoded PDF or placeholder).
   * Note: Full PDF generation requires an external library.
   * This provides a structured format that can be passed to a PDF service.
   */
  formatAsPdf(report: VerificationAuditReport): string {
    // In production, this would generate a real PDF using a library like pdfkit
    // For now, return a structured format suitable for conversion
    const pdfContent = {
      title: "Credential Verification Audit Report",
      reportId: report.reportId,
      generatedAt: new Date(report.generatedAt).toISOString(),
      period: {
        start: new Date(report.periodStart).toISOString(),
        end: new Date(report.periodEnd).toISOString(),
      },
      summary: {
        totalBatches: report.totalBatches,
        totalCredentialsVerified: report.totalCredentialsVerified,
        totalCredentialsFailed: report.totalCredentialsFailed,
        averageSuccessRate: `${report.averageSuccessRate}%`,
        averageFailureRate: `${report.averageFailureRate}%`,
      },
      byBorrower: Array.from(report.byBorrower.values()),
    };

    // Return JSON representation that can be converted to PDF
    return JSON.stringify(pdfContent, null, 2);
  }

  /**
   * Get formatted report in the specified format.
   */
  getFormattedReport(reportId: string, format: ReportFormat): string | null {
    const report = this.reports.get(reportId);
    if (!report) return null;

    switch (format) {
      case "json":
        return this.formatAsJson(report);
      case "csv":
        return this.formatAsCsv(report);
      case "pdf":
        return this.formatAsPdf(report);
      default:
        return null;
    }
  }

  /**
   * Create a scheduled report.
   */
  createSchedule(
    format: ReportFormat,
    frequency: "daily" | "weekly" | "monthly",
    recipients: string[]
  ): ReportSchedule {
    const scheduleId = `schedule_${Date.now()}_${Math.random()
      .toString(36)
      .substr(2, 9)}`;

    const schedule: ReportSchedule = {
      scheduleId,
      format,
      frequency,
      recipients,
      enabled: true,
      createdAt: Date.now(),
    };

    // Calculate next run
    schedule.nextRun = this.calculateNextRun(frequency);

    this.schedules.set(scheduleId, schedule);
    return schedule;
  }

  /**
   * Get all schedules.
   */
  getSchedules(): ReportSchedule[] {
    return Array.from(this.schedules.values());
  }

  /**
   * Update a schedule.
   */
  updateSchedule(
    scheduleId: string,
    updates: Partial<ReportSchedule>
  ): ReportSchedule | null {
    const schedule = this.schedules.get(scheduleId);
    if (!schedule) return null;

    Object.assign(schedule, updates);
    return schedule;
  }

  /**
   * Delete a schedule.
   */
  deleteSchedule(scheduleId: string): boolean {
    return this.schedules.delete(scheduleId);
  }

  /**
   * Record schedule execution.
   */
  recordScheduleExecution(scheduleId: string): boolean {
    const schedule = this.schedules.get(scheduleId);
    if (!schedule) return false;

    schedule.lastRun = Date.now();
    schedule.nextRun = this.calculateNextRun(schedule.frequency);
    return true;
  }

  /**
   * Calculate next run time based on frequency.
   */
  private calculateNextRun(frequency: string): number {
    const now = Date.now();
    switch (frequency) {
      case "daily":
        return now + 24 * 60 * 60 * 1000;
      case "weekly":
        return now + 7 * 24 * 60 * 60 * 1000;
      case "monthly":
        return now + 30 * 24 * 60 * 60 * 1000;
      default:
        return now;
    }
  }

  /**
   * Get report history.
   */
  getReportHistory(limit: number = 50): VerificationAuditReport[] {
    return Array.from(this.reports.values())
      .sort((a, b) => b.generatedAt - a.generatedAt)
      .slice(0, limit);
  }

  /**
   * Clear old reports (older than retentionDays).
   */
  clearOldReports(retentionDays: number = 90): number {
    const cutoff = Date.now() - retentionDays * 24 * 60 * 60 * 1000;
    let count = 0;

    for (const [reportId, report] of this.reports) {
      if (report.generatedAt < cutoff) {
        this.reports.delete(reportId);
        count++;
      }
    }

    return count;
  }
}

// Singleton instance
export const auditReportGenerator = new AuditReportGenerator();
