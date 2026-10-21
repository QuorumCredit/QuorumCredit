import { describe, it, expect } from "vitest";
import { AuditReportGenerator } from "../src/batch/auditReportGenerator.js";

describe("audit report ownership", () => {
  it("returns a report to the caller that generated it", () => {
    const generator = new AuditReportGenerator();
    const report = generator.generateReport([], "json", 0, Date.now(), "auditor-a");
    expect(generator.getFormattedReport(report.reportId, "json", "auditor-a")).not.toBeNull();
  });

  it("does not return another caller's report, and reports it as missing", () => {
    const generator = new AuditReportGenerator();
    const report = generator.generateReport([], "json", 0, Date.now(), "auditor-a");
    expect(generator.getFormattedReport(report.reportId, "json", "auditor-b")).toBeNull();
  });
});
