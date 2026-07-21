import { describe, it, expect } from "vitest";
import { LoanProjector } from "../src/bridge/loanProjector.js";
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

describe("LoanProjector", () => {
  it("ignores non-loan categories", () => {
    const p = new LoanProjector();
    const result = p.applyEvent(ev({ id: 1, category: "vouch", action: "create", value: { borrower: "B1" } }));
    expect(result).toBeNull();
  });

  it("projects a loan/request into an Active LoanRecord", () => {
    const p = new LoanProjector();
    const loan = p.applyEvent(
      ev({
        id: 1,
        category: "loan",
        action: "request",
        value: { borrower: "B1", amount_stroops: 5000, loan_purpose: "business" },
      })
    );
    expect(loan).not.toBeNull();
    expect(loan?.status).toBe("Active");
    expect(loan?.amount).toBe(5000);
    expect(loan?.loan_purpose).toBe("business");
  });

  it("keeps the same synthetic id across a borrower's lifecycle events", () => {
    const p = new LoanProjector();
    const requested = p.applyEvent(
      ev({ id: 1, category: "loan", action: "request", value: { borrower: "B1", amount_stroops: 1000 } })
    );
    const repaid = p.applyEvent(
      ev({ id: 2, category: "loan", action: "repay", value: { borrower: "B1", payment_stroops: 1000 } })
    );
    expect(repaid?.id).toBe(requested?.id);
    expect(repaid?.status).toBe("Repaid");
  });

  it("assigns different ids to different borrowers", () => {
    const p = new LoanProjector();
    const a = p.applyEvent(
      ev({ id: 1, category: "loan", action: "request", value: { borrower: "B1", amount_stroops: 1000 } })
    );
    const b = p.applyEvent(
      ev({ id: 2, category: "loan", action: "request", value: { borrower: "B2", amount_stroops: 1000 } })
    );
    expect(a?.id).not.toBe(b?.id);
  });

  it("marks a slash as Defaulted without discarding known fields", () => {
    const p = new LoanProjector();
    p.applyEvent(
      ev({
        id: 1,
        category: "loan",
        action: "request",
        value: { borrower: "B1", amount_stroops: 1000, loan_purpose: "farming" },
      })
    );
    const slashed = p.applyEvent(ev({ id: 2, category: "loan", action: "slash", value: { borrower: "B1" } }));
    expect(slashed?.status).toBe("Defaulted");
    expect(slashed?.loan_purpose).toBe("farming");
  });

  it("only marks Repaid once the cumulative repayment reaches the loan amount", () => {
    const p = new LoanProjector();
    p.applyEvent(ev({ id: 1, category: "loan", action: "request", value: { borrower: "B1", amount_stroops: 1000 } }));
    const partial = p.applyEvent(
      ev({ id: 2, category: "loan", action: "repay", value: { borrower: "B1", payment_stroops: 400 } })
    );
    expect(partial?.status).toBe("Active");
    const full = p.applyEvent(
      ev({ id: 3, category: "loan", action: "repay", value: { borrower: "B1", payment_stroops: 600 } })
    );
    expect(full?.status).toBe("Repaid");
  });
});
