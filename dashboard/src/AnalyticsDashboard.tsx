import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AccessibilitySettings } from "./LoanCard";
import {
  LineChart,
  Line,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
  Legend,
} from "recharts";
import {
  Alert,
  AlertThresholds,
  DEFAULT_THRESHOLDS,
  MetricsFilter,
  ProtocolMetrics,
  checkAlerts,
  downloadFile,
  metricsToCSV,
} from "./analytics";
import EmptyState from "./EmptyState";
import { useMetricsSocket } from "./useMetricsSocket";
import { DashboardErrorBoundary } from "./ErrorBoundary";
import EmptyState from "./EmptyState";

const XLM = 10_000_000;

/**
 * Format stroops as a locale-aware XLM decimal string (2 d.p.).
 * Replaces the old ad-hoc `(stroops / XLM).toFixed(2)`.
 */
function fmtXlm(stroops: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  }).format(stroops / XLM);
}

/**
 * Format a ratio (0–1) as a locale-aware percentage string (1 d.p.).
 */
function fmtPct(ratio: number, locale: string): string {
  return new Intl.NumberFormat(locale, {
    style: "percent",
    minimumFractionDigits: 1,
    maximumFractionDigits: 1,
  }).format(ratio);
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

interface AnalyticsDashboardProps {
  apiBase: string;
  wsUrl: string;
  token: string;
  thresholds?: AlertThresholds;
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

const AnalyticsDashboard: React.FC<AnalyticsDashboardProps> = ({
  apiBase,
  wsUrl,
  token,
  thresholds = DEFAULT_THRESHOLDS,
}) => {
  const { t, i18n } = useTranslation();
  const locale = i18n.language || "en";

  const [history, setHistory] = useState<ProtocolMetrics[]>([]);
  const [alerts, setAlerts] = useState<Alert[]>([]);
  const [filter, setFilter] = useState<MetricsFilter>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** True until the first successful snapshot arrives (via WS or fetch). */
  const [initialLoading, setInitialLoading] = useState(true);
  /** Non-null when the initial data load fails (distinct from per-request errors). */
  const [initialError, setInitialError] = useState<string | null>(null);
  const [accessibility, setAccessibility] = useState<AccessibilitySettings>(() => {
    if (typeof window === "undefined")
      return { colorblindFriendly: false, highContrast: false };
    try {
      const stored = window.localStorage.getItem("quorum-dashboard-accessibility");
      return stored
        ? JSON.parse(stored)
        : { colorblindFriendly: false, highContrast: false };
    } catch {
      return { colorblindFriendly: false, highContrast: false };
    }
  });

  const peakTvlRef = useRef(0);

  useEffect(() => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(
        "quorum-dashboard-accessibility",
        JSON.stringify(accessibility),
      );
    }
  }, [accessibility]);

  const [retryKey, setRetryKey] = useState(0);

  const { latest, connected, gaveUp } = useMetricsSocket(wsUrl, undefined, undefined, undefined, retryKey);

  // Apply incoming WS snapshot
  useEffect(() => {
    if (!latest) return;
    if (latest.tvl > peakTvlRef.current) peakTvlRef.current = latest.tvl;
    setHistory((h) => [...h.slice(-99), latest]);
    setAlerts(checkAlerts(latest, peakTvlRef.current, thresholds));
    // First snapshot has arrived — clear the initial loading/error gates.
    setInitialLoading(false);
    setInitialError(null);
  }, [latest, thresholds]);

  // Fetch on demand / filter change
  const fetchMetrics = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(`${apiBase}/api/admin/metrics`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          Authorization: `Bearer ${token}`,
        },
        body: JSON.stringify({
          loans: [],
          vouches: [],
          slash_count: 0,
          fee_revenue: 0,
          filter,
        }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const { metrics }: { metrics: ProtocolMetrics; alerts: Alert[] } =
        await res.json();
      if (metrics.tvl > peakTvlRef.current) peakTvlRef.current = metrics.tvl;
      setHistory((h) => [...h.slice(-99), metrics]);
      setAlerts(checkAlerts(metrics, peakTvlRef.current, thresholds));
      // First successful fetch — clear the initial loading/error gates.
      setInitialLoading(false);
      setInitialError(null);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : "Unknown error";
      setError(msg);
      // Only surface as an initial error if we have never received any data.
      if (initialLoading) {
        setInitialError(msg);
      }
    } finally {
      setLoading(false);
    }
  }, [apiBase, token, filter, thresholds, initialLoading]);

  const handleExportCSV = () => {
    downloadFile(metricsToCSV(history), "metrics.csv", "text/csv");
  };
  const handleExportJSON = () => {
    downloadFile(JSON.stringify(history, null, 2), "metrics.json", "application/json");
  };

  const current = history.length > 0 ? history[history.length - 1] : undefined;

  const toggleButtonStyle = (active: boolean): React.CSSProperties => ({
    border: active ? "2px solid #2563eb" : "1px solid #cbd5e1",
    background: accessibility.highContrast
      ? "#000000"
      : active
        ? "#eff6ff"
        : "#ffffff",
    color: accessibility.highContrast ? "#ffffff" : active ? "#1d4ed8" : "#334155",
    borderRadius: 999,
    padding: "6px 12px",
    fontWeight: 600,
    cursor: "pointer",
  });

  return (
    <div style={{ padding: 24 }}>
      {/* Accessibility toggles */}
      <div style={{ display: "flex", gap: 8, marginBottom: 16 }}>
        <button
          onClick={() =>
            setAccessibility((prev) => ({
              ...prev,
              colorblindFriendly: !prev.colorblindFriendly,
            }))
          }
          style={toggleButtonStyle(Boolean(accessibility.colorblindFriendly))}
        >
          {t("analytics.colorblindFriendly")}
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
          {t("analytics.highContrast")}
        </button>
      </div>

      {/* Header */}
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          marginBottom: 20,
        }}
      >
        <h1 style={{ margin: 0, fontSize: 24, fontWeight: 700 }}>
          {t("analytics.pageTitle")}
        </h1>
        <span
          aria-label={connected ? "WebSocket connected" : "WebSocket disconnected"}
          style={{
            fontSize: 13,
            fontWeight: 600,
            color: connected ? "#10b981" : "#ef4444",
          }}
        >
          {connected ? t("analytics.statusLive") : t("analytics.statusDisconnected")}
        </span>
      </div>

      {/* Give-up state — shown when the WebSocket exhausts all reconnect attempts */}
      {gaveUp && (
        <EmptyState
          variant="give-up"
          onRetry={() => setRetryKey((k) => k + 1)}
        />
      )}

      {/* Alerts */}
      {!gaveUp && alerts.length > 0 && (
        <div aria-label="Alerts" style={{ marginBottom: 16 }}>
          {alerts.map((a) => (
            <div
              key={a.message}
              role="alert"
              style={{
                background: "#fef3c7",
                border: "1px solid #f59e0b",
                borderRadius: 8,
                padding: "10px 14px",
                marginBottom: 8,
                color: "#92400e",
                fontSize: 14,
              }}
            >
              ⚠ {a.message}
            </div>
          ))}
        </div>
      )}

      {/* Filters */}
      {!gaveUp && (
      <div
        aria-label="Filters"
        style={{
          display: "flex",
          gap: 12,
          marginBottom: 16,
          flexWrap: "wrap",
          alignItems: "center",
        }}
      >
        <label style={{ fontSize: 13 }}>
          {t("analytics.filtersFrom")}{" "}
          <input
            type="date"
            onChange={(e) =>
              setFilter((f) => ({
                ...f,
                from: e.target.value
                  ? Math.floor(new Date(e.target.value).getTime() / 1000)
                  : undefined,
              }))
            }
          />
        </label>
        <label style={{ fontSize: 13 }}>
          {t("analytics.filtersTo")}{" "}
          <input
            type="date"
            onChange={(e) =>
              setFilter((f) => ({
                ...f,
                to: e.target.value
                  ? Math.floor(new Date(e.target.value).getTime() / 1000)
                  : undefined,
              }))
            }
          />
        </label>
        <label style={{ fontSize: 13 }}>
          {t("analytics.filterLoanSize")}{" "}
          <select
            onChange={(e) =>
              setFilter((f) => ({
                ...f,
                loan_size: (e.target.value as MetricsFilter["loan_size"]) || undefined,
              }))
            }
          >
            <option value="">{t("analytics.filterAll")}</option>
            <option value="small">{t("analytics.filterSmall")}</option>
            <option value="medium">{t("analytics.filterMedium")}</option>
            <option value="large">{t("analytics.filterLarge")}</option>
          </select>
        </label>
        <button onClick={fetchMetrics} style={{ fontSize: 13, padding: "4px 12px" }}>
          {loading ? t("analytics.fetchLoading") : t("analytics.fetchButton")}
        </button>
      </div>
      )}

      {!gaveUp && error && (
        <p style={{ color: "#ef4444", fontSize: 14 }}>
          {t("analytics.errorPrefix")} {error}
        </p>
      )}

      {/* ------------------------------------------------------------------ */}
      {/* Initial error state — first load failed and no data exists yet      */}
      {/* ------------------------------------------------------------------ */}
      {initialError && history.length === 0 && (
        <div
          data-testid="analytics-initial-error"
          style={{
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            gap: 12,
            padding: "48px 24px",
            borderRadius: 16,
            border: "1px dashed #ef4444",
            textAlign: "center",
            background: accessibility.highContrast ? "#111827" : "#1e293b",
          }}
          role="alert"
          aria-live="assertive"
        >
          <span style={{ fontSize: 40 }} aria-hidden="true">⚠️</span>
          <p
            style={{
              color: "#ef4444",
              fontSize: 17,
              fontWeight: 600,
              margin: 0,
            }}
          >
            {t("analytics.errorTitle")}
          </p>
          <p style={{ color: "#94a3b8", fontSize: 14, margin: 0 }}>
            {initialError}
          </p>
          <button
            onClick={() => {
              setInitialError(null);
              setInitialLoading(true);
              fetchMetrics();
            }}
            style={{
              marginTop: 8,
              padding: "8px 20px",
              borderRadius: 8,
              border: "none",
              background: "#3b82f6",
              color: "#ffffff",
              fontWeight: 600,
              cursor: "pointer",
              fontSize: 14,
            }}
          >
            {t("analytics.retryButton")}
          </button>
        </div>
      )}

      {/* Only render the main dashboard content once initial loading is done */}
      {!initialLoading && !initialError && (
        <>
          {/* Alerts */}
          {alerts.length > 0 && (
            <div aria-label="Alerts" style={{ marginBottom: 16 }}>
              {alerts.map((a) => (
                <div
                  key={a.message}
                  role="alert"
                  style={{
                    background: "#fef3c7",
                    border: "1px solid #f59e0b",
                    borderRadius: 8,
                    padding: "10px 14px",
                    marginBottom: 8,
                    color: "#92400e",
                    fontSize: 14,
                  }}
                >
                  ⚠ {a.message}
                </div>
              ))}
            </div>
          )}

          {/* Filters */}
          <div
            aria-label="Filters"
            style={{
              display: "flex",
              gap: 12,
              marginBottom: 16,
              flexWrap: "wrap",
              alignItems: "center",
            }}
          >
            <label style={{ fontSize: 13 }}>
              {t("analytics.filtersFrom")}{" "}
              <input
                type="date"
                onChange={(e) =>
                  setFilter((f) => ({
                    ...f,
                    from: e.target.value
                      ? Math.floor(new Date(e.target.value).getTime() / 1000)
                      : undefined,
                  }))
                }
              />
            </label>
            <label style={{ fontSize: 13 }}>
              {t("analytics.filtersTo")}{" "}
              <input
                type="date"
                onChange={(e) =>
                  setFilter((f) => ({
                    ...f,
                    to: e.target.value
                      ? Math.floor(new Date(e.target.value).getTime() / 1000)
                      : undefined,
                  }))
                }
              />
            </label>
            <label style={{ fontSize: 13 }}>
              {t("analytics.filterLoanSize")}{" "}
              <select
                onChange={(e) =>
                  setFilter((f) => ({
                    ...f,
                    loan_size: (e.target.value as MetricsFilter["loan_size"]) || undefined,
                  }))
                }
              >
                <option value="">{t("analytics.filterAll")}</option>
                <option value="small">{t("analytics.filterSmall")}</option>
                <option value="medium">{t("analytics.filterMedium")}</option>
                <option value="large">{t("analytics.filterLarge")}</option>
              </select>
            </label>
            <button onClick={fetchMetrics} style={{ fontSize: 13, padding: "4px 12px" }}>
              {loading ? t("analytics.fetchLoading") : t("analytics.fetchButton")}
            </button>
          </div>

          {error && (
            <p style={{ color: "#ef4444", fontSize: 14 }}>
              {t("analytics.errorPrefix")} {error}
            </p>
          )}

          {/* Zero-data empty state — data has loaded but history is empty */}
          {history.length === 0 && (
            <EmptyState
              variant="loans"
              highContrast={accessibility.highContrast}
            />
          )}

          {/* KPI Cards */}
          {current && (
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "repeat(auto-fill, minmax(160px, 1fr))",
                gap: 16,
                marginBottom: 24,
              }}
            >
              <KpiCard
                label={t("analytics.kpiTvl")}
                value={fmtXlm(current.tvl, locale)}
                accessibility={accessibility}
              />
              <KpiCard
                label={t("analytics.kpiActiveLoans")}
                value={new Intl.NumberFormat(locale).format(current.active_loans)}
                accessibility={accessibility}
              />
              <KpiCard
                label={t("analytics.kpiDefaultRate")}
                value={fmtPct(current.default_rate, locale)}
                highlight={current.default_rate > thresholds.max_default_rate}
                accessibility={accessibility}
              />
              <KpiCard
                label={t("analytics.kpiTotalBorrowers")}
                value={new Intl.NumberFormat(locale).format(current.total_borrowers)}
                accessibility={accessibility}
              />
              <KpiCard
                label={t("analytics.kpiTotalVouchers")}
                value={new Intl.NumberFormat(locale).format(current.total_vouchers)}
                accessibility={accessibility}
              />
              <KpiCard
                label={t("analytics.kpiAvgLoanSize")}
                value={fmtXlm(current.avg_loan_size, locale)}
                accessibility={accessibility}
              />
            </div>
          )}

          {/* TVL over time chart */}
          {history.length > 1 && (
            <div style={{ marginBottom: 32 }}>
              <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>
                {t("analytics.tvlOverTime")}
              </h2>
              <ResponsiveContainer width="100%" height={200}>
                <LineChart data={history}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="timestamp" hide />
                  <YAxis tickFormatter={(v: number) => fmtXlm(v, locale)} />
                  <Tooltip formatter={(v: number) => [`${fmtXlm(v, locale)} XLM`]} />
                  <Legend />
                  <Line type="monotone" dataKey="tvl" dot={false} stroke="#3b82f6" />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}

          {/* Default rate chart */}
          {history.length > 1 && (
            <div style={{ marginBottom: 32 }}>
              <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>
                {t("analytics.defaultRateOverTime")}
              </h2>
              <ResponsiveContainer width="100%" height={200}>
                <LineChart data={history}>
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="timestamp" hide />
                  <YAxis tickFormatter={(v: number) => fmtPct(v, locale)} />
                  <Tooltip formatter={(v: number) => [fmtPct(v, locale)]} />
                  <Legend />
                  <Line
                    type="monotone"
                    dataKey="default_rate"
                    dot={false}
                    stroke="#ef4444"
                  />
                </LineChart>
              </ResponsiveContainer>
            </div>
          )}

          {/* Top borrowers */}
          {current && current.top_borrowers.length > 0 && (
            <div style={{ marginBottom: 32 }}>
              <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>
                {t("analytics.topBorrowers")}
              </h2>
              <ResponsiveContainer width="100%" height={200}>
                <BarChart
                  data={current.top_borrowers.map(([addr, amt]) => ({
                    addr: addr.slice(0, 8) + "…",
                    amount: amt / XLM,
                  }))}
                  aria-label={t("analytics.topBorrowers")}
                >
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="addr" />
                  <YAxis
                    tickFormatter={(v: number) =>
                      new Intl.NumberFormat(locale, {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                      }).format(v)
                    }
                  />
                  <Tooltip
                    formatter={(v: number) => [
                      `${new Intl.NumberFormat(locale, {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                      }).format(v)} XLM`,
                    ]}
                  />
                  <Bar dataKey="amount" fill="#3b82f6" />
                </BarChart>
              </ResponsiveContainer>
            </div>
          )}

          {/* Top vouchers */}
          {current && current.top_vouchers.length > 0 && (
            <div style={{ marginBottom: 32 }}>
              <h2 style={{ fontSize: 16, fontWeight: 600, marginBottom: 12 }}>
                {t("analytics.topVouchers")}
              </h2>
              <ResponsiveContainer width="100%" height={200}>
                <BarChart
                  data={current.top_vouchers.map(([addr, stake]) => ({
                    addr: addr.slice(0, 8) + "…",
                    stake: stake / XLM,
                  }))}
                  aria-label={t("analytics.topVouchers")}
                >
                  <CartesianGrid strokeDasharray="3 3" />
                  <XAxis dataKey="addr" />
                  <YAxis
                    tickFormatter={(v: number) =>
                      new Intl.NumberFormat(locale, {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                      }).format(v)
                    }
                  />
                  <Tooltip
                    formatter={(v: number) => [
                      `${new Intl.NumberFormat(locale, {
                        minimumFractionDigits: 2,
                        maximumFractionDigits: 2,
                      }).format(v)} XLM`,
                    ]}
                  />
                  <Bar dataKey="stake" fill="#10b981" />
                </BarChart>
              </ResponsiveContainer>
            </div>
          )}

          {/* Export */}
          <div style={{ display: "flex", gap: 8 }}>
            <button onClick={handleExportCSV}>{t("analytics.exportCSV")}</button>
            <button onClick={handleExportJSON}>{t("analytics.exportJSON")}</button>
          </div>
        </>
      )}

      {/* Export */}
      {!gaveUp && (
      <div style={{ display: "flex", gap: 8 }}>
        <button onClick={handleExportCSV}>{t("analytics.exportCSV")}</button>
        <button onClick={handleExportJSON}>{t("analytics.exportJSON")}</button>
      </div>
      )}
    </div>
  );
};

// ---------------------------------------------------------------------------
// KPI card sub-component
// ---------------------------------------------------------------------------

const KpiCard: React.FC<{
  label: string;
  value: string | number;
  highlight?: boolean;
  accessibility?: AccessibilitySettings;
}> = ({ label, value, highlight, accessibility }) => (
  <div
    style={{
      background: accessibility?.highContrast ? "#111827" : "#f8fafc",
      border: highlight
        ? "2px solid #ef4444"
        : `1px solid ${accessibility?.highContrast ? "#334155" : "#e2e8f0"}`,
      borderRadius: 12,
      padding: "16px 20px",
    }}
  >
    <p
      style={{
        fontSize: 11,
        fontWeight: 600,
        color: "#64748b",
        textTransform: "uppercase",
        letterSpacing: "0.08em",
        margin: "0 0 6px",
      }}
    >
      {label}
    </p>
    <p
      style={{
        fontSize: 22,
        fontWeight: 700,
        color: highlight
          ? "#ef4444"
          : accessibility?.highContrast
            ? "#ffffff"
            : "#0f172a",
        margin: 0,
      }}
    >
      {value}
    </p>
  </div>
);

export default AnalyticsDashboard;

// ---------------------------------------------------------------------------
// Re-export wrapped in DashboardErrorBoundary for direct page-level use
// ---------------------------------------------------------------------------

/**
 * AnalyticsDashboardWithBoundary — the AnalyticsDashboard wrapped in a
 * DashboardErrorBoundary so any catastrophic render error degrades
 * gracefully instead of blanking the page.
 */
export const AnalyticsDashboardWithBoundary: React.FC<AnalyticsDashboardProps> = (
  props,
) => (
  <DashboardErrorBoundary>
    <AnalyticsDashboard {...props} />
  </DashboardErrorBoundary>
);
