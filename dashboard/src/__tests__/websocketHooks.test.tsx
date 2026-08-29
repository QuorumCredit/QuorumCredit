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

// ---------------------------------------------------------------------------
// #1511 — useLoanSocket connect_error: auth vs transient distinction
// ---------------------------------------------------------------------------

describe("#1511 useLoanSocket — connect_error auth vs transient", () => {
  beforeEach(() => {
    mockSocket.emit = vi.fn();
    mockSocket.disconnect = vi.fn();
    mockSocket.handlers = {};
    // Default: socket is active (socket.io will retry) — transient mode
    (mockSocket as unknown as { active: boolean }).active = true;
  });

  afterEach(() => {
    vi.clearAllMocks();
    vi.resetModules();
  });

  it("dispatches socketError with kind=transient on a network-level connect_error", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC", apiKey: "good-key" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    // socket.active=true means socket.io is retrying → transient error
    (mockSocket as unknown as { active: boolean }).active = true;
    const networkError = new Error("websocket error");

    await act(async () => {
      mockSocket.handlers["connect_error"]?.(networkError);
    });

    const state = testStore.getState().loans;
    expect(state.socketError).not.toBeNull();
    expect(state.socketError?.kind).toBe("transient");
    expect(state.socketError?.message).toBe("websocket error");
    // connected should remain false (never connected)
    expect(state.connected).toBe(false);
  });

  it("dispatches socketError with kind=auth when socket.active is false (server rejection)", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC", apiKey: "bad-key" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    // socket.active=false signals that socket.io will NOT retry → auth error
    (mockSocket as unknown as { active: boolean }).active = false;
    const authError = new Error("invalid credentials");

    await act(async () => {
      mockSocket.handlers["connect_error"]?.(authError);
    });

    const state = testStore.getState().loans;
    expect(state.socketError).not.toBeNull();
    expect(state.socketError?.kind).toBe("auth");
    expect(state.socketError?.message).toBe("invalid credentials");
  });

  it("dispatches socketError with kind=auth when err.data.type is AuthError", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC", apiKey: "bad-key" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    // socket.active=true but err.data carries an auth error signal
    (mockSocket as unknown as { active: boolean }).active = true;
    const authError = Object.assign(new Error("not authorized"), {
      data: { type: "AuthError", message: "API key not recognized" },
    });

    await act(async () => {
      mockSocket.handlers["connect_error"]?.(authError);
    });

    const state = testStore.getState().loans;
    expect(state.socketError?.kind).toBe("auth");
  });

  it("dispatches socketError with kind=auth when err.data.message contains 'unauthorized'", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC", apiKey: "expired-key" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    (mockSocket as unknown as { active: boolean }).active = true;
    const authError = Object.assign(new Error("connection refused"), {
      data: { message: "Unauthorized: token expired" },
    });

    await act(async () => {
      mockSocket.handlers["connect_error"]?.(authError);
    });

    expect(testStore.getState().loans.socketError?.kind).toBe("auth");
  });

  it("dispatches socketError with kind=auth when err.message contains 'unauthorized'", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC", apiKey: "bad-key" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    (mockSocket as unknown as { active: boolean }).active = true;
    const authError = new Error("Unauthorized");

    await act(async () => {
      mockSocket.handlers["connect_error"]?.(authError);
    });

    expect(testStore.getState().loans.socketError?.kind).toBe("auth");
  });

  it("clears socketError when socket successfully connects after a transient failure", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC", apiKey: "good-key" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    // First: transient connect_error
    (mockSocket as unknown as { active: boolean }).active = true;
    await act(async () => {
      mockSocket.handlers["connect_error"]?.(new Error("ECONNREFUSED"));
    });

    expect(testStore.getState().loans.socketError?.kind).toBe("transient");

    // Then: successful connect clears the error
    await act(async () => {
      mockSocket.handlers["connect"]?.();
    });

    expect(testStore.getState().loans.socketError).toBeNull();
    expect(testStore.getState().loans.connected).toBe(true);
  });

  it("socketError is initially null", async () => {
    const { useLoanSocket } = await import("../useLoanSocket");
    const testStore = makeStore();

    function TestHook() {
      useLoanSocket({ url: "http://localhost:3000", borrower: "GABC" });
      return null;
    }

    await act(async () => {
      render(<Provider store={testStore}><TestHook /></Provider>);
    });

    expect(testStore.getState().loans.socketError).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// #1510 — useMetricsSocket exponential backoff and give-up state
// ---------------------------------------------------------------------------

describe("#1510 useMetricsSocket — exponential backoff and give-up", () => {
  beforeEach(() => {
    createdSockets = [];
    vi.useFakeTimers();
    vi.stubGlobal("WebSocket", FakeWebSocket);
    // Pin Math.random to 1.0 so delay = ceiling (worst-case / deterministic)
    vi.spyOn(Math, "random").mockReturnValue(1.0);
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    vi.resetModules();
  });

  it("does NOT give up before maxAttempts are exhausted", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");
    let capturedGaveUp = false;

    function TestHook() {
      const { gaveUp } = useMetricsSocket("ws://localhost/metrics", 100, 10_000, 3);
      capturedGaveUp = gaveUp;
      return null;
    }

    await act(async () => { render(<TestHook />); });

    // Close attempt 0 — one of 3 allowed failures
    const ws1 = createdSockets[0];
    await act(async () => { ws1._close(); });
    await act(async () => { vi.advanceTimersByTime(10_000); });
    // Only 1 of 3 maxAttempts used; should not have given up
    expect(capturedGaveUp).toBe(false);
  });

  it("sets gaveUp=true after maxAttempts consecutive close events", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");
    let capturedGaveUp = false;

    function TestHook() {
      const { gaveUp } = useMetricsSocket("ws://localhost/metrics", 100, 10_000, 3);
      capturedGaveUp = gaveUp;
      return null;
    }

    await act(async () => { render(<TestHook />); });

    // maxAttempts=3 means 3 retries scheduled after 3 closes.
    // The 4th close (after the 3rd retry fires) is what pushes attempt=3 >= 3.
    // Total closes needed: maxAttempts + 1 = 4.
    for (let i = 0; i < 4; i++) {
      const ws = createdSockets[createdSockets.length - 1];
      await act(async () => { ws._close(); });
      await act(async () => { vi.advanceTimersByTime(10_000); });
    }

    expect(capturedGaveUp).toBe(true);
  });

  it("stops creating new WebSocket connections after giving up", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");

    function TestHook() {
      useMetricsSocket("ws://localhost/metrics", 50, 5_000, 2);
      return null;
    }

    await act(async () => { render(<TestHook />); });

    // Exhaust maxAttempts=2 → need 3 closes (2+1)
    for (let i = 0; i < 3; i++) {
      const ws = createdSockets[createdSockets.length - 1];
      await act(async () => { ws._close(); });
      await act(async () => { vi.advanceTimersByTime(10_000); });
    }

    const socketCountAfterGiveUp = createdSockets.length;

    // Advance more time — no new sockets should be created
    await act(async () => { vi.advanceTimersByTime(60_000); });

    expect(createdSockets.length).toBe(socketCountAfterGiveUp);
  });

  it("resets gaveUp=false and reconnects when resetKey changes", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");
    let capturedGaveUp = false;
    let setKey!: (k: number) => void;

    function TestHook() {
      const [key, setK] = React.useState(0);
      setKey = setK;
      const { gaveUp } = useMetricsSocket("ws://localhost/metrics", 50, 5_000, 2, key);
      capturedGaveUp = gaveUp;
      return null;
    }

    await act(async () => { render(<TestHook />); });

    // Exhaust maxAttempts=2 → need 3 closes (2+1)
    for (let i = 0; i < 3; i++) {
      const ws = createdSockets[createdSockets.length - 1];
      await act(async () => { ws._close(); });
      await act(async () => { vi.advanceTimersByTime(10_000); });
    }

    expect(capturedGaveUp).toBe(true);
    const socketCountBeforeRetry = createdSockets.length;

    // Trigger retry via resetKey increment
    await act(async () => { setKey(1); });

    expect(capturedGaveUp).toBe(false);
    // A new socket should have been opened
    expect(createdSockets.length).toBeGreaterThan(socketCountBeforeRetry);
  });

  it("uses exponential backoff: delay for attempt N is capped at maxDelayMs", async () => {
    // With Math.random() === 1.0:  delay = min(base * 2^attempt, maxDelay)
    // base=100, maxDelay=200  →  attempt 0: 100ms, attempt 1: 200ms (capped)
    const { useMetricsSocket } = await import("../useMetricsSocket");

    function TestHook() {
      useMetricsSocket("ws://localhost/metrics", 100, 200, 5);
      return null;
    }

    await act(async () => { render(<TestHook />); });

    // Attempt 0: initial connect, close immediately
    const ws0 = createdSockets[0];
    await act(async () => { ws0._close(); });

    // After 99ms: reconnect timer has NOT fired yet (delay = 100ms)
    await act(async () => { vi.advanceTimersByTime(99); });
    expect(createdSockets).toHaveLength(1);

    // After 1 more ms (total 100ms): reconnect fires
    await act(async () => { vi.advanceTimersByTime(1); });
    expect(createdSockets.length).toBeGreaterThanOrEqual(2);

    // Attempt 1: close immediately; delay = min(100*2^1, 200) = 200ms
    const ws1 = createdSockets[createdSockets.length - 1];
    await act(async () => { ws1._close(); });
    const countBefore = createdSockets.length;

    await act(async () => { vi.advanceTimersByTime(199); });
    expect(createdSockets.length).toBe(countBefore); // not yet

    await act(async () => { vi.advanceTimersByTime(1); });
    expect(createdSockets.length).toBeGreaterThan(countBefore); // fires at 200ms
  });

  it("resets attempt counter after a successful connection", async () => {
    const { useMetricsSocket } = await import("../useMetricsSocket");
    let capturedGaveUp = false;

    function TestHook() {
      const { gaveUp } = useMetricsSocket("ws://localhost/metrics", 50, 5_000, 2);
      capturedGaveUp = gaveUp;
      return null;
    }

    await act(async () => { render(<TestHook />); });

    // One failed attempt
    const ws1 = createdSockets[0];
    await act(async () => { ws1._close(); });
    await act(async () => { vi.advanceTimersByTime(10_000); });

    // Second attempt: this one opens successfully → resets the counter
    const ws2 = createdSockets[createdSockets.length - 1];
    await act(async () => { ws2._open(); });

    // Now close it — counter was reset on open, so we have 2 more retries
    await act(async () => { ws2._close(); });
    await act(async () => { vi.advanceTimersByTime(10_000); });

    // Should NOT have given up (1 of 2 attempts used after counter reset)
    expect(capturedGaveUp).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// #1510 — EmptyState give-up variant rendering
// These tests do NOT use vi.resetModules() so @testing-library/react screen
// remains the module-level import throughout.
// ---------------------------------------------------------------------------

import EmptyStateComponent from "../EmptyState";

describe("#1510 EmptyState — give-up variant", () => {
  it("renders give-up state with role=alert", () => {
    const { getByTestId } = render(<EmptyStateComponent variant="give-up" />);
    const el = getByTestId("empty-state-give-up");
    expect(el).toBeInTheDocument();
    expect(el).toHaveAttribute("role", "alert");
  });

  it("renders the give-up title text", () => {
    const { getByText } = render(<EmptyStateComponent variant="give-up" />);
    expect(getByText(/Unable to connect/i)).toBeInTheDocument();
  });

  it("renders the subtitle message", () => {
    const { getByText } = render(<EmptyStateComponent variant="give-up" />);
    expect(getByText(/could not be reached/i)).toBeInTheDocument();
  });

  it("renders a retry button when onRetry is provided", () => {
    const onRetry = vi.fn();
    const { getByRole } = render(
      <EmptyStateComponent variant="give-up" onRetry={onRetry} />,
    );
    const btn = getByRole("button", { name: /try again/i });
    expect(btn).toBeInTheDocument();
    btn.click();
    expect(onRetry).toHaveBeenCalledOnce();
  });

  it("does NOT render a retry button when onRetry is omitted", () => {
    const { queryByRole } = render(<EmptyStateComponent variant="give-up" />);
    expect(queryByRole("button", { name: /try again/i })).not.toBeInTheDocument();
  });
});
