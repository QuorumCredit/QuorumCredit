import { describe, it, expect } from "vitest";
import { addProduct, submitClaim, approveClaim, rejectClaim } from "../src/insurance-marketplace.js";

addProduct({
  id: "product-flow",
  providerId: "provider-flow",
  name: "Flow product",
  description: "Product for the claim flow tests",
  coveragePercentage: 50,
  premiumBps: 100,
  maxClaimAmount: 1000,
  claimsProcessingDays: 3,
  active: true,
  createdAt: 0,
});

describe("insurance claim flow", () => {
  it("refuses a claim above the product's maximum coverage", () => {
    expect(() => submitClaim(10, "GBORROWER", "product-flow", 5000)).toThrow(
      "Claim amount exceeds maximum coverage"
    );
  });

  it("approves a claim within coverage", () => {
    const claim = submitClaim(11, "GBORROWER", "product-flow", 500);
    expect(claim.status).toBe("pending");
    expect(approveClaim(claim.id).status).toBe("approved");
  });

  it("rejects a claim with a reason", () => {
    const claim = submitClaim(12, "GBORROWER", "product-flow", 500);
    expect(rejectClaim(claim.id, "duplicate claim").status).toBe("rejected");
  });
});
