import { describe, it, expect } from "vitest";
import { CostOptimizer } from "../src/costs/costOptimizer.js";
import type { CostReport } from "../src/costs/costAllocator.js";

const emptyReport: CostReport = {
  generatedAt: 0,
  period: "2026-01",
  totalTransactionCount: 0,
  totalCostCents: 0,
  contractFeesIncludedInTotal: false,
  features: [],
  optimizationHints: [],
};

describe("cost optimizer", () => {
  it("returns no recommendations for empty input rather than an error", () => {
    const optimizer = new CostOptimizer();
    const analysis = optimizer.analyzeCosts(emptyReport);
    expect(analysis.recommendations).toEqual([]);
  });

  it("averages cost per transaction from a fixed report", () => {
    const optimizer = new CostOptimizer();
    const report: CostReport = { ...emptyReport, totalTransactionCount: 100, totalCostCents: 500 };
    optimizer.addReport(report);
    const analysis = optimizer.analyzeCosts(report);
    expect(analysis.averageCostPerTransactionCents).toBe(5);
  });
});
