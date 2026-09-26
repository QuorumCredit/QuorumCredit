/**
 * Issue #1744: Request idempotency keys.
 *
 * Clients send `Idempotency-Key: <opaque key>` on mutating requests (POST/PUT/PATCH/
 * DELETE). The first request with a key runs normally and its response (status,
 * headers, body) is stored; retries with the same key replay the stored response
 * instead of repeating the operation. A retry that arrives while the original is still
 * in flight gets 409, and reusing a key with a different request payload gets 422.
 * Records expire after a TTL and are swept periodically.
 */

import { createHash } from "node:crypto";
import type { IncomingMessage, ServerResponse } from "node:http";
import { metrics } from "./metricsRegistry.js";

export const IDEMPOTENCY_HEADER = "idempotency-key";
const MAX_KEY_LENGTH = 255;
const IDEMPOTENT_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);

interface StoredResponse {
  statusCode: number;
  headers: Record<string, string | number | string[]>;
  body: Buffer;
}

interface IdempotencyRecord {
  fingerprint: string;
  state: "in_flight" | "completed";
  response?: StoredResponse;
  createdAt: number;
  expiresAt: number;
}

export interface IdempotencyStoreOptions {
  /** How long a completed record is retained. Default 24h. */
  ttlMs?: number;
  /** How long an in-flight record is held before it is considered abandoned. Default 60s. */
  inFlightTtlMs?: number;
  /** Cleanup sweep interval. Default 10 min. */
  cleanupIntervalMs?: number;
  /** Upper bound on retained records; oldest are evicted first. Default 100k. */
  maxRecords?: number;
  /** Responses with bodies larger than this are not stored. Default 1 MiB. */
  maxBodyBytes?: number;
}

export class IdempotencyStore {
  private readonly records = new Map<string, IdempotencyRecord>();
  private readonly ttlMs: number;
  private readonly inFlightTtlMs: number;
  private readonly maxRecords: number;
  readonly maxBodyBytes: number;
  private cleanupTimer?: ReturnType<typeof setInterval>;

  constructor(opts: IdempotencyStoreOptions = {}) {
    this.ttlMs = opts.ttlMs ?? 24 * 60 * 60 * 1000;
    this.inFlightTtlMs = opts.inFlightTtlMs ?? 60 * 1000;
    this.maxRecords = opts.maxRecords ?? 100_000;
    this.maxBodyBytes = opts.maxBodyBytes ?? 1024 * 1024;
    const interval = opts.cleanupIntervalMs ?? 10 * 60 * 1000;
    this.cleanupTimer = setInterval(() => this.cleanup(), interval);
    this.cleanupTimer.unref?.();
  }

  get(key: string): IdempotencyRecord | undefined {
    const record = this.records.get(key);
    if (record && record.expiresAt <= Date.now()) {
      this.records.delete(key);
      return undefined;
    }
    return record;
  }

  begin(key: string, fingerprint: string): void {
    const now = Date.now();
    this.records.set(key, {
      fingerprint,
      state: "in_flight",
      createdAt: now,
      expiresAt: now + this.inFlightTtlMs,
    });
    this.evictOverflow();
  }

  complete(key: string, response: StoredResponse): void {
    const record = this.records.get(key);
    if (!record) return;
    record.state = "completed";
    record.response = response;
    record.expiresAt = Date.now() + this.ttlMs;
  }

  /** Drop the record so the client may retry (used for 5xx and unstorable responses). */
  release(key: string): void {
    this.records.delete(key);
  }

  /** Removes expired records. Returns the number removed. */
  cleanup(now = Date.now()): number {
    let removed = 0;
    for (const [key, record] of this.records) {
      if (record.expiresAt <= now) {
        this.records.delete(key);
        removed++;
      }
    }
    if (removed > 0) metrics.incCounter("qc_idempotency_records_expired_total", removed);
    metrics.setGauge("qc_idempotency_records", this.records.size);
    return removed;
  }

  get size(): number {
    return this.records.size;
  }

  close(): void {
    if (this.cleanupTimer) clearInterval(this.cleanupTimer);
    this.records.clear();
  }

  private evictOverflow(): void {
    while (this.records.size > this.maxRecords) {
      const oldest = this.records.keys().next().value;
      if (oldest === undefined) break;
      this.records.delete(oldest);
    }
  }
}

export const defaultIdempotencyStore = new IdempotencyStore();

/**
 * Wraps a request handler with idempotency-key handling. The request body is buffered
 * to fingerprint it and then re-emitted so downstream `readJsonBody` works unchanged.
 */
export function withIdempotency(
  req: IncomingMessage,
  res: ServerResponse,
  next: () => void,
  store: IdempotencyStore = defaultIdempotencyStore
): void {
  const rawKey = req.headers[IDEMPOTENCY_HEADER];
  const key = Array.isArray(rawKey) ? rawKey[0] : rawKey;

  if (!key || !IDEMPOTENT_METHODS.has(req.method ?? "")) {
    next();
    return;
  }

  if (key.length > MAX_KEY_LENGTH || !/^[\x21-\x7e]+$/.test(key)) {
    res.writeHead(400, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        error: `Idempotency-Key must be 1-${MAX_KEY_LENGTH} printable ASCII characters`,
      })
    );
    return;
  }

  bufferBody(req, (body) => {
    const url = new URL(req.url ?? "", "http://internal");
    // Scope keys to the caller's credentials and route so different clients can't collide.
    const principal = String(req.headers["authorization"] ?? req.headers["x-api-key"] ?? "");
    const scopedKey = hash(`${principal}\n${req.method}\n${url.pathname}\n${key}`);
    const fingerprint = hash(`${url.search}\n${body.toString("utf8")}`);

    const existing = store.get(scopedKey);
    if (existing) {
      if (existing.fingerprint !== fingerprint) {
        metrics.incCounter("qc_idempotency_conflicts_total");
        res.writeHead(422, { "content-type": "application/json" });
        res.end(
          JSON.stringify({ error: "Idempotency-Key was already used with a different request payload" })
        );
        return;
      }
      if (existing.state === "in_flight" || !existing.response) {
        metrics.incCounter("qc_idempotency_in_flight_rejections_total");
        res.writeHead(409, { "content-type": "application/json", "retry-after": "1" });
        res.end(JSON.stringify({ error: "a request with this Idempotency-Key is still being processed" }));
        return;
      }
      metrics.incCounter("qc_idempotency_replays_total");
      const { statusCode, headers, body: storedBody } = existing.response;
      res.writeHead(statusCode, { ...headers, "idempotent-replayed": "true" });
      res.end(storedBody);
      return;
    }

    store.begin(scopedKey, fingerprint);
    captureResponse(res, store.maxBodyBytes, (captured) => {
      if (!captured || captured.statusCode >= 500) {
        store.release(scopedKey);
      } else {
        store.complete(scopedKey, captured);
      }
    });
    next();
  });
}

function hash(input: string): string {
  return createHash("sha256").update(input).digest("hex");
}

/** Reads the full body and replays it on `req` as `data` + `end` events for downstream readers. */
function bufferBody(req: IncomingMessage, done: (body: Buffer) => void): void {
  const chunks: Buffer[] = [];
  const onData = (chunk: Buffer): void => {
    chunks.push(chunk);
  };
  req.on("data", onData);
  req.once("end", () => {
    req.off("data", onData);
    const body = Buffer.concat(chunks);
    done(body);
    process.nextTick(() => {
      if (body.length > 0) req.emit("data", body);
      req.emit("end");
    });
  });
}

/** Intercepts writeHead/write/end to capture the response for later replay. */
function captureResponse(
  res: ServerResponse,
  maxBodyBytes: number,
  onFinish: (captured: StoredResponse | null) => void
): void {
  const chunks: Buffer[] = [];
  let size = 0;
  let overflow = false;

  const collect = (chunk: unknown, encoding?: unknown): void => {
    if (chunk == null || overflow) return;
    const buf = Buffer.isBuffer(chunk)
      ? chunk
      : Buffer.from(String(chunk), typeof encoding === "string" ? (encoding as BufferEncoding) : "utf8");
    size += buf.length;
    if (size > maxBodyBytes) {
      overflow = true;
      chunks.length = 0;
      return;
    }
    chunks.push(buf);
  };

  // Headers passed directly to writeHead() are not always reflected in getHeaders(),
  // so capture them explicitly.
  const writeHeadHeaders: StoredResponse["headers"] = {};
  const originalWriteHead = res.writeHead.bind(res) as (...args: unknown[]) => ServerResponse;
  (res as unknown as { writeHead: (...args: unknown[]) => ServerResponse }).writeHead = (
    ...args: unknown[]
  ) => {
    const headersArg = typeof args[1] === "object" && args[1] !== null ? args[1] : args[2];
    if (headersArg && typeof headersArg === "object" && !Array.isArray(headersArg)) {
      for (const [name, value] of Object.entries(headersArg as Record<string, unknown>)) {
        if (value !== undefined) writeHeadHeaders[name.toLowerCase()] = value as string | number | string[];
      }
    }
    return originalWriteHead(...args);
  };

  const originalWrite = res.write.bind(res) as (...args: unknown[]) => boolean;
  const originalEnd = res.end.bind(res) as (...args: unknown[]) => ServerResponse;

  (res as unknown as { write: (...args: unknown[]) => boolean }).write = (...args: unknown[]) => {
    if (typeof args[0] !== "function") collect(args[0], args[1]);
    return originalWrite(...args);
  };
  (res as unknown as { end: (...args: unknown[]) => ServerResponse }).end = (...args: unknown[]) => {
    if (typeof args[0] !== "function") collect(args[0], args[1]);
    return originalEnd(...args);
  };

  res.once("finish", () => {
    if (overflow) {
      onFinish(null);
      return;
    }
    const headers: StoredResponse["headers"] = { ...writeHeadHeaders };
    for (const [name, value] of Object.entries(res.getHeaders())) {
      if (value !== undefined) headers[name] = value as string | number | string[];
    }
    onFinish({ statusCode: res.statusCode, headers, body: Buffer.concat(chunks) });
  });
  res.once("close", () => {
    if (!res.writableFinished) onFinish(null);
  });
}
