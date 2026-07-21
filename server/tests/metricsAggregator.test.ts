import { describe, it, expect } from "vitest";
import { MetricsAggregator } from "../src/bridge/metricsAggregator.js";
import type { IndexedEvent } from "../src/types.js";

function ev(partial: Partial<IndexedEvent> & Pick<IndexedEvent, "id" | "category" | "action">): IndexedEvent {
  return {
    ledger: 1,
    ledgerClosedAt: new Date().toISOString(),
    txHash: "tx",
    contractId: "C...",
    value: {},
    ...partial,
  };
}

describe("MetricsAggregator", () => {
  it("counts a loan request as a new active loan and adds to TVL", () => {
    const agg = new MetricsAggregator();
    const snap = agg.applyEvent(
      ev({ id: 1, category: "loan", action: "request", value: { borrower: "B1", amount_stroops: 1000 } })
    );
    expect(snap.total_loans).toBe(1);
    expect(snap.active_loans).toBe(1);
    expect(snap.tvl).toBe(1000);
    expect(snap.top_borrowers).toEqual([["B1", 1000]]);
  });

  it("reduces active loans and principal on repay", () => {
    const agg = new MetricsAggregator();
    agg.applyEvent(ev({ id: 1, category: "loan", action: "request", value: { borrower: "B1", amount_stroops: 1000 } }));
    const snap = agg.applyEvent(
      ev({ id: 2, category: "loan", action: "repay", value: { borrower: "B1", payment_stroops: 1000 } })
    );
    expect(snap.active_loans).toBe(0);
    expect(snap.total_yield_distributed).toBe(1000);
  });

  it("counts a slash as a default and reduces active loans", () => {
    const agg = new MetricsAggregator();
    agg.applyEvent(ev({ id: 1, category: "loan", action: "request", value: { borrower: "B1", amount_stroops: 1000 } }));
    const snap = agg.applyEvent(
      ev({ id: 2, category: "loan", action: "slash", value: { borrower: "B1", total_slashed_stroops: 500 } })
    );
    expect(snap.active_loans).toBe(0);
    expect(snap.defaulted_loans).toBe(1);
    expect(snap.default_rate).toBe(1);
  });

  it("never lets active_loans or tvl go negative on unmatched repay/slash", () => {
    const agg = new MetricsAggregator();
    const snap = agg.applyEvent(
      ev({ id: 1, category: "loan", action: "repay", value: { borrower: "B1", payment_stroops: 999 } })
    );
    expect(snap.active_loans).toBe(0);
    expect(snap.tvl).toBeGreaterThanOrEqual(0);
  });

  it("tracks voucher stake through create/increase/decrease/withdraw", () => {
    const agg = new MetricsAggregator();
    agg.applyEvent(ev({ id: 1, category: "vouch", action: "create", value: { voucher: "V1", stake_stroops: 100 } }));
    agg.applyEvent(ev({ id: 2, category: "vouch", action: "increase", value: { voucher: "V1", stake_stroops: 50 } }));
    let snap = agg.applyEvent(
      ev({ id: 3, category: "vouch", action: "decrease", value: { voucher: "V1", stake_stroops: 30 } })
    );
    expect(snap.tvl).toBe(120); // 100 + 50 - 30
    snap = agg.applyEvent(ev({ id: 4, category: "vouch", action: "withdraw", value: { voucher: "V1" } }));
    expect(snap.tvl).toBe(0);
  });
});
