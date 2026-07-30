import React from "react";
import { useTranslation } from "react-i18next";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type EmptyStateVariant = "loans" | "vouches";

interface EmptyStateProps {
  variant: EmptyStateVariant;
  /** When true the component renders a loading spinner instead of the empty-
   *  state message. Keeping both states in one component keeps the rendering
   *  logic co-located and makes snapshot testing straightforward. */
  loading?: boolean;
  highContrast?: boolean;
}

// ---------------------------------------------------------------------------
// Styles
// ---------------------------------------------------------------------------

const containerBase: React.CSSProperties = {
  display: "flex",
  flexDirection: "column",
  alignItems: "center",
  justifyContent: "center",
  padding: "48px 24px",
  borderRadius: 16,
  border: "1px dashed",
  textAlign: "center",
  gap: 12,
};

// ---------------------------------------------------------------------------
// Loading spinner sub-component
// ---------------------------------------------------------------------------

const spinnerKeyframes = `
@keyframes qc-spin {
  from { transform: rotate(0deg); }
  to   { transform: rotate(360deg); }
}
`;

const LoadingSpinner: React.FC<{ color: string }> = ({ color }) => (
  <>
    <style>{spinnerKeyframes}</style>
    <div
      role="status"
      aria-label="Loading"
      style={{
        width: 40,
        height: 40,
        borderRadius: "50%",
        border: `3px solid ${color}33`,
        borderTopColor: color,
        animation: "qc-spin 0.8s linear infinite",
      }}
    />
  </>
);

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

const ICONS: Record<EmptyStateVariant, string> = {
  loans: "📋",
  vouches: "🤝",
};

/**
 * EmptyState — shown when a borrower/voucher has no records yet,
 * or while the initial data load is still in flight.
 *
 * Props:
 *   variant  – "loans" | "vouches"
 *   loading  – when true shows a spinner instead of the empty message
 *   highContrast – mirror the dashboard's accessibility setting
 */
const EmptyState: React.FC<EmptyStateProps> = ({
  variant,
  loading = false,
  highContrast = false,
}) => {
  const { t } = useTranslation();

  const borderColor = highContrast ? "#ffffff44" : "#334155";
  const bg = highContrast ? "#111827" : "#1e293b";
  const textPrimary = highContrast ? "#ffffff" : "#94a3b8";
  const textSecondary = highContrast ? "#cbd5e1" : "#64748b";
  const accentColor = "#3b82f6";

  if (loading) {
    return (
      <div
        data-testid="empty-state-loading"
        style={{ ...containerBase, background: bg, borderColor }}
        aria-busy="true"
      >
        <LoadingSpinner color={accentColor} />
        <p style={{ color: textPrimary, fontSize: 15, margin: 0 }}>
          {t("emptyState.loading")}
        </p>
      </div>
    );
  }

  const icon = ICONS[variant];
  const title =
    variant === "loans"
      ? t("emptyState.loans.title")
      : t("emptyState.vouches.title");
  const subtitle =
    variant === "loans"
      ? t("emptyState.loans.subtitle")
      : t("emptyState.vouches.subtitle");

  return (
    <div
      data-testid={`empty-state-${variant}`}
      style={{ ...containerBase, background: bg, borderColor }}
      role="region"
      aria-label={title}
    >
      <span style={{ fontSize: 40, lineHeight: 1 }} aria-hidden="true">
        {icon}
      </span>
      <p
        style={{
          color: textPrimary,
          fontSize: 17,
          fontWeight: 600,
          margin: 0,
        }}
      >
        {title}
      </p>
      <p style={{ color: textSecondary, fontSize: 14, margin: 0, maxWidth: 320 }}>
        {subtitle}
      </p>
    </div>
  );
};

export default EmptyState;
