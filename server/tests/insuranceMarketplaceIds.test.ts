import { describe, it, expect, vi, afterEach } from "vitest";
import { addProduct, submitClaim } from "../src/insurance-marketplace.js";

describe("insurance claim IDs", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("gives two claims created in the same millisecond different IDs", () => {
    vi.spyOn(Date, "now").mockReturnValue(1_700_000_000_000);
    addProduct({
      id: "product-ids",
      providerId: "provider-ids",
      name: "Test product",
      description: "Product used by the ID test",
      coveragePercentage: 50,
      premiumBps: 100,
      maxClaimAmount: 1_000_000,
      claimsProcessingDays: 3,
      active: true,
      createdAt: 0,
    });

    const first = submitClaim(1, "GBORROWER", "product-ids", 10);
    const second = submitClaim(1, "GBORROWER", "product-ids", 10);

    expect(first.id).toMatch(/^claim_/);
    expect(first.id).not.toBe(second.id);
  });
});
