/**
 * Issue #1743: Real-time credential status over WebSocket at /ws/credentials/status.
 *
 * Connect with `?token=<jwt>`. Clients then manage subscriptions in-band:
 *   {type:"subscribe", credentialIds?: string[], holderId?: string}
 *   {type:"unsubscribe", credentialIds?: string[], holderId?: string}
 *
 * The server sends one full `snapshot` per newly subscribed credential and afterwards
 * only `delta` frames carrying the fields that changed since the last frame that
 * connection received for that credential (delta encoding). Connections are admitted
 * through a pool that caps both total connections and connections per client key.
 */

import type { Server as HttpServer, IncomingMessage } from "node:http";
import { WebSocketServer, WebSocket } from "ws";
import { verifyToken } from "../auth/tokens.js";
import {
  credentialStore,
  type CredentialStatusChange,
} from "../credentials/credentialStore.js";
import { metrics as opsMetrics } from "../http/metricsRegistry.js";

export interface CredentialStatusWsOptions {
  httpServer: HttpServer;
  authSecret: string;
  path?: string;
  /** Max concurrent connections for this instance. Default 1000. */
  maxConnections?: number;
  /** Max concurrent connections per client key (token subject or IP). Default 5. */
  maxConnectionsPerClient?: number;
  /** Max credential subscriptions per connection. Default 500. */
  maxSubscriptionsPerConnection?: number;
  heartbeatIntervalMs?: number;
}

type StatusFields = {
  status: string;
  verificationStatus?: string;
  holderId: string;
  changedAt: number;
};

type ClientFrame =
  | { type: "subscribe"; credentialIds?: string[]; holderId?: string }
  | { type: "unsubscribe"; credentialIds?: string[]; holderId?: string }
  | { type: "ping" };

type ServerFrame =
  | { type: "snapshot"; credentialId: string; seq: number; state: StatusFields | null }
  | { type: "delta"; credentialId: string; seq: number; changes: Partial<StatusFields> }
  | { type: "subscribed"; credentialIds: string[]; holderIds: string[] }
  | { type: "error"; error: string }
  | { type: "pong" };

interface Subscriber {
  socket: WebSocket;
  clientKey: string;
  credentialIds: Set<string>;
  holderIds: Set<string>;
  /** Last state sent per credential — the base for delta encoding. */
  lastSent: Map<string, StatusFields>;
  seq: number;
  alive: boolean;
}

/**
 * Tracks admitted connections and enforces global and per-client limits, and keeps a
 * reverse index credential/holder -> subscribers so a status change is fanned out
 * only to interested connections rather than every open socket.
 */
export class CredentialStatusConnectionPool {
  private readonly subscribers = new Set<Subscriber>();
  private readonly perClient = new Map<string, number>();
  private readonly byCredential = new Map<string, Set<Subscriber>>();
  private readonly byHolder = new Map<string, Set<Subscriber>>();

  constructor(
    private readonly maxConnections: number,
    private readonly maxPerClient: number
  ) {}

  canAdmit(clientKey: string): { ok: true } | { ok: false; reason: string } {
    if (this.subscribers.size >= this.maxConnections) {
      return { ok: false, reason: "connection pool exhausted" };
    }
    if ((this.perClient.get(clientKey) ?? 0) >= this.maxPerClient) {
      return { ok: false, reason: "too many connections for client" };
    }
    return { ok: true };
  }

  add(sub: Subscriber): void {
    this.subscribers.add(sub);
    this.perClient.set(sub.clientKey, (this.perClient.get(sub.clientKey) ?? 0) + 1);
    this.reportGauge();
  }

  remove(sub: Subscriber): void {
    if (!this.subscribers.delete(sub)) return;
    const n = (this.perClient.get(sub.clientKey) ?? 1) - 1;
    if (n <= 0) this.perClient.delete(sub.clientKey);
    else this.perClient.set(sub.clientKey, n);
    for (const id of sub.credentialIds) removeFromIndex(this.byCredential, id, sub);
    for (const id of sub.holderIds) removeFromIndex(this.byHolder, id, sub);
    this.reportGauge();
  }

  indexCredential(sub: Subscriber, credentialId: string, add: boolean): void {
    if (add) addToIndex(this.byCredential, credentialId, sub);
    else removeFromIndex(this.byCredential, credentialId, sub);
  }

  indexHolder(sub: Subscriber, holderId: string, add: boolean): void {
    if (add) addToIndex(this.byHolder, holderId, sub);
    else removeFromIndex(this.byHolder, holderId, sub);
  }

  interestedIn(change: CredentialStatusChange): Set<Subscriber> {
    const out = new Set<Subscriber>();
    for (const s of this.byCredential.get(change.credentialId) ?? []) out.add(s);
    for (const s of this.byHolder.get(change.holderId) ?? []) out.add(s);
    return out;
  }

  all(): Iterable<Subscriber> {
    return this.subscribers;
  }

  get size(): number {
    return this.subscribers.size;
  }

  private reportGauge(): void {
    opsMetrics.setGauge("qc_credential_status_ws_connections", this.subscribers.size);
  }
}

function addToIndex(index: Map<string, Set<Subscriber>>, key: string, sub: Subscriber): void {
  let set = index.get(key);
  if (!set) {
    set = new Set();
    index.set(key, set);
  }
  set.add(sub);
}

function removeFromIndex(index: Map<string, Set<Subscriber>>, key: string, sub: Subscriber): void {
  const set = index.get(key);
  if (!set) return;
  set.delete(sub);
  if (set.size === 0) index.delete(key);
}

/** Returns only the fields of `next` that differ from `prev`, or null if nothing changed. */
export function diffStatus(
  prev: StatusFields | undefined,
  next: StatusFields
): Partial<StatusFields> | null {
  if (!prev) return { ...next };
  const changes: Partial<StatusFields> = {};
  let changed = false;
  for (const key of Object.keys(next) as (keyof StatusFields)[]) {
    if (key === "changedAt") continue;
    if (prev[key] !== next[key]) {
      (changes as Record<string, unknown>)[key] = next[key];
      changed = true;
    }
  }
  if (!changed) return null;
  changes.changedAt = next.changedAt;
  return changes;
}

function currentState(credentialId: string): StatusFields | null {
  const credential = credentialStore.getCredential(credentialId);
  if (!credential) return null;
  return {
    status: credential.status,
    verificationStatus: credentialStore.getVerification(credentialId)?.verificationStatus,
    holderId: credential.holderId,
    changedAt: Math.floor(Date.now() / 1000),
  };
}

export function attachCredentialStatusWsServer(opts: CredentialStatusWsOptions): WebSocketServer {
  const path = opts.path ?? "/ws/credentials/status";
  const maxSubs = opts.maxSubscriptionsPerConnection ?? 500;
  const wss = new WebSocketServer({ noServer: true, perMessageDeflate: true });
  const pool = new CredentialStatusConnectionPool(
    opts.maxConnections ?? 1000,
    opts.maxConnectionsPerClient ?? 5
  );

  const send = (sub: Subscriber, frame: ServerFrame): void => {
    if (sub.socket.readyState === WebSocket.OPEN) sub.socket.send(JSON.stringify(frame));
  };

  const sendState = (sub: Subscriber, credentialId: string, state: StatusFields | null): void => {
    const prev = sub.lastSent.get(credentialId);
    if (!state) {
      sub.lastSent.delete(credentialId);
      send(sub, { type: "snapshot", credentialId, seq: ++sub.seq, state: null });
      return;
    }
    if (!prev) {
      sub.lastSent.set(credentialId, state);
      send(sub, { type: "snapshot", credentialId, seq: ++sub.seq, state });
      return;
    }
    const changes = diffStatus(prev, state);
    if (!changes) return;
    sub.lastSent.set(credentialId, state);
    send(sub, { type: "delta", credentialId, seq: ++sub.seq, changes });
    opsMetrics.incCounter("qc_credential_status_ws_deltas_total");
  };

  const unsubscribeStatus = credentialStore.onStatusChange((change) => {
    const next: StatusFields = {
      status: change.status,
      verificationStatus: change.verificationStatus,
      holderId: change.holderId,
      changedAt: change.changedAt,
    };
    for (const sub of pool.interestedIn(change)) sendState(sub, change.credentialId, next);
  });

  const heartbeat = setInterval(() => {
    for (const sub of pool.all()) {
      if (!sub.alive) {
        sub.socket.terminate();
        continue;
      }
      sub.alive = false;
      if (sub.socket.readyState === WebSocket.OPEN) sub.socket.ping();
    }
  }, opts.heartbeatIntervalMs ?? 30_000);
  heartbeat.unref?.();

  wss.on("close", () => {
    clearInterval(heartbeat);
    unsubscribeStatus();
  });

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

    const clientKey = result.payload.sub ?? req.socket.remoteAddress ?? "unknown";
    const admission = pool.canAdmit(clientKey);
    if (!admission.ok) {
      opsMetrics.incCounter("qc_credential_status_ws_rejected_total");
      socket.write(`HTTP/1.1 503 Service Unavailable\r\nRetry-After: 5\r\n\r\n`);
      socket.destroy();
      return;
    }

    wss.handleUpgrade(req, socket, head, (ws) => {
      const sub: Subscriber = {
        socket: ws,
        clientKey,
        credentialIds: new Set(),
        holderIds: new Set(),
        lastSent: new Map(),
        seq: 0,
        alive: true,
      };
      pool.add(sub);

      ws.on("pong", () => {
        sub.alive = true;
      });

      ws.on("message", (data) => {
        let frame: ClientFrame;
        try {
          frame = JSON.parse(data.toString());
        } catch {
          send(sub, { type: "error", error: "invalid JSON" });
          return;
        }

        if (frame.type === "ping") {
          send(sub, { type: "pong" });
          return;
        }

        if (frame.type !== "subscribe" && frame.type !== "unsubscribe") {
          send(sub, { type: "error", error: "unknown frame type" });
          return;
        }

        const adding = frame.type === "subscribe";
        const credentialIds = Array.isArray(frame.credentialIds)
          ? frame.credentialIds.filter((id): id is string => typeof id === "string")
          : [];

        if (adding && sub.credentialIds.size + credentialIds.length > maxSubs) {
          send(sub, { type: "error", error: `subscription limit of ${maxSubs} exceeded` });
          return;
        }

        for (const id of credentialIds) {
          if (adding) {
            if (sub.credentialIds.has(id)) continue;
            sub.credentialIds.add(id);
            pool.indexCredential(sub, id, true);
            sendState(sub, id, currentState(id));
          } else {
            sub.credentialIds.delete(id);
            sub.lastSent.delete(id);
            pool.indexCredential(sub, id, false);
          }
        }

        if (typeof frame.holderId === "string" && frame.holderId) {
          const holderId = frame.holderId;
          if (adding && !sub.holderIds.has(holderId)) {
            sub.holderIds.add(holderId);
            pool.indexHolder(sub, holderId, true);
            for (const c of credentialStore.getCredentialsForHolder(holderId)) {
              sendState(sub, c.id, currentState(c.id));
            }
          } else if (!adding) {
            sub.holderIds.delete(holderId);
            pool.indexHolder(sub, holderId, false);
          }
        }

        send(sub, {
          type: "subscribed",
          credentialIds: Array.from(sub.credentialIds),
          holderIds: Array.from(sub.holderIds),
        });
      });

      ws.on("close", () => pool.remove(sub));
      ws.on("error", () => pool.remove(sub));
    });
  });

  return wss;
}
