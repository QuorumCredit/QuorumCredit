import type { Server as HttpServer, IncomingMessage } from "node:http";
import { WebSocketServer, WebSocket } from "ws";
import { verifyToken, isExpiringSoon } from "../auth/tokens.js";
import { ConnectionQueue } from "./connectionQueue.js";
import type { EventStore } from "../bridge/eventStore.js";
import { MetricsAggregator } from "../bridge/metricsAggregator.js";
import type { PubSubBus } from "../pubsub/PubSubBus.js";
import {
  EVENTS_CHANNEL,
  type BroadcastEvent,
  type MetricsClientFrame,
  type MetricsServerFrame,
  type ProtocolMetrics,
} from "../types.js";
import { metrics as opsMetrics } from "../http/metricsRegistry.js";

export interface MetricsWsServerOptions {
  httpServer: HttpServer;
  bus: PubSubBus;
  store: EventStore;
  authSecret: string;
  connectionQueueMax: number;
  path?: string;
  authExpiryWarningMs?: number;
}

interface ConnState {
  queue: ConnectionQueue<MetricsServerFrame>;
  token: string;
  authTimer: ReturnType<typeof setInterval>;
}

/**
 * Raw WebSocket wiring for /ws/metrics, consumed by
 * dashboard/src/useMetricsSocket.ts. Auth + resume are both carried on the query
 * string at connect time (`?token=...&since=<lastEventId>`) since a plain WebSocket
 * handshake has no room for a custom auth exchange the way socket.io's does; token
 * refresh happens via an in-band `{type:"refresh_auth", token}` client frame.
 */
export function attachMetricsWsServer(opts: MetricsWsServerOptions): WebSocketServer {
  const path = opts.path ?? "/ws/metrics";
  const wss = new WebSocketServer({ noServer: true });
  const states = new Map<WebSocket, ConnState>();
  const warningMs = opts.authExpiryWarningMs ?? 30_000;

  const busHandler = (message: string): void => {
    let parsed: BroadcastEvent;
    try {
      parsed = JSON.parse(message);
    } catch {
      return;
    }
    const frame: MetricsServerFrame = { type: "snapshot", id: parsed.eventId, metrics: parsed.metrics };

    for (const [socket, state] of states) {
      const dropped = state.queue.push(frame);
      flush(socket, state);
      if (dropped) {
        opsMetrics.incCounter("qc_broadcast_messages_dropped_total");
        send(socket, { type: "resync_required", reason: "queue_overflow", resumeFrom: parsed.eventId });
      }
    }
  };

  void opts.bus.subscribe(EVENTS_CHANNEL, busHandler);

  opts.httpServer.on("upgrade", (req: IncomingMessage, socket, head) => {
    const url = new URL(req.url ?? "", "http://internal");
    if (url.pathname !== path) return;

    const token = url.searchParams.get("token") ?? "";
    const result = verifyToken(opts.authSecret, token);
    if (!result.valid) {
      socket.write("HTTP/1.1 401 Unauthorized\r\n\r\n");
      socket.destroy();
      return;
    }

    wss.handleUpgrade(req, socket, head, (ws) => {
      const since = Number.parseInt(url.searchParams.get("since") ?? "0", 10) || 0;
      const state: ConnState = {
        queue: new ConnectionQueue(opts.connectionQueueMax),
        token,
        authTimer: setInterval(() => checkAuthExpiry(ws, states, opts.authSecret, warningMs), 5000),
      };
      states.set(ws, state);
      opsMetrics.setGauge("qc_broadcast_metrics_connections", states.size);

      // Metrics are a cumulative gauge, not a per-item list: a reconnecting client
      // needs "the current snapshot", not a replay of every intermediate one, so we
      // always compute the latest cumulative snapshot from the full event history
      // regardless of `since` (which only affects the initial loan replay).
      void since;
      const { id, metrics } = computeCurrentSnapshot(opts.store);
      send(ws, { type: "snapshot", id, metrics });

      ws.on("message", (data) => {
        let frame: MetricsClientFrame;
        try {
          frame = JSON.parse(data.toString());
        } catch {
          return;
        }
        if (frame.type === "refresh_auth" && typeof frame.token === "string") {
          const refreshed = verifyToken(opts.authSecret, frame.token);
          if (refreshed.valid) {
            state.token = frame.token;
          } else {
            send(ws, { type: "auth_expired" });
            ws.close(4001, "auth_expired");
          }
        }
      });

      ws.on("close", () => {
        clearInterval(state.authTimer);
        states.delete(ws);
        opsMetrics.setGauge("qc_broadcast_metrics_connections", states.size);
      });
    });
  });

  return wss;
}

function flush(socket: WebSocket, state: ConnState): void {
  const items = state.queue.drainAll();
  for (const item of items) send(socket, item);
}

function send(socket: WebSocket, frame: MetricsServerFrame): void {
  if (socket.readyState === WebSocket.OPEN) socket.send(JSON.stringify(frame));
}

function checkAuthExpiry(
  socket: WebSocket,
  states: Map<WebSocket, ConnState>,
  secret: string,
  warningMs: number
): void {
  const state = states.get(socket);
  if (!state) return;
  const result = verifyToken(secret, state.token);
  if (!result.valid) {
    send(socket, { type: "auth_expired" });
    socket.close(4001, "auth_expired");
    return;
  }
  if (isExpiringSoon(result.payload, warningMs)) {
    send(socket, { type: "auth_expiring", expiresAt: result.payload.exp * 1000 });
  }
}

/** Replays the full event history through a fresh aggregator to get the true
 * cumulative snapshot for a newly-connecting client. Cheap at this protocol's event
 * volume; if that stops being true, the bridge (which already maintains a live
 * aggregator) should expose a "current snapshot" query instead of every connection
 * re-deriving it independently. */
function computeCurrentSnapshot(store: EventStore): { id: number; metrics: ProtocolMetrics } {
  const aggregator = new MetricsAggregator();
  const rows = store.getEventsSince(0);
  let metrics: ProtocolMetrics = {
    tvl: 0,
    active_loans: 0,
    total_loans: 0,
    defaulted_loans: 0,
    default_rate: 0,
    total_yield_distributed: 0,
    slash_count: 0,
    fee_revenue: 0,
    top_borrowers: [],
    top_vouchers: [],
    timestamp: 0,
  };
  let id = 0;
  for (const row of rows) {
    metrics = aggregator.applyEvent(row);
    id = row.id;
  }
  return { id, metrics };
}
