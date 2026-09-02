/**
 * emptyState.test.tsx — Snapshot + behaviour tests for issue #1304.
 *
 * Covers:
 *   - EmptyState loading variant (spinner)
 *   - EmptyState empty-loans variant
 *   - EmptyState empty-vouches variant
 *   - LoanStatusDashboard renders loading state (loading=true, loans=[])
 *   - LoanStatusDashboard renders empty state (loading=false, loans=[])
 *   - LoanStatusDashboard does NOT show empty state when loans exist
 */

import React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { configureStore } from "@reduxjs/toolkit";
import { Provider } from "react-redux";
import loanReducer, {
  setConnected,
  setLoans,
  setLoading,
  type LoanRecord,
} from "../loanSlice";
import EmptyState from "../EmptyState";
import LoanCard from "../LoanCard";

// ---------------------------------------------------------------------------
// socket.io-client mock (required by useLoanSocket inside LoanStatusDashboard)
// ---------------------------------------------------------------------------

type MockHandler = (data?: unknown) => void;

const mockSocket = {
  emit: vi.fn(),
  disconnect: vi.fn(),
  handlers: {} as Record<string, MockHandler>,
  on(event: string, handler: MockHandler) {
    this.handlers[event] = handler;
  },
};

vi.mock("socket.io-client", () => ({ io: vi.fn(() => mockSocket) }));

// ---------------------------------------------------------------------------
// Helper — build a pre-configured store
// ---------------------------------------------------------------------------

function makeStore(
  overrides: Partial<{
    loans: LoanRecord[];
    connected: boolean;
    loading: boolean;
  }> = {},
) {
  const store = configureStore({ reducer: { loans: loanReducer } });
  if (overrides.loans !== undefined) store.dispatch(setLoans(overrides.loans));
  if (overrides.connected !== undefined)
    store.dispatch(setConnected(overrides.connected));
  if (overrides.loading !== undefined) store.dispatch(setLoading(overrides.loading));
  return store;
}

// ---------------------------------------------------------------------------
// Sample loan fixture
// ---------------------------------------------------------------------------

const sampleLoan: LoanRecord = {
  id: 42,
  borrower: "GABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890ABCDEFGHIJKLMNOPQRS",
  amount: 10_000_000,
  amount_repaid: 0,
  total_yield: 0,
  status: "Active",
  created_at: 1700000000,
  deadline: 1730000000,
  loan_purpose: "Test loan",
  vouchers: [],
};

// ---------------------------------------------------------------------------
// EmptyState component — snapshot tests
// ---------------------------------------------------------------------------

describe("EmptyState snapshots", () => {
  it("matches snapshot: loading state", () => {
    const { container } = render(
      <EmptyState variant="loans" loading />,
    );
    expect(container.firstChild).toMatchSnapshot();
  });

  it("matches snapshot: empty loans state", () => {
    const { container } = render(
      <EmptyState variant="loans" />,
    );
    expect(container.firstChild).toMatchSnapshot();
  });

  it("matches snapshot: empty vouches state", () => {
    const { container } = render(
      <EmptyState variant="vouches" />,
    );
    expect(container.firstChild).toMatchSnapshot();
  });

  it("matches snapshot: empty loans state (high contrast)", () => {
    const { container } = render(
      <EmptyState variant="loans" highContrast />,
    );
    expect(container.firstChild).toMatchSnapshot();
  });
});

// ---------------------------------------------------------------------------
// EmptyState component — behaviour tests
// ---------------------------------------------------------------------------

describe("EmptyState behaviour", () => {
  it("renders loading indicator when loading=true", () => {
    render(<EmptyState variant="loans" loading />);
    expect(screen.getByTestId("empty-state-loading")).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "Loading" })).toBeInTheDocument();
  });

  it("does NOT render loading indicator when loading=false (default)", () => {
    render(<EmptyState variant="loans" />);
    expect(screen.queryByTestId("empty-state-loading")).not.toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "Loading" })).not.toBeInTheDocument();
  });

  it("renders loans empty state message", () => {
    render(<EmptyState variant="loans" />);
    expect(screen.getByTestId("empty-state-loans")).toBeInTheDocument();
    expect(screen.getByText(/No loans yet/i)).toBeInTheDocument();
    expect(screen.getByText(/Request your first loan/i)).toBeInTheDocument();
  });

  it("renders vouches empty state message", () => {
    render(<EmptyState variant="vouches" />);
    expect(screen.getByTestId("empty-state-vouches")).toBeInTheDocument();
    expect(screen.getByText(/No vouches yet/i)).toBeInTheDocument();
    expect(screen.getByText(/haven't backed any borrowers/i)).toBeInTheDocument();
  });

  it("loading state is aria-busy", () => {
    render(<EmptyState variant="loans" loading />);
    expect(screen.getByTestId("empty-state-loading")).toHaveAttribute(
      "aria-busy",
      "true",
    );
  });
});

// ---------------------------------------------------------------------------
// loanSlice — loading field
// ---------------------------------------------------------------------------

describe("loanSlice loading field", () => {
  it("starts as true (initial state)", () => {
    const store = configureStore({ reducer: { loans: loanReducer } });
    expect(store.getState().loans.loading).toBe(true);
  });

  it("setLoans sets loading to false", () => {
    const store = makeStore();
    // After makeStore with no overrides, loading=true (initial)
    const fresh = configureStore({ reducer: { loans: loanReducer } });
    expect(fresh.getState().loans.loading).toBe(true);
    fresh.dispatch(setLoans([]));
    expect(fresh.getState().loans.loading).toBe(false);
  });

  it("setLoading(false) manually clears loading flag", () => {
    const store = configureStore({ reducer: { loans: loanReducer } });
    expect(store.getState().loans.loading).toBe(true);
    store.dispatch(setLoading(false));
    expect(store.getState().loans.loading).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// LoanStatusDashboard integration — loading vs empty distinction
// Test DashboardInner directly with a controlled store so we can preset state.
// ---------------------------------------------------------------------------

import { DashboardInner } from "../LoanStatusDashboard";

describe("LoanStatusDashboard — loading vs empty distinction", () => {
  beforeEach(() => {
    mockSocket.emit = vi.fn();
    mockSocket.disconnect = vi.fn();
    mockSocket.handlers = {};
  });
  afterEach(() => vi.clearAllMocks());

  it("shows loading spinner (not empty-state) while initial data is in flight", () => {
    // Default store: loading=true, loans=[]
    const store = configureStore({ reducer: { loans: loanReducer } });
    render(
      <Provider store={store}>
        <DashboardInner borrower="GABC" wsUrl="http://localhost:3000" />
      </Provider>,
    );
    expect(screen.getByTestId("empty-state-loading")).toBeInTheDocument();
    expect(screen.queryByText(/No loans yet/i)).not.toBeInTheDocument();
  });

  it("shows empty-state (not spinner) once data has loaded but borrower has no loans", () => {
    const store = makeStore({ loans: [], loading: false });
    render(
      <Provider store={store}>
        <DashboardInner borrower="GABC" wsUrl="http://localhost:3000" />
      </Provider>,
    );
    expect(screen.getByTestId("empty-state-loans")).toBeInTheDocument();
    expect(screen.getByText(/No loans yet/i)).toBeInTheDocument();
    expect(screen.queryByTestId("empty-state-loading")).not.toBeInTheDocument();
  });

  it("shows loan cards (not empty-state) when loans exist", () => {
    const store = makeStore({ loans: [sampleLoan], loading: false });
    render(
      <Provider store={store}>
        <DashboardInner borrower="GABC" wsUrl="http://localhost:3000" />
      </Provider>,
    );
    expect(screen.queryByTestId("empty-state-loans")).not.toBeInTheDocument();
    expect(screen.queryByTestId("empty-state-loading")).not.toBeInTheDocument();
    expect(screen.getByText(/GABCDEFGHI/)).toBeInTheDocument();
  });
});
