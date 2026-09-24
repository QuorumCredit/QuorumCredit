import React, { Component, ErrorInfo, ReactNode } from "react";

// ---------------------------------------------------------------------------
// Generic ErrorBoundary
// ---------------------------------------------------------------------------

export interface ErrorBoundaryProps {
  /** Content to render when no error has occurred */
  children: ReactNode;
  /** Custom fallback UI; receives the caught error */
  fallback?: (error: Error) => ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/**
 * Generic React Error Boundary.
 *
 * Catches render-time exceptions thrown by any child component and renders
 * the supplied `fallback` (or a default message) instead of crashing the
 * entire mounted tree.
 *
 * Usage:
 * ```tsx
 * <ErrorBoundary fallback={(err) => <p>Something went wrong: {err.message}</p>}>
 *   <MyComponent />
 * </ErrorBoundary>
 * ```
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    // Log to console in development; in production this could be sent to
    // an error-reporting service (e.g., Sentry).
    console.error("[ErrorBoundary] Caught render error:", error, info.componentStack);
  }

  render() {
    const { error } = this.state;
    if (error) {
      return this.props.fallback
        ? this.props.fallback(error)
        : (
          <div role="alert" aria-label="Component error">
            <p>Something went wrong rendering this section.</p>
          </div>
        );
    }
    return this.props.children;
  }
}

// ---------------------------------------------------------------------------
// LoanCardErrorBoundary — narrow boundary for individual loan cards
// ---------------------------------------------------------------------------

/**
 * Error boundary scoped to a single LoanCard.
 *
 * When a LoanCard throws during render (e.g. due to a malformed or
 * unexpectedly-shaped payload from the WebSocket), this boundary catches the
 * error and renders a compact error tile in place of the broken card.
 * All other cards in the list remain unaffected.
 */
export class LoanCardErrorBoundary extends Component<
  { children: ReactNode },
  ErrorBoundaryState
> {
  constructor(props: { children: ReactNode }) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[LoanCardErrorBoundary] Caught render error in LoanCard:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          role="alert"
          aria-label="Loan card error"
          style={{
            background: "rgba(239, 68, 68, 0.08)",
            border: "1px solid rgba(239, 68, 68, 0.4)",
            borderRadius: 12,
            padding: 20,
            color: "#ef4444",
            fontSize: 13,
            display: "flex",
            flexDirection: "column",
            gap: 8,
          }}
        >
          <span style={{ fontWeight: 700 }}>⚠ Unable to display this loan</span>
          <span style={{ color: "#94a3b8", fontSize: 12 }}>
            The loan record contains unexpected data and could not be rendered.
          </span>
        </div>
      );
    }
    return this.props.children;
  }
}

// ---------------------------------------------------------------------------
// DashboardErrorBoundary — top-level boundary for the entire dashboard view
// ---------------------------------------------------------------------------

/**
 * Top-level error boundary that wraps the full dashboard.
 *
 * Catches unexpected errors that slip past narrower boundaries (e.g. errors
 * in the Redux-connected view layer itself) and shows a full-page fallback
 * instead of an unrecoverable blank screen.
 */
export class DashboardErrorBoundary extends Component<
  { children: ReactNode },
  ErrorBoundaryState
> {
  constructor(props: { children: ReactNode }) {
    super(props);
    this.state = { error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[DashboardErrorBoundary] Fatal dashboard render error:", error, info.componentStack);
  }

  render() {
    if (this.state.error) {
      return (
        <div
          role="alert"
          aria-label="Dashboard error"
          style={{
            minHeight: "100vh",
            background: "#0f172a",
            color: "#f1f5f9",
            display: "flex",
            flexDirection: "column",
            alignItems: "center",
            justifyContent: "center",
            gap: 16,
            padding: 32,
            fontFamily: "system-ui, -apple-system, sans-serif",
          }}
        >
          <span style={{ fontSize: 48 }}>⚠</span>
          <h2 style={{ margin: 0, fontSize: 24, color: "#ef4444" }}>Dashboard Error</h2>
          <p style={{ margin: 0, color: "#94a3b8", textAlign: "center", maxWidth: 480 }}>
            An unexpected error occurred while rendering the dashboard. Please
            reload the page. If the problem persists, contact support.
          </p>
          <button
            type="button"
            onClick={() => window.location.reload()}
            style={{
              marginTop: 8,
              padding: "10px 24px",
              background: "#3b82f6",
              color: "#ffffff",
              border: "none",
              borderRadius: 8,
              fontWeight: 600,
              cursor: "pointer",
              fontSize: 14,
            }}
          >
            Reload
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}
