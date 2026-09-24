import type { Server as HttpServer } from "node:http";
import { Server as SocketIOServer, type Socket } from "socket.io";
import { verifyToken, isExpiringSoon } from "../auth/tokens.js";
import { ConnectionQueue } from "./connectionQueue.js";
import { WsRateLimiter } from "./wsRateLimiter.js";
import { buildWsAggregateRateLimiter } from "./wsAggregateRateLimiter.js";
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
  /**
   * Sliding-window rate-limit for inbound WS messages (subscribe / unsubscribe /
   * auth:refresh). Defaults tuned for a well-behaved dashboard client; override in
   * config for stricter or looser environments.
   */
  rateLimitWindowMs?: number;
  /** Max inbound messages accepted per window per connection. Default: 30. */
  rateLimitMaxMessages?: number;
  /**
   * Consecutive throttle violations before the connection is forcibly closed with
   * code 4029 ("Too Many Requests"). Default: 3.
   */
  rateLimitMaxViolations?: number;
  /**
   * Application-level ping/pong heartbeat interval (ms). The server sends a
   * "ping" event at this cadence and expects a "pong" event back. Default: 30 000.
   */
  heartbeatIntervalMs?: number;
  /**
   * How long (ms) after the last pong (or initial connect) before the server
   * treats the connection as a half-open/idle zombie and tears it down.
   * Default: 60 000.
   */
  idleTimeoutMs?: number;
  /**
   * Redis URL for the aggregate cross-instance rate limiter. When set, inbound
   * messages are counted against a shared per-key budget (IP or authenticated
   * subject) across all replicas, preventing a multi-connection client from
   * multiplying its effective rate limit. When undefined, an in-process limiter
   * is used instead (NOT safe for multi-replica production).
   */
  redisUrl?: string;
}

interface SocketState {
  borrower: string | null;
  queue: ConnectionQueue<{ eventId: number; loan: unknown }>;
  authTimer: ReturnType<typeof setInterval>;
  /** Per-connection sliding-window rate limiter for inbound messages. */
  rateLimiter: WsRateLimiter;
  /** Aggregate cross-instance rate limiter keyed by IP/subject. */
  aggregateRateLimiter: ReturnType<typeof buildWsAggregateRateLimiter>;
  /** Timer that fires pings at heartbeatIntervalMs cadence. */
  heartbeatTimer: ReturnType<typeof setInterval>;
  /** Timer that fires idleTimeoutMs after the last pong (or connect). */
  idleTimer: ReturnType<typeof setTimeout>;
}

/** Close code sent when a connection is killed for exceeding the rate limit. */
export const WS_CLOSE_TOO_MANY_REQUESTS = 4029;
/** Close code sent when a connection is killed for idle/heartbeat timeout. */
export const WS_CLOSE_IDLE_TIMEOUT = 4008;

/**
 * socket.io wiring for the /loans stream consumed by dashboard/src/useLoanSocket.ts.
 *
 * Auth: handshake `auth.token` is verified on connect; a periodic check warns the
 * client via `auth_expired` before hard-disconnecting so it can call `auth:refresh`
 * with a freshly issued token without losing the socket.
 *
 * Rate limiting: every inbound event (subscribe / unsubscribe / auth:refresh) is
 * passed through a per-connection sliding-window limiter. Connections that exceed the
 * limit are warned once ("rate_limited") then disconnected with close-code 4029 if
 * the excess continues.
 *
 * Heartbeat / idle timeout: the server sends a "ping" event every
 * `heartbeatIntervalMs` and expects a "pong" event back within `idleTimeoutMs`. If
 * no pong arrives in time (half-open / NAT-dead connection), the socket is
 * disconnected, its ConnectionQueue and subscription state are released, and the
 * qc_ws_idle_closed_total counter is incremented.
 */
export function attachLoanSocketServer(opts: LoanSocketServerOptions): SocketIOServer {
  const io = new SocketIOServer(opts.httpServer, {
    cors: { origin: "*" },
  });

  const projector = new LoanProjector();
  const states = new Map<Socket, SocketState>();
  const warningMs = opts.authExpiryWarningMs ?? 30_000;
  const heartbeatIntervalMs = opts.heartbeatIntervalMs ?? 30_000;
  const idleTimeoutMs = opts.idleTimeoutMs ?? 60_000;

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
      const dropped = state.queue.push(
        { eventId: parsed.eventId, loan },
        () => {
          metrics.incLabeledCounter("qc_ws_queue_drops_total", "type", "loan");
        }
      );
      flush(socket, state);
      if (dropped) {
        metrics.incCounter("qc_broadcast_messages_dropped_total");
        socket.emit("resync_required", { reason: "queue_overflow", resumeFrom: parsed.eventId });
      }
    }
  };

  void opts.bus.subscribe(EVENTS_CHANNEL, busHandler);

  io.on("connection", (socket) => {
    // ── heartbeat / idle-timeout setup ──────────────────────────────────────
    // scheduleIdleTimer returns a NodeJS.Timeout that disconnects the socket if it
    // fires. resetIdle cancels and reschedules it — call it whenever a pong arrives.
    const scheduleIdleTimer = (): ReturnType<typeof setTimeout> =>
      setTimeout(() => {
        const s = states.get(socket);
        if (!s) return;
        clearInterval(s.heartbeatTimer);
        clearInterval(s.authTimer);
        clearTimeout(s.idleTimer);
        states.delete(socket);
        metrics.setGauge("qc_broadcast_loan_connections", states.size);
        metrics.incCounter("qc_ws_idle_closed_total");
        socket.disconnect(true);
      }, idleTimeoutMs);

    const heartbeatTimer = setInterval(() => {
      socket.emit("ping");
    }, heartbeatIntervalMs);

    const state: SocketState = {
      borrower: null,
      queue: new ConnectionQueue(opts.connectionQueueMax),
      authTimer: setInterval(() => checkAuthExpiry(socket, opts.authSecret, warningMs), 5000),
      rateLimiter: new WsRateLimiter({
        windowMs: opts.rateLimitWindowMs,
        maxMessages: opts.rateLimitMaxMessages,
        maxViolations: opts.rateLimitMaxViolations,
      }),
      aggregateRateLimiter: buildWsAggregateRateLimiter(opts.redisUrl),
      heartbeatTimer,
      idleTimer: scheduleIdleTimer(),
    };
    states.set(socket, state);
    metrics.setGauge("qc_broadcast_loan_connections", states.size);

    // ── pong resets the idle timer ───────────────────────────────────────────
    socket.on("pong", () => {
      clearTimeout(state.idleTimer);
      state.idleTimer = scheduleIdleTimer();
    });

    // ── rate-limited event handlers ──────────────────────────────────────────

    socket.on("subscribe", (payload: SubscribePayload) => {
      const aggregateKey = (socket.handshake.address ?? "unknown") + "|" + (socket.handshake.auth?.token ?? "anon");
      const aggregateBlocked = await state.aggregateRateLimiter.isBlocked(aggregateKey);
      if (aggregateBlocked) {
        metrics.incCounter("qc_ws_rate_limited_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        return;
      }

      const decision = state.rateLimiter.check();
      if (decision === "throttled") {
        metrics.incCounter("qc_ws_rate_limited_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        return;
      }
      if (decision === "disconnect") {
        metrics.incCounter("qc_ws_rate_limited_total");
        metrics.incCounter("qc_ws_force_disconnected_rate_limit_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        socket.disconnect(true);
        return;
      }

      await state.aggregateRateLimiter.recordHit(aggregateKey);

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

    socket.on("unsubscribe", () => {
      const aggregateKey = (socket.handshake.address ?? "unknown") + "|" + (socket.handshake.auth?.token ?? "anon");
      if (await state.aggregateRateLimiter.isBlocked(aggregateKey)) {
        metrics.incCounter("qc_ws_rate_limited_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        return;
      }

      const decision = state.rateLimiter.check();
      if (decision === "throttled") {
        metrics.incCounter("qc_ws_rate_limited_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        return;
      }
      if (decision === "disconnect") {
        metrics.incCounter("qc_ws_rate_limited_total");
        metrics.incCounter("qc_ws_force_disconnected_rate_limit_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        socket.disconnect(true);
        return;
      }

      await state.aggregateRateLimiter.recordHit(aggregateKey);
      state.borrower = null;
    });

    socket.on("auth:refresh", (payload: { token?: string }) => {
      const aggregateKey = (socket.handshake.address ?? "unknown") + "|" + (payload.token ?? socket.handshake.auth?.token ?? "anon");
      if (await state.aggregateRateLimiter.isBlocked(aggregateKey)) {
        metrics.incCounter("qc_ws_rate_limited_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        return;
      }

      const decision = state.rateLimiter.check();
      if (decision === "throttled") {
        metrics.incCounter("qc_ws_rate_limited_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        return;
      }
      if (decision === "disconnect") {
        metrics.incCounter("qc_ws_rate_limited_total");
        metrics.incCounter("qc_ws_force_disconnected_rate_limit_total");
        socket.emit("rate_limited", { retryAfterMs: opts.rateLimitWindowMs ?? 1_000 });
        socket.disconnect(true);
        return;
      }

      await state.aggregateRateLimiter.recordHit(aggregateKey);

      if (!payload || typeof payload.token !== "string") return;
      const result = verifyToken(opts.authSecret, payload.token);
      if (!result.valid) {
        socket.emit("auth_expired");
        socket.disconnect(true);
      }
    });

    socket.on("disconnect", () => {
      clearInterval(state.heartbeatTimer);
      clearTimeout(state.idleTimer);
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
