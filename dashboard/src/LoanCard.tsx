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
  Active:   { bg: "rgba(59, 130, 246, 0.1)", text: "#3b82f6", border: "#3b82f6", label: "Active", icon: "●" },
  Repaid:   { bg: "rgba(16, 185, 129, 0.1)", text: "#10b981", border: "#10b981", label: "Repaid", icon: "✓" },
  Defaulted:{ bg: "rgba(239, 68, 68, 0.1)", text: "#ef4444", border: "#ef4444", label: "Defaulted", icon: "⚠" },
  None:     { bg: "rgba(100, 116, 139, 0.1)", text: "#64748b", border: "#475569", label: "None", icon: "○" },
};

function resolveStatusStyle(status: LoanStatus, accessibility?: AccessibilitySettings) {
  const base = STATUS_STYLES[status] ?? STATUS_STYLES.None;

  if (accessibility?.highContrast) {
    return {
      ...base,
      bg: "#111827",
      text: "#ffffff",
      border: "#ffffff",
      badgeBg: "#1f2937",
      badgeText: "#ffffff",
    };
  }

  if (accessibility?.colorblindFriendly) {
    return {
      ...base,
      bg: status === "Active" ? "rgba(59, 130, 246, 0.1)" : status === "Repaid" ? "rgba(245, 158, 11, 0.1)" : status === "Defaulted" ? "rgba(168, 85, 247, 0.1)" : "rgba(100, 116, 139, 0.1)",
      text: status === "Active" ? "#3b82f6" : status === "Repaid" ? "#f59e0b" : status === "Defaulted" ? "#a855f7" : "#64748b",
      border: status === "Active" ? "#3b82f6" : status === "Repaid" ? "#f59e0b" : status === "Defaulted" ? "#a855f7" : "#475569",
      badgeBg: status === "Active" ? "rgba(59, 130, 246, 0.2)" : status === "Repaid" ? "rgba(245, 158, 11, 0.2)" : status === "Defaulted" ? "rgba(168, 85, 247, 0.2)" : "rgba(100, 116, 139, 0.2)",
      badgeText: status === "Active" ? "#3b82f6" : status === "Repaid" ? "#f59e0b" : status === "Defaulted" ? "#a855f7" : "#64748b",
    };
  }

  return {
    ...base,
    badgeBg: base.bg,
    badgeText: base.text,
  };
}

function repaidPct(loan: LoanRecord): number {
  if (loan.amount === 0) return 0;
  return Math.min(100, (loan.amount_repaid / loan.amount) * 100);
}

/**
 * LoanCard — displays a single loan record with borrower, principal, repaid %,
 * yield earned, and repayment deadline. Dark theme with modern styling.
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
        borderRadius: 12,
        padding: 20,
        display: "flex",
        flexDirection: "column",
        gap: 16,
        transition: "all 0.2s ease",
      }}
    >
      {/* Header row - Borrower address + Status badge */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start", gap: 12, flexWrap: "wrap" }}>
        <div style={{ flex: 1, minWidth: 0 }}>
          <div style={{ fontSize: 11, color: "#94a3b8", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 4 }}>
            Borrower
          </div>
          <span style={{ fontWeight: 600, fontSize: 14, color: "#f1f5f9", wordBreak: "break-all", fontFamily: "monospace" }}>
            {loan.borrower.substring(0, 10)}...{loan.borrower.substring(loan.borrower.length - 10)}
          </span>
        </div>
        <span
          aria-label={`Status: ${style.label}`}
          style={{
            background: style.badgeBg,
            color: style.badgeText ?? style.text,
            fontSize: 12,
            fontWeight: 700,
            borderRadius: 8,
            padding: "6px 12px",
            whiteSpace: "nowrap",
            border: `1px solid ${style.border}`,
          }}
        >
          {style.icon} {badgeText}
        </span>
      </div>

      {/* Purpose */}
      {loan.loan_purpose && (
        <p style={{ margin: 0, fontSize: 13, color: "#cbd5e1", fontStyle: "italic" }}>"{loan.loan_purpose}"</p>
      )}

      {/* Key metrics grid */}
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(120px, 1fr))", gap: 16 }}>
        <div>
          <div style={{ fontSize: 11, color: "#94a3b8", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 6 }}>
            Principal
          </div>
          <div style={{ fontWeight: 700, fontSize: 16, color: "#3b82f6" }}>{principal} XLM</div>
        </div>
        <div>
          <div style={{ fontSize: 11, color: "#94a3b8", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 6 }}>
            Yield Earned
          </div>
          <div style={{ fontWeight: 700, fontSize: 16, color: "#10b981" }}>+{yieldEarned} XLM</div>
        </div>
        <div>
          <div style={{ fontSize: 11, color: "#94a3b8", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 6 }}>
            Due Date
          </div>
          <div style={{ fontWeight: 600, fontSize: 14, color: "#f1f5f9" }}>{deadline}</div>
        </div>
      </div>

      {/* Repayment progress bar */}
      <div>
        <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, color: "#cbd5e1", marginBottom: 8 }}>
          <span style={{ fontWeight: 600 }}>Repayment Progress</span>
          <span aria-label={`Repaid ${pct.toFixed(1)}%`} style={{ fontWeight: 700, color: style.text }}>
            {pct.toFixed(1)}%
          </span>
        </div>
        <div
          role="progressbar"
          aria-valuenow={pct}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label="Repayment progress"
          style={{
            height: 8,
            background: "rgba(148, 163, 184, 0.2)",
            borderRadius: 999,
            overflow: "hidden",
            border: "1px solid rgba(148, 163, 184, 0.3)",
          }}
        >
          <div
            style={{
              height: "100%",
              width: `${pct}%`,
              background: loan.status === "Defaulted" 
                ? "linear-gradient(90deg, #ef4444, #dc2626)" 
                : `linear-gradient(90deg, ${style.text}, ${style.text}cc)`,
              borderRadius: 999,
              transition: "width 0.3s ease",
            }}
          />
        </div>
      </div>

      {/* Loan amount repaid details */}
      <div style={{ display: "flex", justifyContent: "space-between", fontSize: 12, color: "#94a3b8", padding: "8px 0", borderTop: "1px solid rgba(148, 163, 184, 0.1)" }}>
        <span>Repaid: {stroopsToXlm(loan.amount_repaid)} / {principal} XLM</span>
        <span>ID: #{loan.id}</span>
      </div>
    </article>
  );
};

export default LoanCard;
