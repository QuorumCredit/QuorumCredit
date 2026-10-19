import { describe, it, expect } from "vitest";
import { LoanCartStore } from "../src/cart/loanCartStore.js";

describe("loan cart store", () => {
  it("keeps every item added to a borrower's cart", () => {
    const store = new LoanCartStore();
    store.addItem("GBORROWER1", 100, 3600);
    store.addItem("GBORROWER1", 200, 3600);
    expect(store.getCart("GBORROWER1")?.items).toHaveLength(2);
  });

  it("keeps carts for different borrowers separate", () => {
    const store = new LoanCartStore();
    store.addItem("GBORROWER1", 100, 3600);
    store.addItem("GBORROWER2", 300, 3600);
    expect(store.getCart("GBORROWER1")?.items).toHaveLength(1);
    expect(store.getCart("GBORROWER2")?.items).toHaveLength(1);
  });

  it("removes the cart when it is abandoned", () => {
    const store = new LoanCartStore();
    store.addItem("GBORROWER1", 100, 3600);
    expect(store.abandon("GBORROWER1")).toBe(true);
    expect(store.getCart("GBORROWER1")).toBeNull();
  });

  it("keeps every item when additions arrive concurrently", async () => {
    const store = new LoanCartStore();
    await Promise.all(
      Array.from({ length: 10 }, () => Promise.resolve().then(() => store.addItem("GBORROWER2", 1, 60)))
    );
    expect(store.getCart("GBORROWER2")?.items).toHaveLength(10);
  });
});
