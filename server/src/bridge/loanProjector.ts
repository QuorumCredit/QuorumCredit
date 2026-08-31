import type { IndexedEvent, LoanRecord } from "../types.js";

/**
 * Projects the indexer's decoded loan/vouch events onto the dashboard's LoanRecord
 * shape (dashboard/src/loanSlice.ts).
 *
 * As of the v2 enriched decoder (`tools/indexer/src/indexer.rs` `simplify_value`),
 * `loan/request` events now include `loan_id` (u64), `deadline` (unix timestamp u64),
 * and `vouchers` (array of voucher address hex strings). This projector uses `loan_id`
 * as the primary key, enabling correct tracking of multiple concurrent loans per borrower.
 *
 * Backward compatibility: if a `loan/request` event lacks `loan_id` (old v1 events),
 * the projector falls back to a synthetic id derived from the borrower address so that
 * existing indexed history continues to render without data loss.
 */
export class LoanProjector {
  private readonly byLoanId = new Map<string, LoanRecord>();

  applyEvent(event: IndexedEvent): LoanRecord | null {
    if (event.category !== "loan") return null;
    const v = event.value;
    const borrower = typeof v.borrower === "string" ? v.borrower : undefined;
    if (!borrower) return null;

    const createdAt = Math.floor(new Date(event.ledgerClosedAt).getTime() / 1000) || 0;

    // Determine the map key: prefer real loan_id from decoded event, fall back to
    // synthetic id for old events that pre-date the v2 decoder.
    const rawLoanId = v.loan_id;
    const loanKey =
      rawLoanId !== undefined && rawLoanId !== null
        ? String(rawLoanId)
        : String(syntheticLoanId(borrower));

    const existing = this.byLoanId.get(loanKey);
    const base: LoanRecord = existing ?? {
      id: rawLoanId !== undefined && rawLoanId !== null ? Number(rawLoanId) : syntheticLoanId(borrower),
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

    function num(key: string): number {
      const raw = v[key];
      return typeof raw === "number" ? raw : typeof raw === "string" ? Number(raw) || 0 : 0;
    }

    const updated = applyLoanFields(base, event, num);
    if (!updated) return null;

    this.byLoanId.set(loanKey, updated);
    return updated;
  }

  /**
   * Returns all projected LoanRecords across all loan IDs.
   */
  getAll(): LoanRecord[] {
    return Array.from(this.byLoanId.values());
  }

  /**
   * Returns the LoanRecord for a given borrower address. Searches across all
   * tracked loans to support the case where a borrower has (or has had) multiple
   * loans. Returns the first match found (most recently inserted order is
   * Map insertion order).
   */
  get(borrower: string): LoanRecord | undefined {
    for (const record of this.byLoanId.values()) {
      if (record.borrower === borrower) {
        return record;
      }
    }
    return undefined;
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
    case "request": {
      // Populate deadline from decoded event when available.
      const deadline =
        typeof v.deadline === "number" && v.deadline > 0
          ? v.deadline
          : typeof v.deadline === "string" && Number(v.deadline) > 0
          ? Number(v.deadline)
          : base.deadline;

      // Populate vouchers from decoded event when available.
      const vouchers: LoanRecord["vouchers"] = Array.isArray(v.vouchers)
        ? (v.vouchers as string[]).map((addr) => ({
            voucher: addr,
            stake: 0,
            vouch_timestamp: 0,
          }))
        : base.vouchers;

      return {
        ...base,
        amount: num("amount_stroops"),
        amount_repaid: 0,
        status: "Active",
        loan_purpose: typeof v.loan_purpose === "string" ? v.loan_purpose : base.loan_purpose,
        deadline,
        vouchers,
      };
    }
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

/**
 * Generates a stable synthetic loan id from a borrower address string.
 * Used only as a fallback for pre-v2 events that lack a real `loan_id`.
 */
function syntheticLoanId(borrower: string): number {
  let hash = 0;
  for (let i = 0; i < borrower.length; i++) {
    hash = (hash * 31 + borrower.charCodeAt(i)) | 0;
  }
  return Math.abs(hash);
}
