import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import { handleApiV1Request } from "../src/http/apiV1Routes.js";
import {
  AttestorFeedbackStore,
  BASE_REPUTATION_BPS,
  FeedbackValidationError,
  MAX_REPUTATION_ADJUSTMENT_BPS,
} from "../src/attestors/feedbackStore.js";

const AUTH_SECRET = "test-secret-with-at-least-32-characters";

function clockStore(windowSize?: number) {
  let now = 1_000;
  const store = new AttestorFeedbackStore({
    now: () => (now += 1),
    trendWindow: windowSize,
  });
  return store;
}

describe("AttestorFeedbackStore", () => {
  it("rejects self-ratings and out-of-range or non-integer scores", () => {
    const store = clockStore();
    expect(() => store.submit("att", "att", 5)).toThrow(FeedbackValidationError);
    expect(() => store.submit("att", "r1", 0)).toThrow(FeedbackValidationError);
    expect(() => store.submit("att", "r1", 6)).toThrow(FeedbackValidationError);
    expect(() => store.submit("att", "r1", 3.5)).toThrow(FeedbackValidationError);
    expect(() => store.submit("att", "r1", "5")).toThrow(FeedbackValidationError);
    expect(() => store.submit("", "r1", 5)).toThrow(FeedbackValidationError);
  });

  it("keeps one rating per rater, so re-submitting replaces rather than stacks", () => {
    const store = clockStore();
    store.submit("att", "r1", 1);
    const summary = store.submit("att", "r1", 5);
    expect(summary.count).toBe(1);
    expect(summary.average).toBe(5);
  });

  it("leaves reputation neutral until enough independent ratings exist", () => {
    const store = clockStore();
    store.submit("att", "r1", 5);
    store.submit("att", "r2", 5);
    expect(store.summary("att").reputationBps).toBe(BASE_REPUTATION_BPS);
    expect(store.submit("att", "r3", 5).reputationBps).toBe(
      BASE_REPUTATION_BPS + MAX_REPUTATION_ADJUSTMENT_BPS
    );
  });

  it("moves reputation down for poor averages and clamps the adjustment", () => {
    const store = clockStore();
    store.submit("att", "r1", 1);
    store.submit("att", "r2", 1);
    const summary = store.submit("att", "r3", 1);
    expect(summary.reputationBps).toBe(BASE_REPUTATION_BPS - MAX_REPUTATION_ADJUSTMENT_BPS);
  });

  it("reports an upward trend when recent ratings outscore the previous window", () => {
    const store = clockStore(2);
    for (const score of [2, 2, 5, 5]) {
      store.submit("att", `r${score}-${Math.random()}`, score);
    }
    const { trend } = store.summary("att");
    expect(trend.previousAverage).toBe(2);
    expect(trend.recentAverage).toBe(5);
    expect(trend.direction).toBe("up");
  });

  it("reports flat when there is no previous window to compare against", () => {
    const store = clockStore(5);
    store.submit("att", "r1", 4);
    expect(store.summary("att").trend.direction).toBe("flat");
  });

  it("returns an empty summary for an unknown attestor", () => {
    const summary = clockStore().summary("nobody");
    expect(summary.count).toBe(0);
    expect(summary.average).toBeNull();
    expect(summary.reputationBps).toBe(BASE_REPUTATION_BPS);
  });
});

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

async function startServer(): Promise<string> {
  const limiter = new LocalApiRateLimiter({
    windowMs: 60_000,
    tiers: { free: 100, pro: 100, enterprise: 100 },
  });
  const server = createServer((req, res) => {
    handleApiV1Request(req, res, new URL(req.url ?? "/", "http://localhost"), {
      authSecret: AUTH_SECRET,
      apiRateLimiter: limiter,
    });
  });
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address() as AddressInfo;
  return `http://127.0.0.1:${address.port}`;
}

describe("POST /api/v1/attestors/{id}/feedback", () => {
  it("requires a bearer token", async () => {
    const baseUrl = await startServer();
    const response = await fetch(`${baseUrl}/api/v1/attestors/att-1/feedback`, {
      method: "POST",
      body: JSON.stringify({ score: 4 }),
    });
    expect(response.status).toBe(401);
  });

  it("accepts a rating from the authenticated rater and returns the summary", async () => {
    const baseUrl = await startServer();
    const token = issueToken(AUTH_SECRET, "rater-1", 60).token;
    const response = await fetch(`${baseUrl}/api/v1/attestors/att-route-1/feedback`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ score: 4, comment: "reliable" }),
    });
    expect(response.status).toBe(201);
    const body = (await response.json()) as { count: number; average: number };
    expect(body.count).toBe(1);
    expect(body.average).toBe(4);
  });

  it("rejects invalid scores and malformed bodies with 400", async () => {
    const baseUrl = await startServer();
    const token = issueToken(AUTH_SECRET, "rater-2", 60).token;
    const headers = { authorization: `Bearer ${token}`, "content-type": "application/json" };
    const bad = await fetch(`${baseUrl}/api/v1/attestors/att-route-2/feedback`, {
      method: "POST",
      headers,
      body: JSON.stringify({ score: 9 }),
    });
    expect(bad.status).toBe(400);
    const malformed = await fetch(`${baseUrl}/api/v1/attestors/att-route-2/feedback`, {
      method: "POST",
      headers,
      body: "not json",
    });
    expect(malformed.status).toBe(400);
  });

  it("returns 405 for non-POST methods", async () => {
    const baseUrl = await startServer();
    const token = issueToken(AUTH_SECRET, "rater-3", 60).token;
    const response = await fetch(`${baseUrl}/api/v1/attestors/att-route-3/feedback`, {
      headers: { authorization: `Bearer ${token}` },
    });
    expect(response.status).toBe(405);
  });
});
