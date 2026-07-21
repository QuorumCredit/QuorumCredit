import type { IndexedEvent, LoanRecord } from "../types.js";

/**
 * Projects the indexer's decoded loan/vouch events onto the dashboard's LoanRecord
 * shape (dashboard/src/loanSlice.ts).
 *
 * Known gap, documented rather than papered over: tools/indexer/src/indexer.rs's
 * `simplify_value` does not currently decode a real on-chain loan id, deadline, or
 * co-borrower/voucher list into event values — only borrower/amount/purpose/token for
 * `loan/request` and borrower/payment for `loan/repay|slash`. Closing that gap means
 * teaching the indexer's decoder about the full LoanRecord ABI (soroban-side), which
 * is out of scope here (a Rust change to tools/indexer's event decoder deserves its
 * own review, and this sandbox has no Rust toolchain to verify it against). Until
 * then this projector:
 *  - keys loans by a stable hash of `borrower` (assumes at most one live loan per
 *    borrower at a time, matching what's actually derivable from today's indexed
 *    fields) instead of a real loan id, so repeated events for the same borrower
 *    upsert in place rather than appearing as distinct loans;
 *  - merges fields cumulatively across a borrower's events (request establishes
 *    amount/purpose, repay/slash update status) rather than resetting them;
 *  - leaves `deadline` at 0 and `vouchers` empty when the indexer hasn't decoded
 *    that data — never fabricates values it doesn't have.
 */
export class LoanProjector {
  private readonly byBorrower = new Map<string, LoanRecord>();

  applyEvent(event: IndexedEvent): LoanRecord | null {
    if (event.category !== "loan") return null;
    const v = event.value;
    const borrower = typeof v.borrower === "string" ? v.borrower : undefined;
    if (!borrower) return null;

    const createdAt = Math.floor(new Date(event.ledgerClosedAt).getTime() / 1000) || 0;
    const existing = this.byBorrower.get(borrower);
    const base: LoanRecord = existing ?? {
      id: syntheticLoanId(borrower),
      borrower,
      amount: 0,
      amount_repaid: 0,
      total_yield: 0,
      status: "None",
      created_at: createdAt,
      deadline: 0,
      loan_purpose: "",
      vouchers: [],
    };

    const updated = applyLoanFields(base, event, num);
    if (!updated) return null;

    this.byBorrower.set(borrower, updated);
    return updated;

    function num(key: string): number {
      const raw = v[key];
      return typeof raw === "number" ? raw : typeof raw === "string" ? Number(raw) || 0 : 0;
    }
  }

  get(borrower: string): LoanRecord | undefined {
    return this.byBorrower.get(borrower);
  }
}

function applyLoanFields(
  base: LoanRecord,
  event: IndexedEvent,
  num: (key: string) => number
): LoanRecord | null {
  if (event.category !== "loan") return null;
  const v = event.value;

  switch (event.action) {
    case "request":
      return {
        ...base,
        amount: num("amount_stroops"),
        amount_repaid: 0,
        status: "Active",
        loan_purpose: typeof v.loan_purpose === "string" ? v.loan_purpose : base.loan_purpose,
      };
    case "repay": {
      const amountRepaid = base.amount_repaid + num("payment_stroops");
      return {
        ...base,
        amount_repaid: amountRepaid,
        status: amountRepaid >= base.amount && base.amount > 0 ? "Repaid" : base.status,
      };
    }
    case "slash":
      return { ...base, status: "Defaulted" };
    default:
      return null;
  }
}

function syntheticLoanId(borrower: string): number {
  let hash = 0;
  for (let i = 0; i < borrower.length; i++) {
    hash = (hash * 31 + borrower.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}
