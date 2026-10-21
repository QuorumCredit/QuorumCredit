import { describe, it, expect } from "vitest";
import { ComplexityScorer } from "../src/verification/complexityScorer.js";

const base = {
  credentialId: "cred-1",
  method: "document",
  dataFields: 5,
  requiresBiometric: false,
  requiresManualReview: false,
  documentCount: 2,
  createdAt: 0,
};

describe("complexity scorer", () => {
  it("keeps the component scores in the 1 to 10 range for minimal, typical and maximal inputs", () => {
    const scorer = new ComplexityScorer();
    const inputs = [
      { ...base, method: "automated", dataFields: 0, documentCount: 0 },
      base,
      { ...base, method: "manual", dataFields: 1000, documentCount: 100, requiresBiometric: true, requiresManualReview: true },
    ];
    for (const input of inputs) {
      const score = scorer.scoreVerification(input);
      for (const component of [score.methodComplexity, score.dataComplexity, score.processComplexity]) {
        expect(component).toBeGreaterThanOrEqual(1);
        expect(component).toBeLessThanOrEqual(10);
      }
    }
  });

  it("rejects a negative data field count", () => {
    const scorer = new ComplexityScorer();
    expect(() => scorer.scoreVerification({ ...base, dataFields: -1 })).toThrow(/dataFields/);
  });

  it("rejects a NaN document count", () => {
    const scorer = new ComplexityScorer();
    expect(() => scorer.scoreVerification({ ...base, documentCount: Number.NaN })).toThrow(/documentCount/);
  });
});
