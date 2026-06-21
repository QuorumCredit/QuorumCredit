import React from "react";
import { stroopsToXlm } from "./stroops";
import type { LoanRecord, LoanStatus } from "./loanSlice";

interface LoanCardProps {
  loan: LoanRecord;
}

const STATUS_STYLES: Record<LoanStatus, { bg: string; text: string; label: string }> = {
  Active:   { bg: "#eff6ff", text: "#1d4ed8", label: "Active" },
  Repaid:   { bg: "#f0fdf4", text: "#15803d", label: "Repaid" },
  Defaulted:{ bg: "#fef2f2", text: "#dc2626", label: "Defaulted" },
  None:     { bg: "#f8fafc", text: "#64748b", label: "None" },
};

function repaidPct(loan: LoanRecord): number {
  if (loan.amount === 0) return 0;
  return Math.min(100, (loan.amount_repaid / loan.amount) * 100);
}

/**
 * LoanCard — displays a single loan record with borrower, principal, repaid %,
 * yield earned, and repayment deadline. Mobile-responsive via inline flex.
 */
const LoanCard: React.FC<LoanCardProps> = ({ loan }) => {
  const style = STATUS_STYLES[loan.status] ?? STATUS_STYLES.None;
  const pct = repaidPct(loan);
  const deadline = new Date(loan.deadline * 1000).toLocaleDateString();
  const principal = stroopsToXlm(loan.amount);
  const yieldEarned = stroopsToXlm(loan.total_yield);

  return (
    <article
      aria-label={`Loan ${loan.id}`}
      style={{
        background: style.bg,
        border: `1px solid ${style.text}33`,
        borderRadius: 10,
        padding: "16px 20px",
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      {/* Header row */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", flexWrap: "wrap", gap: 4 }}>
        <span style={{ fontWeight: 700, fontSize: 14, color: "#0f172a", wordBreak: "break-all" }}>
          {loan.borrower}
        </span>
        <span
          aria-label={`Status: ${style.label}`}
          style={{
            background: style.text + "1a",
            color: style.text,
            fontSize: 12,
            fontWeight: 600,
            borderRadius: 999,
            padding: "2px 10px",
            whiteSpace: "nowrap",
          }}
        >
          {style.label}
        </span>
      </div>

      {/* Purpose */}
      {loan.loan_purpose && (
        <p style={{ margin: 0, fontSize: 13, color: "#475569" }}>{loan.loan_purpose}</p>
      )}

      {/* Principal / Yield row */}
      <div style={{ display: "flex", gap: 24, flexWrap: "wrap" }}>
        <div>
          <div style={{ fontSize: 11, color: "#64748b" }}>Principal</div>
          <div style={{ fontWeight: 600, fontSize: 16 }}>{principal} XLM</div>
        </div>
        <div>
          <div style={{ fontSize: 11, color: "#64748b" }}>Yield</div>
          <div style={{ fontWeight: 600, fontSize: 16, color: "#15803d" }}>{yieldEarned} XLM</div>
        </div>
        <div>
          <div style={{ fontSize: 11, color: "#64748b" }}>Deadline</div>
          <div style={{ fontWeight: 600, fontSize: 14 }}>{deadline}</div>
        </div>
      </div>

      {/* Repayment progress bar */}
      <div>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, color: "#64748b", marginBottom: 4 }}>
          <span>Repaid</span>
          <span aria-label={`Repaid ${pct.toFixed(1)}%`}>{pct.toFixed(1)}%</span>
        </div>
        <div
          role="progressbar"
          aria-valuenow={pct}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label="Repayment progress"
          style={{ height: 6, background: "#e2e8f0", borderRadius: 999, overflow: "hidden" }}
        >
          <div
            style={{
              height: "100%",
              width: `${pct}%`,
              background: loan.status === "Defaulted" ? "#ef4444" : style.text,
              borderRadius: 999,
              transition: "width 0.3s ease",
            }}
          />
        </div>
      </div>
    </article>
  );
};

export default LoanCard;
