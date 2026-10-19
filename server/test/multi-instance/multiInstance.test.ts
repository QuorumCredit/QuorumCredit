import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { fork, type ChildProcess } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { startRelayServer, type RelayServerHandle } from "../../src/pubsub/relayServer.js";

/**
 * Proves the core claim in issue #1142: an event whose write is only ever observed
 * by one server instance is still delivered to a client connected to a *different*
 * instance, because both instances share a common pub/sub backbone — here, two
 * separate OS processes (`instanceProcess.ts`, forked below). When REDIS_URL is set (as in CI, see
 * .github/workflows/server-multi-instance.yml) both children use RedisBus against
 * that real Redis; otherwise they fall back to RelayBus against a test-only relay
 * server standing in for Redis (see relayServer.ts), so the suite still runs
 * locally without a redis-server binary.
 *
 * Kept out of `npm test` (separate vitest.multi-instance.config.ts / npm script) so
 * the default CI gate stays fast and hermetic; this one spawns real processes and
 * is run explicitly via `npm run test:multi-instance`.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const instanceScript = join(__dirname, "instanceProcess.ts");

const redisUrl = process.env.REDIS_URL;
// Unique per run so lock / rate-limit keys never collide with a previous run's
// leftovers in a shared Redis.
const runId = `${process.pid}-${Date.now()}`;

let relay: RelayServerHandle | undefined;
let instanceA: ChildProcess;
let instanceB: ChildProcess;
let reqCounter = 0;

function waitForMessage(child: ChildProcess, predicate: (msg: any) => boolean, timeoutMs = 5000): Promise<any> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error("timed out waiting for child message")), timeoutMs);
    const handler = (msg: any): void => {
      if (predicate(msg)) {
        clearTimeout(timer);
        child.off("message", handler);
        resolve(msg);
      }
    };
    child.on("message", handler);
  });
}

/** Sends a request-style command to a child and resolves with its `value` reply. */
function request(child: ChildProcess, cmd: Record<string, unknown>): Promise<any> {
  const reqId = ++reqCounter;
  const reply = waitForMessage(child, (m) => m.type === "result" && m.reqId === reqId);
  child.send({ ...cmd, reqId });
  return reply.then((m) => m.value);
}

const RATE_LIMIT_MAX = 5;

function forkInstance(relayPort: number | undefined): ChildProcess {
  // Runs the .ts entry file directly under Node's own module loader hooks
  // (tsx's documented `node --import tsx entry.ts` mode) so fork()'s IPC channel
  // setup — which requires a real, unwrapped Node process — works normally.
  return fork(instanceScript, [], {
    execArgv: ["--import", "tsx"],
    env: {
      ...process.env,
      ...(relayPort !== undefined ? { RELAY_PORT: String(relayPort) } : {}),
      WS_AGG_MAX_MESSAGES: String(RATE_LIMIT_MAX),
    },
    stdio: ["ignore", "pipe", "pipe", "ipc"],
  });
}

beforeAll(async () => {
  if (!redisUrl) relay = await startRelayServer(0);
  instanceA = forkInstance(relay?.port);
  instanceB = forkInstance(relay?.port);
  await Promise.all([
    waitForMessage(instanceA, (m) => m.type === "ready"),
    waitForMessage(instanceB, (m) => m.type === "ready"),
  ]);
});

afterAll(async () => {
  instanceA?.kill();
  instanceB?.kill();
  await relay?.close();
});

describe("cross-instance delivery", () => {
  it("delivers an event published on instance B to a subscriber on instance A", async () => {
    const payload = JSON.stringify({ eventId: 1, hello: "world" });
    const received = waitForMessage(instanceA, (m) => m.type === "received" && m.msg === payload);
    instanceB.send({ type: "publish", msg: payload });
    const msg = await received;
    expect(msg.msg).toBe(payload);
  });

  it("delivers events symmetrically in the other direction too", async () => {
    const payload = JSON.stringify({ eventId: 2, hello: "reverse" });
    const received = waitForMessage(instanceB, (m) => m.type === "received" && m.msg === payload);
    instanceA.send({ type: "publish", msg: payload });
    const msg = await received;
    expect(msg.msg).toBe(payload);
  });

  it("delivers cross-instance within a p99 < 200ms latency bound", async () => {
    const ITERATIONS = 50;
    const latencies: number[] = [];

    for (let i = 0; i < ITERATIONS; i++) {
      const payload = JSON.stringify({ eventId: 100 + i });
      const received = waitForMessage(instanceA, (m) => m.type === "received" && m.msg === payload);
      const sentAt = Date.now();
      instanceB.send({ type: "publish", msg: payload });
      await received;
      latencies.push(Date.now() - sentAt);
    }

    latencies.sort((a, b) => a - b);
    const p99 = latencies[Math.floor(latencies.length * 0.99)];
    expect(p99).toBeLessThan(200);
  });
});

describe("cross-instance distributed lock", () => {
  it("lets only one instance hold the lock until it is released", async () => {
    const key = `qc:test:lock:${runId}`;
    expect(await request(instanceA, { type: "lock", key, holder: "A", ttlMs: 10_000 })).toBe(true);
    expect(await request(instanceB, { type: "lock", key, holder: "B", ttlMs: 10_000 })).toBe(false);
    // The current holder may renew its own lock.
    expect(await request(instanceA, { type: "lock", key, holder: "A", ttlMs: 10_000 })).toBe(true);

    await request(instanceA, { type: "unlock", key, holder: "A" });
    expect(await request(instanceB, { type: "lock", key, holder: "B", ttlMs: 10_000 })).toBe(true);
    await request(instanceB, { type: "unlock", key, holder: "B" });
  });
});

// The aggregate WS rate limiter only shares state across instances via Redis
// (INCR+PEXPIRE); the relay stand-in has no equivalent, so this needs REDIS_URL.
describe.skipIf(!redisUrl)("cross-instance WS aggregate rate limiter (Redis)", () => {
  it("shares one budget across instances so spreading hits does not bypass the limit", async () => {
    const key = `ip:${runId}`;
    for (let i = 0; i < RATE_LIMIT_MAX - 2; i++) {
      expect(await request(instanceA, { type: "hit", key })).toBe(false);
    }
    expect(await request(instanceB, { type: "hit", key })).toBe(false);
    expect(await request(instanceB, { type: "isBlocked", key })).toBe(false);

    // The RATE_LIMIT_MAX-th hit overall — landing on B — trips the shared limit…
    expect(await request(instanceB, { type: "hit", key })).toBe(true);
    // …and A, which only saw RATE_LIMIT_MAX - 2 hits itself, sees the block too.
    expect(await request(instanceA, { type: "isBlocked", key })).toBe(true);
  });
});
