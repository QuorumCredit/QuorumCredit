import React, { useEffect, useState } from "react";
import { Provider, useSelector } from "react-redux";
import { store, RootState } from "./store";
import { useLoanSocket } from "./useLoanSocket";
import LoanCard, { type AccessibilitySettings } from "./LoanCard";

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

  const toggleButtonStyle = (active: boolean) => ({
    border: active ? "2px solid #2563eb" : "1px solid #cbd5e1",
    background: accessibility.highContrast ? "#000000" : active ? "#eff6ff" : "#ffffff",
    color: accessibility.highContrast ? "#ffffff" : active ? "#1d4ed8" : "#334155",
    borderRadius: 999,
    padding: "6px 12px",
    fontWeight: 600,
    cursor: "pointer",
  });

  return (
    <div
      aria-label="Loan Status Dashboard"
      style={{ fontFamily: "system-ui", padding: 24, maxWidth: 900, margin: "0 auto", background: accessibility.highContrast ? "#000000" : "#ffffff", color: accessibility.highContrast ? "#ffffff" : "#0f172a" }}
    >
      <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginBottom: 16 }}>
        <button
          type="button"
          aria-pressed={Boolean(accessibility.colorblindFriendly)}
          onClick={() => setAccessibility((prev) => ({ ...prev, colorblindFriendly: !prev.colorblindFriendly }))}
          style={toggleButtonStyle(Boolean(accessibility.colorblindFriendly))}
        >
          Colorblind-friendly mode
        </button>
        <button
          type="button"
          aria-pressed={Boolean(accessibility.highContrast)}
          onClick={() => setAccessibility((prev) => ({ ...prev, highContrast: !prev.highContrast }))}
          style={toggleButtonStyle(Boolean(accessibility.highContrast))}
        >
          High contrast
        </button>
      </div>

      {/* Header */}
      <header style={{ display: "flex", alignItems: "center", gap: 10, marginBottom: 20, flexWrap: "wrap" }}>
        <h1 style={{ margin: 0, fontSize: 22, color: accessibility.highContrast ? "#ffffff" : "#0f172a" }}>Loan Status Dashboard</h1>
        <span
          aria-label={connected ? "WebSocket connected" : "WebSocket disconnected"}
          style={{
            width: 10,
            height: 10,
            borderRadius: "50%",
            background: connected ? "#22c55e" : "#ef4444",
            display: "inline-block",
          }}
        />
        <span style={{ fontSize: 12, color: "#6b7280" }}>{connected ? "Live" : "Disconnected"}</span>
        {lastUpdated && (
          <span style={{ fontSize: 11, color: "#94a3b8", marginLeft: "auto" }}>
            Updated {new Date(lastUpdated).toLocaleTimeString()}
          </span>
        )}
      </header>

      {/* Reputation */}
      {reputation && (
        <section
          aria-label="Reputation"
          style={{
            background: accessibility.highContrast ? "#111827" : "#f8fafc",
            border: accessibility.highContrast ? "1px solid #ffffff" : "1px solid #e2e8f0",
            borderRadius: 10,
            padding: "12px 20px",
            marginBottom: 20,
            display: "flex",
            gap: 32,
            flexWrap: "wrap",
          }}
        >
          <div>
            <div style={{ fontSize: 11, color: accessibility.highContrast ? "#cbd5e1" : "#64748b" }}>Reputation Tier</div>
            <div style={{ fontWeight: 700, fontSize: 18, color: accessibility.highContrast ? "#fbbf24" : "#6366f1" }}>{reputation.tier}</div>
          </div>
          <div>
            <div style={{ fontSize: 11, color: accessibility.highContrast ? "#cbd5e1" : "#64748b" }}>Score</div>
            <div style={{ fontWeight: 700, fontSize: 18 }}>{reputation.score}</div>
          </div>
        </section>
      )}

      {/* Active Loans */}
      <section aria-label="Active Loans" style={{ marginBottom: 24 }}>
        <h2 style={{ fontSize: 16, color: accessibility.highContrast ? "#fbbf24" : "#1d4ed8", marginBottom: 12 }}>
          Active Loans ({activeLoans.length})
        </h2>
        {activeLoans.length === 0 ? (
          <p style={{ color: accessibility.highContrast ? "#e2e8f0" : "#94a3b8", fontSize: 14 }}>No active loans.</p>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            {activeLoans.map((loan) => (
              <LoanCard key={loan.id} loan={loan} accessibility={accessibility} />
            ))}
          </div>
        )}
      </section>

      {/* Closed Loans */}
      {closedLoans.length > 0 && (
        <section aria-label="Closed Loans">
          <h2 style={{ fontSize: 16, color: "#64748b", marginBottom: 12 }}>
            Closed Loans ({closedLoans.length})
          </h2>
          <div style={{ display: "flex", flexDirection: "column", gap: 12 }}>
            {closedLoans.map((loan) => (
              <LoanCard key={loan.id} loan={loan} accessibility={accessibility} />
            ))}
          </div>
        </section>
      )}
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
 */
const LoanStatusDashboard: React.FC<LoanStatusDashboardProps> = (props) => (
  <Provider store={store}>
    <DashboardInner {...props} />
  </Provider>
);

export default LoanStatusDashboard;
