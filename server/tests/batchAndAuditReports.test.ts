import { describe, it, expect } from "vitest";
import { BatchVerificationStore } from "../src/batch/batchVerificationStore.js";
import { AuditReportGenerator } from "../src/batch/auditReportGenerator.js";

describe("batch verification status", () => {
  it("moves from in_progress to completed as items are verified", () => {
    const store = new BatchVerificationStore();
    store.createBatch("batch-states", "GBORROWER1", ["cred-1", "cred-2"]);
    expect(store.getBatch("batch-states")?.status).toBe("in_progress");

    store.updateItemStatus("batch-states", "cred-1", "verified", { isValid: true } as never);
    expect(store.getBatch("batch-states")?.status).toBe("in_progress");

    store.updateItemStatus("batch-states", "cred-2", "verified", { isValid: true } as never);
    expect(store.getBatch("batch-states")?.status).toBe("completed");
    expect(store.getProgress("batch-states")).toBe(100);
  });
});

describe("audit report output", () => {
  it("summarises a fixed set of batches", () => {
    const store = new BatchVerificationStore();
    store.createBatch("batch-a", "GBORROWER1", ["cred-a"]);
    store.updateItemStatus("batch-a", "cred-a", "verified", { isValid: true } as never);
    store.createBatch("batch-b", "GBORROWER2", ["cred-b"]);
    const batches = [store.getBatch("batch-a")!, store.getBatch("batch-b")!];

    const report = new AuditReportGenerator().generateReport(batches, "csv", 0, Date.now());
    expect(report.totalBatches).toBe(2);
    const csv = new AuditReportGenerator().formatAsCsv(report);
    expect(csv).toContain("Credential Verification Audit Report");
  });
});
