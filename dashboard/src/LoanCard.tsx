import React from "react";
import { stroopsToXlm } from "./stroops";
import type { LoanRecord, LoanStatus } from "./loanSlice";

export interface AccessibilitySettings {
  colorblindFriendly?: boolean;
  highContrast?: boolean;
}

interface LoanCardProps {
  loan: LoanRecord;
  accessibility?: AccessibilitySettings;
}

const STATUS_STYLES: Record<LoanStatus, { bg: string; text: string; border: string; label: string; icon: string }> = {
  Active:   { bg: "#eff6ff", text: "#1d4ed8", border: "#1d4ed8", label: "Active", icon: "●" },
  Repaid:   { bg: "#fff7ed", text: "#c2410c", border: "#c2410c", label: "Repaid", icon: "✓" },
  Defaulted:{ bg: "#f5f3ff", text: "#6d28d9", border: "#6d28d9", label: "Defaulted", icon: "⚠" },
  None:     { bg: "#f8fafc", text: "#475569", border: "#64748b", label: "None", icon: "○" },
};

function resolveStatusStyle(status: LoanStatus, accessibility?: AccessibilitySettings) {
  const base = STATUS_STYLES[status] ?? STATUS_STYLES.None;

  if (accessibility?.highContrast) {
    return {
      ...base,
      bg: "#000000",
      text: "#ffffff",
      border: "#ffffff",
      badgeBg: "#000000",
      badgeText: "#ffffff",
    };
  }

  if (accessibility?.colorblindFriendly) {
    return {
      ...base,
      bg: status === "Active" ? "#dbeafe" : status === "Repaid" ? "#ffedd5" : status === "Defaulted" ? "#ede9fe" : "#f1f5f9",
      text: status === "Active" ? "#0f2c6b" : status === "Repaid" ? "#9a2c00" : status === "Defaulted" ? "#4c1d95" : "#334155",
      border: status === "Active" ? "#2563eb" : status === "Repaid" ? "#ea580c" : status === "Defaulted" ? "#7c3aed" : "#64748b",
      badgeBg: status === "Active" ? "#dbeafe" : status === "Repaid" ? "#ffedd5" : status === "Defaulted" ? "#ede9fe" : "#e2e8f0",
      badgeText: status === "Active" ? "#0f2c6b" : status === "Repaid" ? "#9a2c00" : status === "Defaulted" ? "#4c1d95" : "#334155",
    };
  }

  return {
    ...base,
    badgeBg: `${base.text}1a`,
    badgeText: base.text,
  };
}

function repaidPct(loan: LoanRecord): number {
  if (loan.amount === 0) return 0;
  return Math.min(100, (loan.amount_repaid / loan.amount) * 100);
}

/**
 * LoanCard — displays a single loan record with borrower, principal, repaid %,
 * yield earned, and repayment deadline. Mobile-responsive via inline flex.
 */
const LoanCard: React.FC<LoanCardProps> = ({ loan, accessibility }) => {
  const style = resolveStatusStyle(loan.status, accessibility);
  const pct = repaidPct(loan);
  const badgeText = accessibility?.colorblindFriendly || accessibility?.highContrast
    ? `${style.icon} ${style.label}`
    : style.label;
  const deadline = new Date(loan.deadline * 1000).toLocaleDateString();
  const principal = stroopsToXlm(loan.amount);
  const yieldEarned = stroopsToXlm(loan.total_yield);

  return (
    <article
      aria-label={`Loan ${loan.id}`}
      style={{
        background: style.bg,
        border: `1px solid ${style.border}`,
        borderRadius: 10,
        padding: "16px 20px",
        display: "flex",
        flexDirection: "column",
        gap: 8,
      }}
    >
      {/* Header row */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", flexWrap: "wrap", gap: 4 }}>
        <span style={{ fontWeight: 700, fontSize: 14, color: accessibility?.highContrast ? "#ffffff" : "#0f172a", wordBreak: "break-all" }}>
          {loan.borrower}
        </span>
        <span
          aria-label={`Status: ${style.label}`}
          style={{
            background: style.badgeBg,
            color: style.badgeText ?? style.text,
            fontSize: 12,
            fontWeight: 600,
            borderRadius: 999,
            padding: "2px 10px",
            whiteSpace: "nowrap",
            border: accessibility?.highContrast ? "1px solid #ffffff" : `1px solid ${style.border}`,
          }}
        >
          {badgeText}
        </span>
      </div>

      {/* Purpose */}
      {loan.loan_purpose && (
        <p style={{ margin: 0, fontSize: 13, color: accessibility?.highContrast ? "#e2e8f0" : "#475569" }}>{loan.loan_purpose}</p>
      )}

      {/* Principal / Yield row */}
      <div style={{ display: "flex", gap: 24, flexWrap: "wrap" }}>
        <div>
          <div style={{ fontSize: 11, color: accessibility?.highContrast ? "#cbd5e1" : "#64748b" }}>Principal</div>
          <div style={{ fontWeight: 600, fontSize: 16 }}>{principal} XLM</div>
        </div>
        <div>
          <div style={{ fontSize: 11, color: accessibility?.highContrast ? "#cbd5e1" : "#64748b" }}>Yield</div>
          <div style={{ fontWeight: 600, fontSize: 16, color: accessibility?.highContrast ? "#fbbf24" : "#15803d" }}>{yieldEarned} XLM</div>
        </div>
        <div>
          <div style={{ fontSize: 11, color: accessibility?.highContrast ? "#cbd5e1" : "#64748b" }}>Deadline</div>
          <div style={{ fontWeight: 600, fontSize: 14 }}>{deadline}</div>
        </div>
      </div>

      {/* Repayment progress bar */}
      <div>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, color: accessibility?.highContrast ? "#e2e8f0" : "#64748b", marginBottom: 4 }}>
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
              background: loan.status === "Defaulted" ? "#7c3aed" : style.text,
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
