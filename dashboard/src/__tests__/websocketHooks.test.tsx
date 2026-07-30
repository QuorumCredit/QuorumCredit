/**
 * Tests for issues #1298 and #1299:
 *
 * #1298 — useMetricsSocket must not treat `resync_required` control frames as
 *         ProtocolMetrics data; `latest` must remain unaffected.
 *
 * #1299 — Both hooks must track the last-seen event id and request replay
 *         from that cursor on reconnect so events published during a disconnect
 *         window are not silently dropped.
 */
import React from "react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, act } from "@testing-library/react";
import { configureStore } from "@reduxjs/toolkit";
import { Provider } from "react-redux";
import loanReducer, { type LoanRecord } from "../loanSlice";

// ---------------------------------------------------------------------------
// Fake WebSocket — lets tests control open/message/close lifecycle imperatively
// ---------------------------------------------------------------------------

interface FakeWsInstance {
  url: string;
  readyState: number;
  onopen: (() => void) | null;
  onmessage: ((ev: { data: string }) => void) | null;
  onclose: (() => void) | null;
  onerror: ((err: unknown) => void) | null;
  send: ReturnType<typeof vi.fn>;
  close: () => void;
  // Test helpers
  _open(): void;
  _message(data: string): void;
  _close(): void;
}

let createdSockets: FakeWsInstance[] = [];

class FakeWebSocket implements FakeWsInstance {
  static OPEN = 1;
  static CLOSED = 3;

  url: string;
  readyState = 1; // OPEN by default after construction in tests
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: string }) => void) | null = null;
  onclose: (() => void) | null = null;
  onerror: ((err: unknown) => void) | null = null;
  send = vi.fn();

  constructor(url: string) {
    this.url = url;
    createdSockets.push(this);
  }

  close() {
    this.readyState = FakeWebSocket.CLOSED;
    this.onclose?.();
  }

  // Helpers for tests to drive the lifecycle
  _open() {
    this.readyState = FakeWebSocket.OPEN;
    this.onopen?.();
  }

  _message(data: string) {
    this.onmessage?.({ data });
  }

  _close() {
    this.close();
  }
}

// ---------------------------------------------------------------------------
// socket.io mock (re-used from loanDashboard.test.tsx pattern)
// ---------------------------------------------------------------------------

type MockHandler = (data?: unknown) => void;

const mockSocket = {
  emit: vi.fn(),
  disconnect: vi.fn(),
  handlers: {} as Record<string, MockHandler>,
  on(event: string, handler: MockHandler) {
    this.handlers[event] = handler;
  },
  // simulate socket.io's automatic reconnect: fires connect again
  _reconnect() {
    this.handlers["disconnect"]?.();
    this.handlers["connect"]?.();
  },
};

// socket.io-client is imported as a default import: `import io from "socket.io-client"`
// vitest needs both a `default` export (for ESM default import) and an `io` named export.
vi.mock("socket.io-client", () => {
  const factory = vi.fn(() => mockSocket);
  return { default: factory, io: factory };
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeStore() {
  return configureStore({ reducer: { loans: loanReducer } });
}

const sampleMetrics = {
  tvl: 1000,
  active_loans: 5,
  total_loans: 10,
  defaulted_loans: 1,
  default_rate: 0.1,
  total_yield_distributed: 200,
  slash_count: 1,
  fee_revenue: 50,
  top_borrowers: [] as [string, number][],
  top_vouchers: [] as [string, number][],
  timestamp: 1700000000,
};

const activeLoan: LoanRecord = {
  id: 1,
  borrower: "GABC1234BORROWER",
  amount: 10_000_000,
  amount_repaid: 0,
  total_yield: 200_000,
  status: "Active",
  created_at: 1700000000,
  deadline: 1710000000,
  loan_purpose: "Business expansion",
  vouchers: [],
};

// ---------------------------------------------------------------------------
// #1298 — useMetricsSocket discriminator tests
// ---------------------------------------------------------------------------

describe("#1298 useMetricsSocket — resync_required frame handling", () => {
  beforeEach(() => {
    createdSockets = [];
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it("sets latest from a snapshot frame", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");
    let capturedLatest: unknown = undefined;

    function TestHook() {
      const { latest } = useMetricsSocket("ws://localhost:3001/ws/metrics");
      capturedLatest = latest;
      return null;
    }

    await act(async () => {
      render(<TestHook />);
    });

    const ws = createdSockets[0];
    await act(async () => {
      ws._open();
      ws._message(JSON.stringify({ type: "snapshot", id: 42, metrics: sampleMetrics }));
    });

    expect(capturedLatest).toEqual(sampleMetrics);
  });

  it("does NOT update latest when a resync_required frame is received", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");
    let capturedLatest: unknown = undefined;

    function TestHook() {
      const { latest } = useMetricsSocket("ws://localhost:3001/ws/metrics");
      capturedLatest = latest;
      return null;
    }

    await act(async () => {
      render(<TestHook />);
    });

    const ws = createdSockets[0];
    await act(async () => {
      ws._open();
      // First deliver a real snapshot so latest is set to something meaningful
      ws._message(JSON.stringify({ type: "snapshot", id: 10, metrics: sampleMetrics }));
    });

    const snapshotValue = capturedLatest;
    expect(snapshotValue).not.toBeNull();

    // Now deliver a resync_required control frame — latest must NOT change
    await act(async () => {
      ws._message(
        JSON.stringify({ type: "resync_required", reason: "queue_overflow", resumeFrom: 10 })
      );
    });

    // latest must still equal the snapshot, not the garbled control-frame object
    expect(capturedLatest).toEqual(snapshotValue);
  });

  it("resync_required closes the socket and triggers reconnect", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");

    function TestHook() {
      useMetricsSocket("ws://localhost:3001/ws/metrics", 100);
      return null;
    }

    await act(async () => {
      render(<TestHook />);
    });

    const ws = createdSockets[0];
    await act(async () => {
      ws._open();
      ws._message(JSON.stringify({ type: "snapshot", id: 5, metrics: sampleMetrics }));
    });

    expect(createdSockets).toHaveLength(1);

    await act(async () => {
      ws._message(
        JSON.stringify({ type: "resync_required", reason: "queue_overflow", resumeFrom: 5 })
      );
    });

    // Advance timers so the reconnect setTimeout fires
    await act(async () => {
      vi.advanceTimersByTime(200);
    });

    // A second WebSocket should have been created for the reconnect
    expect(createdSockets.length).toBeGreaterThanOrEqual(2);
  });

  it("reconnect after resync_required uses the resumeFrom cursor", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");

    function TestHook() {
      useMetricsSocket("ws://localhost:3001/ws/metrics", 100);
      return null;
    }

    await act(async () => {
      render(<TestHook />);
    });

    const ws = createdSockets[0];
    await act(async () => {
      ws._open();
      ws._message(JSON.stringify({ type: "snapshot", id: 7, metrics: sampleMetrics }));
      ws._message(
        JSON.stringify({ type: "resync_required", reason: "queue_overflow", resumeFrom: 7 })
      );
    });

    await act(async () => {
      vi.advanceTimersByTime(200);
    });

    const reconnectedWs = createdSockets[createdSockets.length - 1];
    // The reconnected URL must include since=7
    expect(reconnectedWs.url).toContain("since=7");
  });

  it("does NOT update latest for auth_expiring or auth_expired control frames", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");
    let capturedLatest: unknown = undefined;

    function TestHook() {
      const { latest } = useMetricsSocket("ws://localhost:3001/ws/metrics");
      capturedLatest = latest;
      return null;
    }

    await act(async () => {
      render(<TestHook />);
    });

    const ws = createdSockets[0];
    await act(async () => {
      ws._open();
      ws._message(JSON.stringify({ type: "snapshot", id: 1, metrics: sampleMetrics }));
    });

    const snapshotValue = capturedLatest;

    await act(async () => {
      ws._message(JSON.stringify({ type: "auth_expiring", expiresAt: 9999999 }));
      ws._message(JSON.stringify({ type: "auth_expired" }));
    });

    expect(capturedLatest).toEqual(snapshotValue);
  });
});

// ---------------------------------------------------------------------------
// #1299 — useMetricsSocket cursor-on-reconnect tests
// ---------------------------------------------------------------------------

describe("#1299 useMetricsSocket — last-seen event id cursor on reconnect", () => {
  beforeEach(() => {
    createdSockets = [];
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  it("initial connect uses no since parameter when no events seen yet", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");

    function TestHook() {
      useMetricsSocket("ws://localhost:3001/ws/metrics");
      return null;
    }

    await act(async () => {
      render(<TestHook />);
    });

    const ws = createdSockets[0];
    // No since= appended on first connect since we have no cursor
    expect(ws.url).not.toContain("since=");
  });

  it("reconnect after network-blip includes since= with last snapshot id", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");

    function TestHook() {
      useMetricsSocket("ws://localhost:3001/ws/metrics", 100);
      return null;
    }

    await act(async () => {
      render(<TestHook />);
    });

    const ws = createdSockets[0];
    await act(async () => {
      ws._open();
      // Server sends a snapshot with id=99; the client should record this
      ws._message(JSON.stringify({ type: "snapshot", id: 99, metrics: sampleMetrics }));
    });

    // Simulate a network blip: the socket closes unexpectedly
    await act(async () => {
      ws._close();
    });

    // Let the reconnect timer fire
    await act(async () => {
      vi.advanceTimersByTime(200);
    });

    const reconnectedWs = createdSockets[createdSockets.length - 1];
    expect(reconnectedWs.url).toContain("since=99");
  });

  it("cursor advances with each snapshot so repeated reconnects use the latest id", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");

    function TestHook() {
      useMetricsSocket("ws://localhost:3001/ws/metrics", 50);
      return null;
    }

    await act(async () => {
      render(<TestHook />);
    });

    // First connection: receive two snapshots
    const ws1 = createdSockets[0];
    await act(async () => {
      ws1._open();
      ws1._message(JSON.stringify({ type: "snapshot", id: 10, metrics: sampleMetrics }));
      ws1._message(JSON.stringify({ type: "snapshot", id: 20, metrics: sampleMetrics }));
      ws1._close();
    });

    await act(async () => { vi.advanceTimersByTime(100); });

    const ws2 = createdSockets[createdSockets.length - 1];
    expect(ws2.url).toContain("since=20");

    // Second connection: receive one more snapshot, then reconnect again
    await act(async () => {
      ws2._open();
      ws2._message(JSON.stringify({ type: "snapshot", id: 35, metrics: sampleMetrics }));
      ws2._close();
    });

    await act(async () => { vi.advanceTimersByTime(100); });

    const ws3 = createdSockets[createdSockets.length - 1];
    expect(ws3.url).toContain("since=35");
  });
});

// ---------------------------------------------------------------------------
// #1299 — useLoanSocket cursor-on-reconnect tests
// ---------------------------------------------------------------------------

describe("#1299 useLoanSocket — missed events replayed from cursor on reconnect", () => {
  beforeEach(() => {
    mockSocket.emit = vi.fn();
    mockSocket.disconnect = vi.fn();
    mockSocket.handlers = {};
  });

  afterEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it("initial subscribe emits no since field when no events seen", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    // Trigger connect
    await act(async () => {
      mockSocket.handlers["connect"]?.();
    });

    expect(mockSocket.emit).toHaveBeenCalledWith("subscribe", { borrower: "GABC" });
  });

  it("updates lastEventId when loan:update envelope with eventId is received", async () => {
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
      mockSocket.handlers["connect"]?.();
      // Server sends envelope shape with eventId
      mockSocket.handlers["loan:update"]?.({ eventId: 55, loan: activeLoan });
    });

    // Simulate disconnect + reconnect
    await act(async () => {
      mockSocket.handlers["disconnect"]?.();
      mockSocket.handlers["connect"]?.();
    });

    // Second subscribe call must include since=55
    const subscribeCalls = (mockSocket.emit as ReturnType<typeof vi.fn>).mock.calls.filter(
      (c) => c[0] === "subscribe"
    );
    expect(subscribeCalls.length).toBeGreaterThanOrEqual(2);
    const lastSubscribePayload = subscribeCalls[subscribeCalls.length - 1][1];
    expect(lastSubscribePayload).toEqual({ borrower: "GABC", since: 55 });
  });

  it("updates lastEventId when loan:list envelope with eventId is received", async () => {
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
      mockSocket.handlers["connect"]?.();
      // Server sends loan:list with eventId cursor (initial replay envelope)
      mockSocket.handlers["loan:list"]?.({ eventId: 77, loans: [activeLoan] });
    });

    await act(async () => {
      mockSocket.handlers["disconnect"]?.();
      mockSocket.handlers["connect"]?.();
    });

    const subscribeCalls = (mockSocket.emit as ReturnType<typeof vi.fn>).mock.calls.filter(
      (c) => c[0] === "subscribe"
    );
    const lastSubscribePayload = subscribeCalls[subscribeCalls.length - 1][1];
    expect(lastSubscribePayload).toEqual({ borrower: "GABC", since: 77 });
  });

  it("dispatches upsertLoan even when loan:update arrives as bare LoanRecord (legacy shape)", async () => {
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
      mockSocket.handlers["connect"]?.();
      // Legacy shape — no eventId wrapper
      mockSocket.handlers["loan:update"]?.(activeLoan);
    });

    expect(testStore.getState().loans.loans).toHaveLength(1);
    expect(testStore.getState().loans.loans[0].id).toBe(1);
  });

  it("reconnect after disconnect replays missed events via since cursor", async () => {
    // This test simulates the full scenario described in #1299:
    // 1. Client connects and receives an event (id=100)
    // 2. Network blip — disconnect
    // 3. Server publishes new events during the gap
    // 4. Client reconnects and subscribes with since=100
    // 5. Server replays missed events → client eventually observes them

    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    // Step 1: connected, receive event id=100
    await act(async () => {
      mockSocket.handlers["connect"]?.();
      mockSocket.handlers["loan:update"]?.({ eventId: 100, loan: activeLoan });
    });

    expect(testStore.getState().loans.loans).toHaveLength(1);

    // Step 2: disconnect (network blip)
    await act(async () => {
      mockSocket.handlers["disconnect"]?.();
    });

    // Step 3: reconnect — subscribe must carry since=100
    await act(async () => {
      mockSocket.handlers["connect"]?.();
    });

    const subscribeCalls = (mockSocket.emit as ReturnType<typeof vi.fn>).mock.calls.filter(
      (c) => c[0] === "subscribe"
    );
    const reconnectPayload = subscribeCalls[subscribeCalls.length - 1][1];
    expect(reconnectPayload).toEqual({ borrower: "GABC", since: 100 });

    // Step 5: server replays missed events (loan:list replay from the server)
    const missedLoan: LoanRecord = { ...activeLoan, id: 2, status: "Repaid" };
    await act(async () => {
      mockSocket.handlers["loan:list"]?.({ eventId: 101, loans: [activeLoan, missedLoan] });
    });

    // Client now observes both loans — the missed one is no longer dropped
    expect(testStore.getState().loans.loans).toHaveLength(2);
    expect(testStore.getState().loans.loans.some((l) => l.id === 2)).toBe(true);
  });
});
