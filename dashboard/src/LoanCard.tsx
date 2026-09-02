import React from "react";
import { useTranslation } from "react-i18next";
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

const STATUS_STYLES: Record<
  LoanStatus,
  { bg: string; text: string; border: string; label: string; icon: string }
> = {
  Active: {
    bg: "rgba(59, 130, 246, 0.1)",
    text: "#3b82f6",
    border: "#3b82f6",
    label: "Active",
    icon: "●",
  },
  Repaid: {
    bg: "rgba(16, 185, 129, 0.1)",
    text: "#10b981",
    border: "#10b981",
    label: "Repaid",
    icon: "✓",
  },
  Defaulted: {
    bg: "rgba(239, 68, 68, 0.1)",
    text: "#ef4444",
    border: "#ef4444",
    label: "Defaulted",
    icon: "⚠",
  },
  None: {
    bg: "rgba(100, 116, 139, 0.1)",
    text: "#64748b",
    border: "#475569",
    label: "None",
    icon: "○",
  },
};

function resolveStatusStyle(
  status: LoanStatus,
  accessibility?: AccessibilitySettings,
) {
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
      bg:
        status === "Active"
          ? "rgba(59, 130, 246, 0.1)"
          : status === "Repaid"
            ? "rgba(245, 158, 11, 0.1)"
            : status === "Defaulted"
              ? "rgba(168, 85, 247, 0.1)"
              : "rgba(100, 116, 139, 0.1)",
      text:
        status === "Active"
          ? "#3b82f6"
          : status === "Repaid"
            ? "#f59e0b"
            : status === "Defaulted"
              ? "#a855f7"
              : "#64748b",
      border:
        status === "Active"
          ? "#3b82f6"
          : status === "Repaid"
            ? "#f59e0b"
            : status === "Defaulted"
              ? "#a855f7"
              : "#475569",
      badgeBg:
        status === "Active"
          ? "rgba(59, 130, 246, 0.2)"
          : status === "Repaid"
            ? "rgba(245, 158, 11, 0.2)"
            : status === "Defaulted"
              ? "rgba(168, 85, 247, 0.2)"
              : "rgba(100, 116, 139, 0.2)",
      badgeText:
        status === "Active"
          ? "#3b82f6"
          : status === "Repaid"
            ? "#f59e0b"
            : status === "Defaulted"
              ? "#a855f7"
              : "#64748b",
    };
  }

  return { ...base, badgeBg: base.bg, badgeText: base.text };
}

function repaidPct(loan: LoanRecord): number {
  if (loan.amount === 0) return 0;
  return Math.min(100, (loan.amount_repaid / loan.amount) * 100);
}

/**
 * Format a Unix timestamp (seconds) into a locale-aware date string.
 * Falls back gracefully if the locale is unavailable.
 */
function formatDeadline(unixSeconds: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    year: "numeric",
    month: "short",
    day: "numeric",
  }).format(new Date(unixSeconds * 1000));
}

/**
 * LoanCard — displays a single loan record with borrower, principal, repaid %,
 * yield earned, and repayment deadline. Dark theme with modern styling.
 */
const LoanCard: React.FC<LoanCardProps> = ({ loan, accessibility }) => {
  const { t, i18n } = useTranslation();
  const locale = i18n.language || "en";

  const style = resolveStatusStyle(loan.status, accessibility);
  const pct = repaidPct(loan);
  const badgeText =
    accessibility?.colorblindFriendly || accessibility?.highContrast
      ? `${style.icon} ${style.label}`
      : style.label;

  const deadline = formatDeadline(loan.deadline, locale);
  const principal = stroopsToXlm(loan.amount);
  const yieldEarned = stroopsToXlm(loan.total_yield);

  return (
    <div
      role="article"
      aria-label={`Loan ${loan.id}`}
      style={{
        background: style.bg,
        border: `1px solid ${style.border}`,
        borderRadius: 12,
        padding: "20px 24px",
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      {/* Header row — Borrower address + Status badge */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "flex-start",
          gap: 8,
        }}
      >
        <div>
          <p
            style={{
              fontSize: 11,
              fontWeight: 600,
              color: "#64748b",
              textTransform: "uppercase",
              letterSpacing: "0.08em",
              margin: "0 0 4px",
            }}
          >
            {t("loanCard.borrower")}
          </p>
          <p
            style={{
              fontFamily: "monospace",
              fontSize: 13,
              color: "#94a3b8",
              margin: 0,
            }}
          >
            {loan.borrower.substring(0, 10)}…
            {loan.borrower.substring(loan.borrower.length - 10)}
          </p>
        </div>
        <span
          aria-label={`Status: ${style.label}`}
          style={{
            background: style.badgeBg,
            color: style.badgeText,
            border: `1px solid ${style.border}`,
            borderRadius: 999,
            padding: "3px 10px",
            fontSize: 12,
            fontWeight: 600,
            whiteSpace: "nowrap",
          }}
        >
          {style.icon} {badgeText}
        </span>
      </div>

      {/* Purpose */}
      {loan.loan_purpose && (
        <p
          style={{
            color: "#94a3b8",
            fontSize: 13,
            fontStyle: "italic",
            margin: 0,
          }}
        >
          &quot;{loan.loan_purpose}&quot;
        </p>
      )}

      {/* Key metrics grid */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "repeat(3, 1fr)",
          gap: 12,
        }}
      >
        {[
          { label: t("loanCard.principal"), value: `${principal} XLM` },
          { label: t("loanCard.yieldEarned"), value: `+${yieldEarned} XLM` },
          { label: t("loanCard.dueDate"), value: deadline },
        ].map(({ label, value }) => (
          <div key={label}>
            <p
              style={{
                fontSize: 11,
                fontWeight: 600,
                color: "#64748b",
                textTransform: "uppercase",
                letterSpacing: "0.08em",
                margin: "0 0 2px",
              }}
            >
              {label}
            </p>
            <p style={{ fontSize: 14, color: "#f1f5f9", fontWeight: 600, margin: 0 }}>
              {value}
            </p>
          </div>
        ))}
      </div>

      {/* Repayment progress bar */}
      <div>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            fontSize: 12,
            color: "#64748b",
            marginBottom: 4,
          }}
        >
          <span>{t("loanCard.repaymentProgress")}</span>
          <span>
            {new Intl.NumberFormat(locale, {
              style: "percent",
              minimumFractionDigits: 1,
              maximumFractionDigits: 1,
            }).format(pct / 100)}
          </span>
        </div>
        <div
          style={{
            background: "rgba(148, 163, 184, 0.1)",
            borderRadius: 999,
            height: 6,
            overflow: "hidden",
          }}
          role="progressbar"
          aria-valuenow={Math.round(pct)}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-label={t("loanCard.repaymentProgress")}
        >
          <div
            style={{
              width: `${pct}%`,
              background: style.text,
              height: "100%",
              borderRadius: 999,
              transition: "width 0.4s ease",
            }}
          />
        </div>
      </div>

      {/* Loan amount repaid details */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          fontSize: 12,
          color: "#64748b",
        }}
      >
        <span>
          {t("loanCard.repaid")}: {stroopsToXlm(loan.amount_repaid)} / {principal}{" "}
          XLM
        </span>
        <span>
          {t("loanCard.id")}: #{loan.id}
        </span>
      </div>
    </div>
  );
};

export default LoanCard;
