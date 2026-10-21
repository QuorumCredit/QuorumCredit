import { describe, it, expect } from "vitest";
import { FacetedSearchService } from "../src/search/facetedSearch.js";

const events = [
  { id: 1, ledger: 10, ledgerClosedAt: "", txHash: "a", contractId: "c", category: "loan", action: "created", value: {} },
  { id: 2, ledger: 11, ledgerClosedAt: "", txHash: "b", contractId: "c", category: "loan", action: "repaid", value: {} },
  { id: 3, ledger: 12, ledgerClosedAt: "", txHash: "c", contractId: "c", category: "vouch", action: "created", value: {} },
];

describe("faceted search", () => {
  it("counts every event in the fixed dataset", () => {
    const service = new FacetedSearchService({ getEventsSince: () => events } as never);
    const result = service.search({});
    expect(result.total).toBe(3);
    expect(result.items).toHaveLength(3);
  });

  it("counts facet values by category", () => {
    const service = new FacetedSearchService({ getEventsSince: () => events } as never);
    const result = service.search({});
    const category = result.facets.find((f) => f.name === "category");
    expect(category?.values).toEqual(
      expect.arrayContaining([
        { value: "loan", count: 2 },
        { value: "vouch", count: 1 },
      ])
    );
  });

  it("returns no items for a category with no events", () => {
    const service = new FacetedSearchService({ getEventsSince: () => events } as never);
    expect(service.search({ category: "credential" }).total).toBe(0);
  });

  it("uses a search_ prefix for search IDs", () => {
    const service = new FacetedSearchService({ getEventsSince: () => events } as never);
    expect(service.search({}).searchId).toMatch(/^search_/);
  });
});
