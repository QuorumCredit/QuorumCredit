import React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, act } from "@testing-library/react";
import { configureStore } from "@reduxjs/toolkit";
import { Provider } from "react-redux";

import { stroopsToXlm, xlmToStroops, STROOPS_PER_XLM } from "../stroops";
import loanReducer, {
  setConnected,
  upsertLoan,
  setLoans,
  setReputation,
  type LoanRecord,
} from "../loanSlice";
import LoanCard from "../LoanCard";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeStore() {
  return configureStore({ reducer: { loans: loanReducer } });
}

const activeLoan: LoanRecord = {
  id: 1,
  borrower: "GABC1234BORROWER",
  amount: 10_000_000,       // 1 XLM
  amount_repaid: 5_000_000, // 0.5 XLM repaid → 50%
  total_yield: 200_000,     // 0.02 XLM
  status: "Active",
  created_at: 1700000000,
  deadline: 1710000000,
  loan_purpose: "Business expansion",
  vouchers: [],
};

// ---------------------------------------------------------------------------
// socket.io-client mock — must be at module scope so vi.mock hoisting works.
// We use a stable object and mutate its methods in beforeEach.
// ---------------------------------------------------------------------------

type MockHandler = (data?: unknown) => void;

const mockSocket = {
  emit: vi.fn(),
  disconnect: vi.fn(),
  handlers: {} as Record<string, MockHandler>,
  on(event: string, handler: MockHandler) {
    // Capture handlers so tests can trigger them
    this.handlers[event] = handler;
  },
};

vi.mock("socket.io-client", () => ({
  io: vi.fn(() => mockSocket),
}));

// ---------------------------------------------------------------------------
// stroopsToXlm — unit tests
// ---------------------------------------------------------------------------

describe("stroopsToXlm", () => {
  it("converts 10_000_000 stroops to 1.0000000 XLM", () => {
    expect(stroopsToXlm(10_000_000)).toBe("1.0000000");
  });

  it("converts 0 stroops to 0.0000000", () => {
    expect(stroopsToXlm(0)).toBe("0.0000000");
  });

  it("produces exactly 7 decimal places", () => {
    const result = stroopsToXlm(1);
    expect(result.split(".")[1]).toHaveLength(7);
  });

  it("converts 1 stroop to 0.0000001 XLM", () => {
    expect(stroopsToXlm(1)).toBe("0.0000001");
  });

  it("converts large value: 1_000_000_000 stroops → 100.0000000 XLM", () => {
    expect(stroopsToXlm(1_000_000_000)).toBe("100.0000000");
  });

  it("converts bigint input correctly", () => {
    expect(stroopsToXlm(BigInt(10_000_000))).toBe("1.0000000");
  });

  it("handles negative stroops", () => {
    expect(stroopsToXlm(-10_000_000)).toBe("-1.0000000");
  });

  it("STROOPS_PER_XLM constant equals 10_000_000", () => {
    expect(STROOPS_PER_XLM).toBe(10_000_000);
  });
});

describe("xlmToStroops", () => {
  it("converts 1 XLM to 10_000_000 stroops", () => {
    expect(xlmToStroops(1)).toBe(10_000_000);
  });

  it("converts 0 XLM to 0", () => {
    expect(xlmToStroops(0)).toBe(0);
  });

  it("rounds fractional stroops", () => {
    expect(xlmToStroops(0.00000015)).toBe(2); // 1.5 → rounds to 2
  });
});

// ---------------------------------------------------------------------------
// Redux slice — unit tests
// ---------------------------------------------------------------------------

describe("loanSlice", () => {
  it("setConnected sets connected flag", () => {
    const store = makeStore();
    store.dispatch(setConnected(true));
    expect(store.getState().loans.connected).toBe(true);
  });

  it("upsertLoan adds a new loan", () => {
    const store = makeStore();
    store.dispatch(upsertLoan(activeLoan));
    expect(store.getState().loans.loans).toHaveLength(1);
    expect(store.getState().loans.loans[0].id).toBe(1);
  });

  it("upsertLoan updates existing loan by id", () => {
    const store = makeStore();
    store.dispatch(upsertLoan(activeLoan));
    const updated = { ...activeLoan, amount_repaid: 10_000_000 };
    store.dispatch(upsertLoan(updated));
    expect(store.getState().loans.loans).toHaveLength(1);
    expect(store.getState().loans.loans[0].amount_repaid).toBe(10_000_000);
  });

  it("setLoans replaces all loans", () => {
    const store = makeStore();
    store.dispatch(upsertLoan(activeLoan));
    const loan2: LoanRecord = { ...activeLoan, id: 2, status: "Repaid" };
    store.dispatch(setLoans([loan2]));
    expect(store.getState().loans.loans).toHaveLength(1);
    expect(store.getState().loans.loans[0].id).toBe(2);
  });

  it("setReputation stores reputation info", () => {
    const store = makeStore();
    store.dispatch(setReputation({ tier: "Gold", score: 90 }));
    expect(store.getState().loans.reputation?.tier).toBe("Gold");
    expect(store.getState().loans.reputation?.score).toBe(90);
  });

  it("lastUpdated is set after upsertLoan", () => {
    const store = makeStore();
    expect(store.getState().loans.lastUpdated).toBeNull();
    store.dispatch(upsertLoan(activeLoan));
    expect(store.getState().loans.lastUpdated).not.toBeNull();
  });
});

// ---------------------------------------------------------------------------
// useLoanSocket — WebSocket integration tests
// ---------------------------------------------------------------------------

describe("useLoanSocket via LoanStatusDashboard", () => {
  beforeEach(() => {
    // Reset the stable mock socket between tests
    mockSocket.emit = vi.fn();
    mockSocket.disconnect = vi.fn();
    mockSocket.handlers = {};
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("dispatches setConnected(true) on connect event", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();
    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC" });
      return null;
    }
    // Render so useEffect fires and registers handlers
    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });
    // Now trigger the event
    await act(async () => {
      mockSocket.handlers["connect"]?.();
    });
    expect(testStore.getState().loans.connected).toBe(true);
  });

  it("dispatches setConnected(false) on disconnect", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();
    testStore.dispatch(setConnected(true));
    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC" });
      return null;
    }
    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });
    await act(async () => {
      mockSocket.handlers["disconnect"]?.();
    });
    expect(testStore.getState().loans.connected).toBe(false);
  });

  it("dispatches upsertLoan on loan:update event", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();
    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC" });
      return null;
    }
    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });
    await act(async () => {
      mockSocket.handlers["loan:update"]?.(activeLoan);
    });
    expect(testStore.getState().loans.loans).toHaveLength(1);
    expect(testStore.getState().loans.loans[0].id).toBe(1);
  });

  it("dispatches setLoans on loan:list event", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();
    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC" });
      return null;
    }
    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });
    await act(async () => {
      mockSocket.handlers["loan:list"]?.([activeLoan]);
    });
    expect(testStore.getState().loans.loans).toHaveLength(1);
  });

  it("emits subscribe with borrower on connect", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();
    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC_ADDR" });
      return null;
    }
    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });
    expect(mockSocket.emit).toHaveBeenCalledWith("subscribe", { borrower: "GABC_ADDR" });
  });
});

// ---------------------------------------------------------------------------
// LoanCard — component rendering tests
// ---------------------------------------------------------------------------

describe("LoanCard", () => {
  function renderCard(loan: LoanRecord) {
    const testStore = makeStore();
    return render(
      <Provider store={testStore}>
        <LoanCard loan={loan} />
      </Provider>
    );
  }

  it("renders borrower address", () => {
    renderCard(activeLoan);
    expect(screen.getByText("GABC1234BORROWER")).toBeInTheDocument();
  });

  it("renders loan purpose", () => {
    renderCard(activeLoan);
    expect(screen.getByText("Business expansion")).toBeInTheDocument();
  });

  it("renders principal in XLM (7 decimal places)", () => {
    renderCard(activeLoan);
    // 10_000_000 stroops = 1.0000000 XLM
    expect(screen.getByText("1.0000000 XLM")).toBeInTheDocument();
  });

  it("renders yield in XLM", () => {
    renderCard(activeLoan);
    // 200_000 stroops = 0.0200000 XLM
    expect(screen.getByText("0.0200000 XLM")).toBeInTheDocument();
  });

  it("shows Active status badge", () => {
    renderCard(activeLoan);
    expect(screen.getByLabelText("Status: Active")).toBeInTheDocument();
  });

  it("shows Repaid status badge", () => {
    renderCard({ ...activeLoan, status: "Repaid" });
    expect(screen.getByLabelText("Status: Repaid")).toBeInTheDocument();
  });

  it("shows Defaulted status badge", () => {
    renderCard({ ...activeLoan, status: "Defaulted" });
    expect(screen.getByLabelText("Status: Defaulted")).toBeInTheDocument();
  });

  it("shows repayment progress bar with correct percentage", () => {
    renderCard(activeLoan);
    const bar = screen.getByRole("progressbar");
    expect(bar).toBeInTheDocument();
    expect(bar).toHaveAttribute("aria-valuenow", "50");
  });

  it("shows 0% repayment when amount_repaid is 0", () => {
    renderCard({ ...activeLoan, amount_repaid: 0 });
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "0");
  });

  it("caps repayment percentage at 100%", () => {
    renderCard({ ...activeLoan, amount_repaid: 20_000_000 }); // overpayment
    const bar = screen.getByRole("progressbar");
    expect(bar).toHaveAttribute("aria-valuenow", "100");
  });

  it("renders article with accessible label", () => {
    renderCard(activeLoan);
    expect(screen.getByRole("article", { name: "Loan 1" })).toBeInTheDocument();
  });
});
