import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import { handleApiV1Request } from "../src/http/apiV1Routes.js";
import {
  ContractStateMonitor,
  contractStateMonitor,
  type StateAnomaly,
} from "../src/monitoring/contractStateMonitor.js";

const AUTH_SECRET = "test-secret-with-at-least-32-characters";
const servers: Array<ReturnType<typeof createServer>> = [];

afterEach(async () => {
  await Promise.all(
    servers.splice(0).map(
      (server) =>
        new Promise<void>((resolve, reject) => {
          server.close((error) => (error ? reject(error) : resolve()));
        })
    )
  );
});

function monitor(options: ConstructorParameters<typeof ContractStateMonitor>[0] = {}) {
  let clock = 1_000;
  const m = new ContractStateMonitor({ now: () => clock, ...options });
  return {
    m,
    tick: (seconds = 1) => {
      clock += seconds;
    },
  };
}

describe("ContractStateMonitor change tracking", () => {
  it("records the first observation without reporting a change", () => {
    const { m } = monitor();
    const result = m.observe("total_staked", 100);
    expect(result).toEqual({ anomalies: [] });
    expect(m.snapshot()).toEqual({ total_staked: 100 });
  });

  it("records changes with previous and current values and keeps history", () => {
    const { m, tick } = monitor();
    m.observe("loans", 10);
    tick(5);
    const result = m.observe("loans", 12);
    expect(result.change).toEqual({
      key: "loans",
      previous: 10,
      current: 12,
      changedAt: 1_005,
    });
    expect(m.getHistory("loans")).toHaveLength(1);
  });

  it("ignores repeated identical observations", () => {
    const { m } = monitor();
    m.observe("loans", 10);
    expect(m.observe("loans", 10)).toEqual({ anomalies: [] });
    expect(m.getHistory("loans")).toHaveLength(0);
  });

  it("bounds per-key history", () => {
    const { m } = monitor({ maxHistoryPerKey: 2, defaultMaxRelativeChange: 100 });
    m.observe("k", 1);
    m.observe("k", 2);
    m.observe("k", 3);
    m.observe("k", 4);
    expect(m.getHistory("k").map((c) => c.current)).toEqual([3, 4]);
  });
});

describe("ContractStateMonitor anomaly detection", () => {
  it("flags a numeric move beyond the relative threshold", () => {
    const { m } = monitor({ defaultMaxRelativeChange: 0.5 });
    m.observe("total_staked", 100);
    const result = m.observe("total_staked", 200);
    expect(result.anomalies.map((a) => a.kind)).toEqual(["large_change"]);
    expect(result.anomalies[0]).toMatchObject({ key: "total_staked", previous: 100, current: 200 });
  });

  it("does not flag moves within the threshold", () => {
    const { m } = monitor({ defaultMaxRelativeChange: 0.5 });
    m.observe("total_staked", 100);
    expect(m.observe("total_staked", 140).anomalies).toEqual([]);
  });

  it("honours a per-key threshold", () => {
    const { m } = monitor({ defaultMaxRelativeChange: 0.5 });
    m.watch("fee_bps", { maxRelativeChange: 0.01 });
    m.observe("fee_bps", 100);
    expect(m.observe("fee_bps", 103).anomalies.map((a) => a.kind)).toEqual(["large_change"]);
  });

  it("flags a non-negative key that goes below zero", () => {
    const { m } = monitor();
    m.watch("outstanding", { nonNegative: true });
    m.observe("outstanding", 5);
    const result = m.observe("outstanding", -1);
    expect(result.anomalies.map((a) => a.kind)).toContain("negative_value");
  });

  it("flags a value that changes type", () => {
    const { m } = monitor();
    m.observe("status", 1);
    const result = m.observe("status", "paused");
    expect(result.anomalies.map((a) => a.kind)).toEqual(["type_change"]);
  });

  it("flags a first observation that already violates a non-negative rule", () => {
    const { m } = monitor();
    m.watch("balance", { nonNegative: true });
    expect(m.observe("balance", -3).anomalies.map((a) => a.kind)).toEqual(["negative_value"]);
  });

  it("treats moves away from zero as anomalous rather than dividing by zero", () => {
    const { m } = monitor();
    m.observe("reserve", 0);
    expect(m.observe("reserve", 1).anomalies.map((a) => a.kind)).toEqual(["large_change"]);
  });
});

describe("ContractStateMonitor alerting", () => {
  it("notifies subscribers and stores alerts newest first", () => {
    const { m } = monitor({ defaultMaxRelativeChange: 0.1 });
    const received: StateAnomaly[] = [];
    const unsubscribe = m.onAlert((a) => received.push(a));

    m.observe("k", 100);
    m.observe("k", 300);
    m.observe("k", 900);
    expect(received).toHaveLength(2);
    expect(m.getAlerts().map((a) => a.current)).toEqual([900, 300]);
    expect(m.getAlerts(1)).toHaveLength(1);

    unsubscribe();
    m.observe("k", 9_000);
    expect(received).toHaveLength(2);
  });

  it("keeps alert memory bounded and survives a throwing listener", () => {
    const { m } = monitor({ maxAlerts: 2, defaultMaxRelativeChange: 0.1 });
    m.onAlert(() => {
      throw new Error("listener bug");
    });
    m.observe("k", 1);
    for (let i = 2; i <= 6; i++) m.observe("k", i * 100);
    expect(m.getAlerts()).toHaveLength(2);
  });
});

describe("contractStateMonitor API routes", () => {
  async function startServer(): Promise<string> {
    const server = createServer((req, res) => {
      handleApiV1Request(req, res, new URL(req.url ?? "/", "http://localhost"), {
        authSecret: AUTH_SECRET,
        apiRateLimiter: new LocalApiRateLimiter({
          windowMs: 60_000,
          tiers: { free: 1000, pro: 1000, enterprise: 1000 },
        }),
      });
    });
    servers.push(server);
    await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
    return `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  }

  it("serves the snapshot and recent alerts to authorized callers", async () => {
    contractStateMonitor.observe("route_test_key", 7);
    const baseUrl = await startServer();
    const headers = { authorization: `Bearer ${issueToken(AUTH_SECRET, "ops", 60).token}` };

    const snapshot = await fetch(`${baseUrl}/api/v1/state/snapshot`, { headers });
    expect(snapshot.status).toBe(200);
    expect(await snapshot.json()).toMatchObject({
      values: expect.objectContaining({ route_test_key: 7 }),
    });

    const alerts = await fetch(`${baseUrl}/api/v1/state/alerts?limit=5`, { headers });
    expect(alerts.status).toBe(200);
    expect(await alerts.json()).toHaveProperty("alerts");
  });

  it("rejects bad limits and unauthenticated callers", async () => {
    const baseUrl = await startServer();
    const unauthenticated = await fetch(`${baseUrl}/api/v1/state/alerts`);
    expect(unauthenticated.status).toBe(401);

    const headers = { authorization: `Bearer ${issueToken(AUTH_SECRET, "ops", 60).token}` };
    const bad = await fetch(`${baseUrl}/api/v1/state/alerts?limit=500`, { headers });
    expect(bad.status).toBe(400);
  });

  it("does not expose a write route for observations", async () => {
    const baseUrl = await startServer();
    const headers = { authorization: `Bearer ${issueToken(AUTH_SECRET, "ops", 60).token}` };
    const res = await fetch(`${baseUrl}/api/v1/state/snapshot`, {
      method: "POST",
      headers: { ...headers, "content-type": "application/json" },
      body: JSON.stringify({ key: "x", value: 1 }),
    });
    expect(res.status).toBe(405);
  });
});
