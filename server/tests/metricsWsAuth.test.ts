/**
 * Issue #2156: metrics WebSocket server must accept the bearer token from the
 * Sec-WebSocket-Protocol header and treat the ?token= query string as deprecated.
 *
 * These exercise the real http + ws handshake against attachMetricsWsServer (the
 * wsAuthToken parsing logic itself is unit-tested in wsAuthToken.test.ts).
 */
import { describe, it, expect, vi, afterEach } from "vitest";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { WebSocket } from "ws";
import { attachMetricsWsServer } from "../src/ws/metricsWsServer.js";
import { issueToken } from "../src/auth/tokens.js";
import { logger } from "../src/logging/logger.js";

// Minimal fakes — auth tests don't care about snapshot contents, only that the
// handshake is admitted or refused.
const fakeStore = { getEventsSince: () => [] } as unknown as Parameters<
  typeof attachMetricsWsServer
>[0]["store"];
const fakeBus = {
  subscribe: () => undefined,
  unsubscribe: () => Promise.resolve(),
} as unknown as Parameters<typeof attachMetricsWsServer>[0]["bus"];

const SECRET = "x".repeat(32);

interface Outcome {
  opened: boolean;
  protocol?: string;
}

function attemptConnect(port: number, url: string, protocols?: string | string[]): Promise<Outcome> {
  return new Promise((resolve) => {
    let settled = false;
    const done = (o: Outcome): void => {
      if (settled) return;
      settled = true;
      resolve(o);
    };
    const ws = new WebSocket(url, protocols);
    ws.on("open", () => done({ opened: true, protocol: ws.protocol }));
    ws.on("error", () => done({ opened: false }));
    ws.on("close", () => done({ opened: false }));
    setTimeout(() => {
      try { ws.terminate(); } catch { /* ignore */ }
      done({ opened: false });
    }, 1_000);
  });
}

async function start(opts: { queryTokenSunsetDate?: Date }): Promise<{ server: Server; port: number }> {
  const server = createServer();
  attachMetricsWsServer({
    httpServer: server,
    bus: fakeBus,
    store: fakeStore,
    authSecret: SECRET,
    connectionQueueMax: 10,
    queryTokenSunsetDate: opts.queryTokenSunsetDate,
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", () => resolve()));
  return { server, port: (server.address() as AddressInfo).port };
}

async function stop(server: Server): Promise<void> {
  server.closeAllConnections?.();
  await new Promise<void>((resolve) => server.close(() => resolve()));
}

describe("metricsWsServer auth (#2156)", () => {
  let server: Server | undefined;

  afterEach(async () => {
    if (server) await stop(server);
    server = undefined;
  });

  it("admits a client that sends a valid token in the Sec-WebSocket-Protocol header", async () => {
    ({ server } = await start({}));
    const { token } = issueToken(SECRET, "metrics-subscriber", 300);
    const outcome = await attemptConnect(
      (server!.address() as AddressInfo).port,
      `ws://127.0.0.1:${(server!.address() as AddressInfo).port}/ws/metrics`,
      ["bearer", `bearer.${token}`]
    );
    expect(outcome.opened).toBe(true);
    // The server acks the token-less "bearer" subprotocol, never echoing the token.
    expect(outcome.protocol).toBe("bearer");
  });

  it("refuses a client that sends no token at all", async () => {
    ({ server } = await start({}));
    const port = (server!.address() as AddressInfo).port;
    const outcome = await attemptConnect(port, `ws://127.0.0.1:${port}/ws/metrics`);
    expect(outcome.opened).toBe(false);
  });

  it("refuses a client that sends an invalid token in the subprotocol header", async () => {
    ({ server } = await start({}));
    const port = (server!.address() as AddressInfo).port;
    const outcome = await attemptConnect(
      port,
      `ws://127.0.0.1:${port}/ws/metrics`,
      ["bearer", "bearer.not-a-real-token"]
    );
    expect(outcome.opened).toBe(false);
  });

  it("still admits a deprecated ?token= client (and warns) before the sunset date", async () => {
    const warnSpy = vi.spyOn(logger, "warn").mockImplementation(() => undefined);
    ({ server } = await start({
      queryTokenSunsetDate: new Date(Date.now() + 365 * 24 * 60 * 60 * 1000),
    }));
    const port = (server!.address() as AddressInfo).port;
    const { token } = issueToken(SECRET, "legacy-client", 300);
    const outcome = await attemptConnect(
      port,
      `ws://127.0.0.1:${port}/ws/metrics?token=${encodeURIComponent(token)}`
    );
    expect(outcome.opened).toBe(true);
    expect(warnSpy).toHaveBeenCalledTimes(1);
    warnSpy.mockRestore();
  });

  it("refuses a deprecated ?token= client once the sunset date has passed", async () => {
    ({ server } = await start({ queryTokenSunsetDate: new Date(0) }));
    const port = (server!.address() as AddressInfo).port;
    const { token } = issueToken(SECRET, "legacy-client", 300);
    const outcome = await attemptConnect(
      port,
      `ws://127.0.0.1:${port}/ws/metrics?token=${encodeURIComponent(token)}`
    );
    expect(outcome.opened).toBe(false);
  });
});
