import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { fork, type ChildProcess } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { startRelayServer, type RelayServerHandle } from "../../src/pubsub/relayServer.js";

/**
 * Proves the core claim in issue #1142: an event whose write is only ever observed
 * by one server instance is still delivered to a client connected to a *different*
 * instance, because both instances share a common pub/sub backbone — here, two
 * separate OS processes (`instanceProcess.ts`, forked below) both talking to the
 * same relay server standing in for Redis (see relayServer.ts's docstring for why:
 * no redis-server binary is installable in this sandbox). Production wiring swaps
 * RelayBus for RedisBus against a real Redis via REDIS_URL — re-run this exact test
 * against real Redis by pointing both children at RedisBus instead (see
 * server/README.md) once Redis is available in the target environment.
 *
 * Kept out of `npm test` (separate vitest.multi-instance.config.ts / npm script) so
 * the default CI gate stays fast and hermetic; this one spawns real processes and
 * is run explicitly via `npm run test:multi-instance`.
 */

const __dirname = dirname(fileURLToPath(import.meta.url));
const instanceScript = join(__dirname, "instanceProcess.ts");

let relay: RelayServerHandle;
let instanceA: ChildProcess;
let instanceB: ChildProcess;

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

function forkInstance(port: number): ChildProcess {
  // Runs the .ts entry file directly under Node's own module loader hooks
  // (tsx's documented `node --import tsx entry.ts` mode) so fork()'s IPC channel
  // setup — which requires a real, unwrapped Node process — works normally.
  return fork(instanceScript, [], {
    execArgv: ["--import", "tsx"],
    env: { ...process.env, RELAY_PORT: String(port) },
    stdio: ["ignore", "pipe", "pipe", "ipc"],
  });
}

beforeAll(async () => {
  relay = await startRelayServer(0);
  instanceA = forkInstance(relay.port);
  instanceB = forkInstance(relay.port);
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
