import type { Server as HttpServer } from "node:http";
import { Server as SocketIOServer, type Socket } from "socket.io";
import { verifyToken, isExpiringSoon } from "../auth/tokens.js";
import { ConnectionQueue } from "./connectionQueue.js";
import { LoanProjector } from "../bridge/loanProjector.js";
import type { EventStore } from "../bridge/eventStore.js";
import type { PubSubBus } from "../pubsub/PubSubBus.js";
import { EVENTS_CHANNEL, type BroadcastEvent, type SubscribePayload } from "../types.js";
import { metrics } from "../http/metricsRegistry.js";

export interface LoanSocketServerOptions {
  httpServer: HttpServer;
  bus: PubSubBus;
  store: EventStore;
  authSecret: string;
  connectionQueueMax: number;
  /** How long before hard expiry to warn the client so it can refresh proactively. */
  authExpiryWarningMs?: number;
}

interface SocketState {
  borrower: string | null;
  queue: ConnectionQueue<{ eventId: number; loan: unknown }>;
  authTimer: ReturnType<typeof setInterval>;
}

/**
 * socket.io wiring for the /loans stream consumed by dashboard/src/useLoanSocket.ts.
 * Auth: handshake `auth.token` is verified on connect; a periodic check warns the
 * client via `auth_expired` before hard-disconnecting so it can call `auth:refresh`
 * with a freshly issued token without losing the socket.
 */
export function attachLoanSocketServer(opts: LoanSocketServerOptions): SocketIOServer {
  const io = new SocketIOServer(opts.httpServer, {
    cors: { origin: "*" },
  });

  const projector = new LoanProjector();
  const states = new Map<Socket, SocketState>();
  const warningMs = opts.authExpiryWarningMs ?? 30_000;

  io.use((socket, next) => {
    const token = socket.handshake.auth?.token;
    if (typeof token !== "string") return next(new Error("auth_required"));
    const result = verifyToken(opts.authSecret, token);
    if (!result.valid) return next(new Error(result.reason));
    next();
  });

  const busHandler = (message: string): void => {
    let parsed: BroadcastEvent;
    try {
      parsed = JSON.parse(message);
    } catch {
      return;
    }
    const loan = projector.applyEvent(parsed.event);
    if (!loan) return;

    for (const [socket, state] of states) {
      if (state.borrower !== loan.borrower) continue;
      const dropped = state.queue.push({ eventId: parsed.eventId, loan });
      flush(socket, state);
      if (dropped) {
        metrics.incCounter("qc_broadcast_messages_dropped_total");
        socket.emit("resync_required", { reason: "queue_overflow", resumeFrom: parsed.eventId });
      }
    }
  };

  void opts.bus.subscribe(EVENTS_CHANNEL, busHandler);

  io.on("connection", (socket) => {
    const state: SocketState = {
      borrower: null,
      queue: new ConnectionQueue(opts.connectionQueueMax),
      authTimer: setInterval(() => checkAuthExpiry(socket, opts.authSecret, warningMs), 5000),
    };
    states.set(socket, state);
    metrics.setGauge("qc_broadcast_loan_connections", states.size);

    socket.on("subscribe", (payload: SubscribePayload) => {
      if (!payload || typeof payload.borrower !== "string") return;
      state.borrower = payload.borrower;

      const since = typeof payload.since === "number" ? payload.since : 0;
      const rows = opts.store.getEventsSince(since).filter((e) => e.category === "loan");
      const loans = rows
        .map((e) => ({ eventId: e.id, loan: projector.applyEvent(e) }))
        .filter((x): x is { eventId: number; loan: NonNullable<ReturnType<LoanProjector["applyEvent"]>> } => x.loan !== null)
        .filter((x) => x.loan.borrower === payload.borrower);

      if (loans.length > 0) {
        socket.emit("loan:list", { eventId: loans[loans.length - 1].eventId, loans: loans.map((l) => l.loan) });
      }
    });

    socket.on("auth:refresh", (payload: { token?: string }) => {
      if (!payload || typeof payload.token !== "string") return;
      const result = verifyToken(opts.authSecret, payload.token);
      if (!result.valid) {
        socket.emit("auth_expired");
        socket.disconnect(true);
      }
    });

    socket.on("disconnect", () => {
      clearInterval(state.authTimer);
      states.delete(socket);
      metrics.setGauge("qc_broadcast_loan_connections", states.size);
    });
  });

  opts.httpServer.once("close", () => {
    void opts.bus.unsubscribe(EVENTS_CHANNEL, busHandler);
  });

  return io;
}

function flush(socket: Socket, state: SocketState): void {
  const items = state.queue.drainAll();
  for (const item of items) socket.emit("loan:update", item);
}

function checkAuthExpiry(socket: Socket, secret: string, warningMs: number): void {
  const token = socket.handshake.auth?.token;
  if (typeof token !== "string") return;
  const result = verifyToken(secret, token);
  if (!result.valid) {
    socket.emit("auth_expired");
    socket.disconnect(true);
    return;
  }
  if (isExpiringSoon(result.payload, warningMs)) {
    socket.emit("auth_expiring", { expiresAt: result.payload.exp * 1000 });
  }
}
