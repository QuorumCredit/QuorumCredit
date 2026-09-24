import React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import AnalyticsDashboard from "../AnalyticsDashboard";
import { ProtocolMetrics } from "../analytics";

// ---------------------------------------------------------------------------
// Mock WebSocket
// ---------------------------------------------------------------------------

interface MockWSInstance {
  onopen: (() => void) | null;
  onmessage: ((ev: { data: string }) => void) | null;
  onclose: (() => void) | null;
  onerror: (() => void) | null;
  close: () => void;
  send: () => void;
}

let mockWSInstance: MockWSInstance | null = null;

class MockWebSocket implements MockWSInstance {
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: (() => void) | null = null;
  close = vi.fn();
  send = vi.fn();

  constructor(public url: string) {
    mockWSInstance = this;
  }
}

// Mock global fetch
const mockFetch = vi.fn();

const defaultMetrics: ProtocolMetrics = {
  tvl: 5_000_000_000,
  active_loans: 3,
  total_loans: 5,
  defaulted_loans: 1,
  default_rate: 0.2,
  total_yield_distributed: 100_000_000,
  slash_count: 1,
  fee_revenue: 25_000,
  top_borrowers: [["GABC1234", 3_000_000_000]],
  top_vouchers: [["GVOU5678", 1_000_000_000]],
  timestamp: 1000,
};

/**
 * Wrap a ProtocolMetrics object in the snapshot frame format that
 * useMetricsSocket expects: { type: "snapshot", id, metrics }.
 * Raw metrics objects are ignored by the hook.
 */
function snapshotFrame(metrics: ProtocolMetrics, id = 1): string {
  return JSON.stringify({ type: "snapshot", id, metrics });
}

beforeEach(() => {
  vi.stubGlobal("WebSocket", MockWebSocket);
  vi.stubGlobal("fetch", mockFetch);
  mockFetch.mockResolvedValue({
    ok: true,
    json: async () => ({ metrics: defaultMetrics, alerts: [] }),
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  mockWSInstance = null;
});

function renderDashboard() {
  return render(
    <AnalyticsDashboard
      apiBase="http://localhost:3000"
      wsUrl="ws://localhost:3000/api/admin/metrics/ws"
      token="test-token"
    />,
  );
}

// ---------------------------------------------------------------------------
// Smoke tests
// ---------------------------------------------------------------------------

it("renders the dashboard heading", () => {
  renderDashboard();
  expect(screen.getByText(/QuorumCredit Admin Dashboard/i)).toBeInTheDocument();
});

it("shows disconnected status before WS opens", () => {
  renderDashboard();
  expect(screen.getByLabelText(/WebSocket disconnected/i)).toBeInTheDocument();
});

it("shows live status once WebSocket opens", async () => {
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onopen?.();
  });
  expect(screen.getByLabelText(/WebSocket connected/i)).toBeInTheDocument();
});

it("renders KPI cards when WS pushes metrics", async () => {
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onopen?.();
    mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
  });
  expect(screen.getByText("TVL (XLM)")).toBeInTheDocument();
  expect(screen.getByText("Active Loans")).toBeInTheDocument();
});

it("displays TVL in XLM", async () => {
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
  });
  // 5_000_000_000 stroops = 500.00 XLM
  expect(screen.getByText("500.00")).toBeInTheDocument();
});

it("shows alert when default rate exceeds threshold", async () => {
  const highDefault = { ...defaultMetrics, default_rate: 0.06 };
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onmessage?.({ data: snapshotFrame(highDefault) });
  });
  expect(
    screen.getAllByRole("alert").some((el) => /default/i.test(el.textContent ?? "")),
  ).toBe(true);
});

it("shows no default rate alert when rate is acceptable", async () => {
  const okMetrics = { ...defaultMetrics, default_rate: 0.03 };
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onmessage?.({ data: snapshotFrame(okMetrics) });
  });
  const alerts = screen.queryAllByRole("alert");
  expect(
    alerts.every((el) => !/default rate/i.test(el.textContent ?? "")),
  ).toBe(true);
});

it("handles malformed WS message gracefully", async () => {
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onmessage?.({ data: "not-json" });
  });
  expect(screen.getByText(/QuorumCredit Admin Dashboard/i)).toBeInTheDocument();
});

it("export CSV button is rendered after data arrives", async () => {
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
  });
  expect(screen.getByText("Export CSV")).toBeInTheDocument();
});

it("export JSON button is rendered after data arrives", async () => {
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
  });
  expect(screen.getByText("Export JSON")).toBeInTheDocument();
});

it("renders filters section after data arrives", async () => {
  renderDashboard();
  await act(async () => {
    mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
  });
  expect(screen.getByLabelText("Filters")).toBeInTheDocument();
});

it("alerts section not present before data arrives", () => {
  renderDashboard();
  expect(screen.queryByLabelText("Alerts")).not.toBeInTheDocument();
});

// ---------------------------------------------------------------------------
// Issue #1513 — Initial loading state
// ---------------------------------------------------------------------------

describe("initial loading state (issue #1513)", () => {
  it("shows the loading spinner immediately on mount before any data", () => {
    renderDashboard();
    // The analytics-initial-loading container must be present right away.
    expect(screen.getByTestId("analytics-initial-loading")).toBeInTheDocument();
    // The EmptyState spinner has role="status" and aria-label="Loading".
    expect(screen.getByRole("status", { name: /loading/i })).toBeInTheDocument();
    // aria-busy informs screen readers that content is pending.
    expect(screen.getByTestId("analytics-initial-loading")).toHaveAttribute(
      "aria-busy",
      "true",
    );
  });

  it("hides the loading spinner after the first WS snapshot arrives", async () => {
    renderDashboard();
    // Spinner present before data.
    expect(screen.getByTestId("analytics-initial-loading")).toBeInTheDocument();

    await act(async () => {
      mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
    });

    // Spinner must be gone once data has arrived.
    expect(
      screen.queryByTestId("analytics-initial-loading"),
    ).not.toBeInTheDocument();
  });

  it("shows KPI cards (not the spinner) after the first WS snapshot", async () => {
    renderDashboard();
    await act(async () => {
      mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
    });
    // KPI content is visible.
    expect(screen.getByText("TVL (XLM)")).toBeInTheDocument();
    // Spinner is gone.
    expect(
      screen.queryByTestId("analytics-initial-loading"),
    ).not.toBeInTheDocument();
  });

  it("does not show KPI cards or filter controls while still loading", () => {
    renderDashboard();
    // KPI labels must not appear during the initial loading window.
    expect(screen.queryByText("TVL (XLM)")).not.toBeInTheDocument();
    // The Filters section is also hidden while loading.
    expect(screen.queryByLabelText("Filters")).not.toBeInTheDocument();
  });

  it("does not show the zero-data EmptyState while loading", () => {
    renderDashboard();
    // The zero-data empty state (variant=loans, no loading prop) must not
    // appear — only the loading spinner variant should be shown.
    expect(screen.queryByTestId("empty-state-loans")).not.toBeInTheDocument();
  });

  it("initial error panel is absent while waiting for first data", () => {
    renderDashboard();
    expect(
      screen.queryByTestId("analytics-initial-error"),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Issue #1513 — Zero-data empty state (distinct from loading state)
// ---------------------------------------------------------------------------

describe("empty state vs loading state (issue #1513)", () => {
  it("the loading spinner uses the EmptyState loading variant (data-testid=empty-state-loading)", () => {
    renderDashboard();
    // While waiting for first data, the EmptyState with loading=true is shown.
    expect(screen.getByTestId("empty-state-loading")).toBeInTheDocument();
  });

  it("loading spinner disappears after snapshot; zero-data state not shown when history > 0", async () => {
    renderDashboard();
    // Spinner present before data.
    expect(screen.getByTestId("empty-state-loading")).toBeInTheDocument();

    await act(async () => {
      mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
    });

    // After data: spinner gone, zero-data state also not shown (history has 1 entry).
    expect(screen.queryByTestId("empty-state-loading")).not.toBeInTheDocument();
    expect(screen.queryByTestId("empty-state-loans")).not.toBeInTheDocument();
    // KPI cards visible instead.
    expect(screen.getByText("TVL (XLM)")).toBeInTheDocument();
  });

  it("loading spinner and zero-data empty state are never shown simultaneously", async () => {
    renderDashboard();
    // During loading: spinner shown, zero-data not.
    expect(screen.getByTestId("empty-state-loading")).toBeInTheDocument();
    expect(screen.queryByTestId("empty-state-loans")).not.toBeInTheDocument();

    await act(async () => {
      mockWSInstance?.onmessage?.({ data: snapshotFrame(defaultMetrics) });
    });

    // After data: neither spinner nor zero-data (history has entries).
    expect(screen.queryByTestId("empty-state-loading")).not.toBeInTheDocument();
    expect(screen.queryByTestId("empty-state-loans")).not.toBeInTheDocument();
  });
});
