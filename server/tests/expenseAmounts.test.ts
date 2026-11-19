import { describe, expect, it } from "vitest";
import { ExpenseStore, isValidExpenseAmount } from "../src/expenses/expenseStore.js";

describe("expense amounts (integer minor units)", () => {
  it("rejects fractional, non-finite and non-positive amounts", () => {
    expect(isValidExpenseAmount(0.1)).toBe(false);
    expect(isValidExpenseAmount(1.5)).toBe(false);
    expect(isValidExpenseAmount(0)).toBe(false);
    expect(isValidExpenseAmount(-5)).toBe(false);
    expect(isValidExpenseAmount(Number.NaN)).toBe(false);
    expect(isValidExpenseAmount(Number.POSITIVE_INFINITY)).toBe(false);
    expect(isValidExpenseAmount(Number.MAX_SAFE_INTEGER + 1)).toBe(false);
    expect(isValidExpenseAmount("10")).toBe(false);
  });

  it("accepts positive integer minor units", () => {
    expect(isValidExpenseAmount(1)).toBe(true);
    expect(isValidExpenseAmount(10)).toBe(true);
  });

  it("totals ten minor units recorded three times exactly", () => {
    const store = new ExpenseStore();
    for (let i = 0; i < 3; i++) {
      store.addExpense("loan-1", "business", 10, "supplies");
    }
    const breakdown = store.breakdown("loan-1");
    expect(breakdown.totalRecorded).toBe(30);
    expect(breakdown.byCategory.business).toBe(30);
    expect(breakdown.expenseCount).toBe(3);
  });

  it("keeps totals exact across many small amounts (no rounding drift)", () => {
    const store = new ExpenseStore();
    for (let i = 0; i < 1000; i++) {
      store.addExpense("loan-2", "healthcare", 1, "copay");
    }
    expect(store.breakdown("loan-2").totalRecorded).toBe(1000);
  });

  it("refuses to store a fractional amount", () => {
    const store = new ExpenseStore();
    expect(() => store.addExpense("loan-3", "other", 0.1, "bad")).toThrow(RangeError);
    expect(store.listExpenses("loan-3")).toEqual([]);
  });
});
