import React, { useEffect, useState } from "react";
import { Provider, useSelector } from "react-redux";
import { store, RootState } from "./store";
import { useLoanSocket } from "./useLoanSocket";
import LoanCard, { type AccessibilitySettings } from "./LoanCard";
import { Logo } from "./Logo";
import { LoanCardErrorBoundary, DashboardErrorBoundary } from "./ErrorBoundary";

// ---------------------------------------------------------------------------
// Inner component — must be inside Provider
// ---------------------------------------------------------------------------

interface DashboardInnerProps {
  borrower: string;
  wsUrl: string;
  apiKey?: string;
}

const DashboardInner: React.FC<DashboardInnerProps> = ({ borrower, wsUrl, apiKey }) => {
  useLoanSocket({ url: wsUrl, borrower, apiKey });

  const [accessibility, setAccessibility] = useState<AccessibilitySettings>(() => {
    if (typeof window === "undefined") return { colorblindFriendly: false, highContrast: false };
    try {
      const stored = window.localStorage.getItem("quorum-dashboard-accessibility");
      return stored ? JSON.parse(stored) : { colorblindFriendly: false, highContrast: false };
    } catch {
      return { colorblindFriendly: false, highContrast: false };
    }
  });

  useEffect(() => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem("quorum-dashboard-accessibility", JSON.stringify(accessibility));
    }
  }, [accessibility]);

  const { loans, reputation, connected, lastUpdated } = useSelector(
    (state: RootState) => state.loans
  );

  const activeLoans = loans.filter((l) => l.status === "Active");
  const closedLoans = loans.filter((l) => l.status !== "Active");

  const bgColor = accessibility.highContrast ? "#000000" : "#0f172a";
  const textColor = accessibility.highContrast ? "#ffffff" : "#f1f5f9";
  const cardBg = accessibility.highContrast ? "#111827" : "#1e293b";
  const accentColor = "#3b82f6";
  const successColor = "#10b981";

  const toggleButtonStyle = (active: boolean) => ({
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

  return (
    <div
      aria-label="Loan Status Dashboard"
      style={{
        fontFamily: "system-ui, -apple-system, sans-serif",
        minHeight: "100vh",
        background: `linear-gradient(135deg, ${bgColor} 0%, #1a1f2e 100%)`,
        color: textColor,
        padding: 0,
        margin: 0,
      }}
    >
      {/* Navigation Bar */}
      <nav
        style={{
          background: `rgba(15, 23, 42, 0.8)`,
          backdropFilter: "blur(10px)",
          borderBottom: `1px solid rgba(52, 211, 153, 0.1)`,
          padding: "16px 32px",
          display: "flex",
          alignItems: "center",
          justifyContent: "space-between",
          position: "sticky",
          top: 0,
          zIndex: 10,
        }}
      >
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <Logo size={40} />
          <div>
            <h1 style={{ margin: 0, fontSize: 24, fontWeight: 700, color: "#ffffff" }}>QuorumCredit</h1>
            <p style={{ margin: "4px 0 0 0", fontSize: 12, color: "#94a3b8" }}>Proof of Trust Lending</p>
          </div>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 8,
              padding: "8px 12px",
              background: connected ? "rgba(16, 185, 129, 0.1)" : "rgba(239, 68, 68, 0.1)",
              borderRadius: 6,
              border: `1px solid ${connected ? "#10b981" : "#ef4444"}`,
            }}
          >
            <span
              style={{
                width: 8,
                height: 8,
                borderRadius: "50%",
                background: connected ? "#10b981" : "#ef4444",
                animation: connected ? "pulse 2s infinite" : "none",
              }}
            />
            <span style={{ fontSize: 12, fontWeight: 600, color: connected ? "#10b981" : "#ef4444" }}>
              {connected ? "Live" : "Offline"}
            </span>
          </div>
        </div>
      </nav>

      {/* Main Content */}
      <div style={{ padding: "32px", maxWidth: 1200, margin: "0 auto" }}>
        {/* Settings */}
        <div
          style={{
            display: "flex",
            gap: 12,
            marginBottom: 32,
            flexWrap: "wrap",
          }}
        >
          <button
            type="button"
            aria-pressed={Boolean(accessibility.colorblindFriendly)}
            onClick={() => setAccessibility((prev) => ({ ...prev, colorblindFriendly: !prev.colorblindFriendly }))}
            style={toggleButtonStyle(Boolean(accessibility.colorblindFriendly))}
          >
            🎨 Colorblind-friendly
          </button>
          <button
            type="button"
            aria-pressed={Boolean(accessibility.highContrast)}
            onClick={() => setAccessibility((prev) => ({ ...prev, highContrast: !prev.highContrast }))}
            style={toggleButtonStyle(Boolean(accessibility.highContrast))}
          >
            ⚡ High Contrast
          </button>
          {lastUpdated && (
            <div style={{ marginLeft: "auto", fontSize: 12, color: "#64748b", display: "flex", alignItems: "center", gap: 8 }}>
              <span>⏱️</span>
              Updated {new Date(lastUpdated).toLocaleTimeString()}
            </div>
          )}
        </div>

        {/* Reputation Card */}
        {reputation && (
          <div
            style={{
              background: `linear-gradient(135deg, rgba(59, 130, 246, 0.1) 0%, rgba(16, 185, 129, 0.1) 100%)`,
              border: "1px solid rgba(148, 163, 184, 0.2)",
              borderRadius: 12,
              padding: 24,
              marginBottom: 32,
              display: "grid",
              gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))",
              gap: 24,
            }}
          >
            <div>
              <div style={{ fontSize: 12, color: "#94a3b8", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 8 }}>
                Reputation Tier
              </div>
              <div style={{ fontSize: 28, fontWeight: 700, color: accentColor }}>{reputation.tier}</div>
            </div>
            <div>
              <div style={{ fontSize: 12, color: "#94a3b8", textTransform: "uppercase", letterSpacing: "0.05em", marginBottom: 8 }}>
                Credit Score
              </div>
              <div style={{ fontSize: 28, fontWeight: 700, color: successColor }}>{reputation.score}</div>
            </div>
          </div>
        )}

        {/* Active Loans Section */}
        <section style={{ marginBottom: 40 }}>
          <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 20 }}>
            <h2 style={{ margin: 0, fontSize: 20, fontWeight: 700, color: "#ffffff" }}>
              Active Loans
            </h2>
            <span
              style={{
                background: accentColor,
                color: "#ffffff",
                padding: "4px 12px",
                borderRadius: 999,
                fontSize: 13,
                fontWeight: 600,
              }}
            >
              {activeLoans.length}
            </span>
          </div>
          {activeLoans.length === 0 ? (
            <div
              style={{
                background: cardBg,
                border: "1px dashed rgba(148, 163, 184, 0.2)",
                borderRadius: 12,
                padding: 32,
                textAlign: "center",
                color: "#64748b",
              }}
            >
              <p style={{ margin: 0, fontSize: 15 }}>No active loans yet</p>
            </div>
          ) : (
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(350px, 1fr))", gap: 16 }}>
              {activeLoans.map((loan) => (
                <div
                  key={loan.id}
                  style={{
                    background: cardBg,
                    border: `1px solid rgba(148, 163, 184, 0.2)`,
                    borderRadius: 12,
                    overflow: "hidden",
                    transition: "all 0.2s ease",
                  }}
                  onMouseEnter={(e) => {
                    (e.currentTarget as HTMLDivElement).style.borderColor = "rgba(148, 163, 184, 0.4)";
                    (e.currentTarget as HTMLDivElement).style.boxShadow = "0 8px 24px rgba(59, 130, 246, 0.1)";
                  }}
                  onMouseLeave={(e) => {
                    (e.currentTarget as HTMLDivElement).style.borderColor = "rgba(148, 163, 184, 0.2)";
                    (e.currentTarget as HTMLDivElement).style.boxShadow = "none";
                  }}
                >
                  <LoanCardErrorBoundary>
                    <LoanCard loan={loan} accessibility={accessibility} />
                  </LoanCardErrorBoundary>
                </div>
              ))}
            </div>
          )}
        </section>

        {/* Closed Loans Section */}
        {closedLoans.length > 0 && (
          <section>
            <div style={{ display: "flex", alignItems: "center", gap: 12, marginBottom: 20 }}>
              <h2 style={{ margin: 0, fontSize: 20, fontWeight: 700, color: "#cbd5e1" }}>
                Closed Loans
              </h2>
              <span
                style={{
                  background: "#64748b",
                  color: "#ffffff",
                  padding: "4px 12px",
                  borderRadius: 999,
                  fontSize: 13,
                  fontWeight: 600,
                }}
              >
                {closedLoans.length}
              </span>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fill, minmax(350px, 1fr))", gap: 16 }}>
              {closedLoans.map((loan) => (
                <div
                  key={loan.id}
                  style={{
                    background: "rgba(30, 41, 59, 0.5)",
                    border: "1px solid rgba(100, 116, 139, 0.2)",
                    borderRadius: 12,
                    overflow: "hidden",
                  }}
                >
                  <LoanCardErrorBoundary>
                    <LoanCard loan={loan} accessibility={accessibility} />
                  </LoanCardErrorBoundary>
                </div>
              ))}
            </div>
          </section>
        )}
      </div>

      <style>{`
        @keyframes pulse {
          0%, 100% { opacity: 1; }
          50% { opacity: 0.5; }
        }
      `}</style>
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
 * - borrower: Stellar address of the borrower
 * - wsUrl: socket.io server base URL
 * - apiKey: optional API key for socket auth header
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
