import React, { useEffect, useState } from "react";
import { Provider, useSelector } from "react-redux";
import { useTranslation } from "react-i18next";
import { store, RootState } from "./store";
import { useLoanSocket } from "./useLoanSocket";
import LoanCard, { type AccessibilitySettings } from "./LoanCard";
import { Logo } from "./Logo";
import { LoanCardErrorBoundary, DashboardErrorBoundary } from "./ErrorBoundary";
import EmptyState from "./EmptyState";

// ---------------------------------------------------------------------------
// Inner component — must be inside Provider
// ---------------------------------------------------------------------------

interface DashboardInnerProps {
  borrower: string;
  wsUrl: string;
  apiKey?: string;
}

/** @internal exported for testing only */
export const DashboardInner: React.FC<DashboardInnerProps> = ({ borrower, wsUrl, apiKey }) => {
  useLoanSocket({ url: wsUrl, borrower, apiKey });

  const { t, i18n } = useTranslation();
  const locale = i18n.language || "en";

  const [accessibility, setAccessibility] = useState<AccessibilitySettings>(() => {
    if (typeof window === "undefined") return { colorblindFriendly: false, highContrast: false };
    try {
      const stored = window.localStorage.getItem("quorum-dashboard-accessibility");
      return stored
        ? JSON.parse(stored)
        : { colorblindFriendly: false, highContrast: false };
    } catch {
      return { colorblindFriendly: false, highContrast: false };
    }
  });

  useEffect(() => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(
        "quorum-dashboard-accessibility",
        JSON.stringify(accessibility),
      );
    }
  }, [accessibility]);

  const { loans, reputation, connected, loading, lastUpdated } = useSelector(
    (state: RootState) => state.loans,
  );

  const activeLoans = loans.filter((l) => l.status === "Active");
  const closedLoans = loans.filter((l) => l.status !== "Active");

  const bgColor = accessibility.highContrast ? "#000000" : "#0f172a";
  const textColor = accessibility.highContrast ? "#ffffff" : "#f1f5f9";
  const cardBg = accessibility.highContrast ? "#111827" : "#1e293b";
  const accentColor = "#3b82f6";
  const successColor = "#10b981";

  const toggleButtonStyle = (active: boolean): React.CSSProperties => ({
    border: active ? `2px solid ${accentColor}` : "1px solid #475569",
    background: active ? "rgba(59, 130, 246, 0.1)" : "#1e293b",
    color: active ? accentColor : "#cbd5e1",
    borderRadius: 8,
    padding: "8px 16px",
    fontWeight: 600,
    cursor: "pointer",
    transition: "all 0.2s ease",
    fontSize: 13,
  });

  /** Format a timestamp (ms) as a locale-aware time string */
  const formatTime = (ms: number) =>
    new Intl.DateTimeFormat(locale, {
      hour: "numeric",
      minute: "2-digit",
      second: "2-digit",
    }).format(new Date(ms));

  return (
    <div
      style={{
        background: bgColor,
        color: textColor,
        minHeight: "100vh",
        fontFamily:
          "-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif",
      }}
    >
      {/* Navigation Bar */}
      <nav
        style={{
          background: cardBg,
          borderBottom: "1px solid rgba(148, 163, 184, 0.1)",
          padding: "16px 24px",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <Logo />
          <div>
            <h1 style={{ margin: 0, fontSize: 20, fontWeight: 700, color: textColor }}>
              {t("nav.title")}
            </h1>
            <p style={{ margin: 0, fontSize: 12, color: "#64748b" }}>
              {t("nav.tagline")}
            </p>
          </div>
        </div>
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 8,
            fontSize: 13,
            color: connected ? successColor : "#ef4444",
            fontWeight: 600,
          }}
        >
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: connected ? successColor : "#ef4444",
              display: "inline-block",
            }}
          />
          {connected ? t("nav.statusLive") : t("nav.statusOffline")}
        </div>
      </nav>

      {/* Main Content */}
      <main style={{ maxWidth: 900, margin: "0 auto", padding: "32px 24px" }}>
        {/* Settings */}
        <div
          style={{
            display: "flex",
            gap: 8,
            marginBottom: 24,
            flexWrap: "wrap",
            alignItems: "center",
          }}
        >
          <button
            onClick={() =>
              setAccessibility((prev) => ({
                ...prev,
                colorblindFriendly: !prev.colorblindFriendly,
              }))
            }
            style={toggleButtonStyle(Boolean(accessibility.colorblindFriendly))}
          >
            {t("accessibility.colorblindFriendly")}
          </button>
          <button
            onClick={() =>
              setAccessibility((prev) => ({
                ...prev,
                highContrast: !prev.highContrast,
              }))
            }
            style={toggleButtonStyle(Boolean(accessibility.highContrast))}
          >
            {t("accessibility.highContrast")}
          </button>
          {lastUpdated && (
            <span
              style={{
                marginLeft: "auto",
                fontSize: 12,
                color: "#64748b",
              }}
            >
              ⏱️ {t("loans.updated", { time: formatTime(lastUpdated) })}
            </span>
          )}
        </div>

        {/* Reputation Card */}
        {reputation && (
          <div
            style={{
              background: cardBg,
              borderRadius: 16,
              padding: "20px 24px",
              marginBottom: 24,
              display: "grid",
              gridTemplateColumns: "repeat(2, 1fr)",
              gap: 16,
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
                {t("reputation.tier")}
              </p>
              <p style={{ fontSize: 18, fontWeight: 700, color: accentColor, margin: 0 }}>
                {reputation.tier}
              </p>
            </div>
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
                {t("reputation.creditScore")}
              </p>
              <p style={{ fontSize: 18, fontWeight: 700, color: successColor, margin: 0 }}>
                {new Intl.NumberFormat(locale).format(reputation.score)}
              </p>
            </div>
          </div>
        )}

        {/* Active Loans Section */}
        <section aria-labelledby="active-loans-heading" style={{ marginBottom: 32 }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 12,
              marginBottom: 16,
            }}
          >
            <h2
              id="active-loans-heading"
              style={{ margin: 0, fontSize: 18, fontWeight: 700, color: textColor }}
            >
              {t("loans.activeHeading")}
            </h2>
            <span
              style={{
                background: accentColor,
                color: "#ffffff",
                borderRadius: 999,
                padding: "2px 8px",
                fontSize: 12,
                fontWeight: 700,
              }}
            >
              {activeLoans.length}
            </span>
          </div>

          {/* Loading vs empty vs populated */}
          {loading ? (
            <EmptyState
              variant="loans"
              loading
              highContrast={accessibility.highContrast}
            />
          ) : activeLoans.length === 0 ? (
            <EmptyState variant="loans" highContrast={accessibility.highContrast} />
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              {activeLoans.map((loan) => (
                <LoanCardErrorBoundary key={loan.id}>
                  <div
                    style={{
                      borderRadius: 12,
                      border: "1px solid rgba(148, 163, 184, 0.2)",
                      transition: "border-color 0.2s, box-shadow 0.2s",
                    }}
                    onMouseEnter={(e) => {
                      (e.currentTarget as HTMLDivElement).style.borderColor =
                        "rgba(148, 163, 184, 0.4)";
                      (e.currentTarget as HTMLDivElement).style.boxShadow =
                        "0 8px 24px rgba(59, 130, 246, 0.1)";
                    }}
                    onMouseLeave={(e) => {
                      (e.currentTarget as HTMLDivElement).style.borderColor =
                        "rgba(148, 163, 184, 0.2)";
                      (e.currentTarget as HTMLDivElement).style.boxShadow = "none";
                    }}
                  >
                    <LoanCard loan={loan} accessibility={accessibility} />
                  </div>
                </LoanCardErrorBoundary>
              ))}
            </div>
          )}
        </section>

        {/* Closed Loans Section */}
        {!loading && closedLoans.length > 0 && (
          <section aria-labelledby="closed-loans-heading">
            <div
              style={{
                display: "flex",
                alignItems: "center",
                gap: 12,
                marginBottom: 16,
              }}
            >
              <h2
                id="closed-loans-heading"
                style={{ margin: 0, fontSize: 18, fontWeight: 700, color: textColor }}
              >
                {t("loans.closedHeading")}
              </h2>
              <span
                style={{
                  background: "#475569",
                  color: "#ffffff",
                  borderRadius: 999,
                  padding: "2px 8px",
                  fontSize: 12,
                  fontWeight: 700,
                }}
              >
                {closedLoans.length}
              </span>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 16 }}>
              {closedLoans.map((loan) => (
                <LoanCardErrorBoundary key={loan.id}>
                  <LoanCard loan={loan} accessibility={accessibility} />
                </LoanCardErrorBoundary>
              ))}
            </div>
          </section>
        )}
      </main>
    </div>
  );
};

// ---------------------------------------------------------------------------
// Public component — wraps in its own Redux Provider
// ---------------------------------------------------------------------------

export interface LoanStatusDashboardProps {
  /** Borrower address to display loans for */
  borrower: string;
  /** socket.io server URL */
  wsUrl: string;
  /** Optional API key for socket auth */
  apiKey?: string;
}

/**
 * LoanStatusDashboard — self-contained component that connects to a socket.io
 * server, displays active/closed loans with real-time updates, repayment
 * progress, yield earned, and borrower reputation tier.
 *
 * Props:
 *   - borrower: Stellar address of the borrower
 *   - wsUrl: socket.io server base URL
 *   - apiKey: optional API key for socket auth header
 *
 * Wrapped in a DashboardErrorBoundary so any catastrophic render error
 * degrades gracefully instead of blanking the entire page.
 */
const LoanStatusDashboard: React.FC<LoanStatusDashboardProps> = (props) => (
  <DashboardErrorBoundary>
    <Provider store={store}>
      <DashboardInner {...props} />
    </Provider>
  </DashboardErrorBoundary>
);

export default LoanStatusDashboard;
