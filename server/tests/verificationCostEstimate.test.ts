import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import { handleApiV1Request } from "../src/http/apiV1Routes.js";
import { ComplexityScorer } from "../src/verification/complexityScorer.js";

// Issue #1783: verification cost estimation.

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

async function startServer(): Promise<{ baseUrl: string; token: string }> {
  const limiter = new LocalApiRateLimiter({
    windowMs: 60_000,
    tiers: { free: 1000, pro: 1000, enterprise: 1000 },
  });
  const server = createServer((req, res) => {
    handleApiV1Request(
      req,
      res,
      new URL(req.url ?? "/", "http://localhost"),
      { authSecret: AUTH_SECRET, apiRateLimiter: limiter }
    );
  });
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address() as AddressInfo;
  return {
    baseUrl: `http://127.0.0.1:${address.port}`,
    token: issueToken(AUTH_SECRET, "cost-user", 60, undefined, "pro").token,
  };
}

function post(
  server: { baseUrl: string; token: string },
  body: string,
  contentType = "application/json"
): Promise<Response> {
  return fetch(`${server.baseUrl}/api/v1/verify/estimate-cost`, {
    method: "POST",
    headers: {
      authorization: `Bearer ${server.token}`,
      "content-type": contentType,
    },
    body,
  });
}

describe("POST /api/v1/verify/estimate-cost", () => {
  it("requires a bearer token", async () => {
    const server = await startServer();
    const response = await fetch(`${server.baseUrl}/api/v1/verify/estimate-cost`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ method: "automated" }),
    });
    expect(response.status).toBe(401);
  });

  it("returns a cost estimate that grows with complexity", async () => {
    const server = await startServer();
    const simple = await post(server, JSON.stringify({ method: "automated" }));
    expect(simple.status).toBe(200);
    const simpleBody = (await simple.json()) as {
      estimate: { estimatedCostCents: number; totalScore: number };
      currency: string;
      costUnit: string;
    };
    expect(simpleBody.currency).toBe("USD");
    expect(simpleBody.costUnit).toBe("cents");

    const complex = await post(
      server,
      JSON.stringify({
        method: "biometric",
        dataFields: 12,
        documentCount: 5,
        requiresBiometric: true,
        requiresManualReview: true,
      })
    );
    const complexBody = (await complex.json()) as {
      estimate: { estimatedCostCents: number; totalScore: number };
    };
    expect(complexBody.estimate.totalScore).toBeGreaterThan(simpleBody.estimate.totalScore);
    expect(complexBody.estimate.estimatedCostCents).toBeGreaterThan(
      simpleBody.estimate.estimatedCostCents
    );
  });

  it("rejects unknown methods and out-of-range counts", async () => {
    const server = await startServer();
    const badMethod = await post(server, JSON.stringify({ method: "telepathy" }));
    expect(badMethod.status).toBe(400);
    expect(await badMethod.json()).toMatchObject({
      error: expect.stringContaining("method must be one of"),
    });

    const negative = await post(
      server,
      JSON.stringify({ method: "document", dataFields: -1 })
    );
    expect(negative.status).toBe(400);

    const fractional = await post(
      server,
      JSON.stringify({ method: "document", documentCount: 1.5 })
    );
    expect(fractional.status).toBe(400);

    const tooMany = await post(
      server,
      JSON.stringify({ method: "document", dataFields: 101 })
    );
    expect(tooMany.status).toBe(400);

    const badFlag = await post(
      server,
      JSON.stringify({ method: "manual", requiresManualReview: "yes" })
    );
    expect(badFlag.status).toBe(400);
  });

  it("rejects non-object bodies, invalid JSON and wrong content types", async () => {
    const server = await startServer();
    expect((await post(server, JSON.stringify(["document"]))).status).toBe(400);
    expect((await post(server, JSON.stringify(null))).status).toBe(400);
    expect((await post(server, "{not json")).status).toBe(400);
    expect((await post(server, "{}", "text/plain")).status).toBe(415);
  });

  it("rejects non-POST methods", async () => {
    const server = await startServer();
    const response = await fetch(`${server.baseUrl}/api/v1/verify/estimate-cost`, {
      headers: { authorization: `Bearer ${server.token}` },
    });
    expect(response.status).toBe(405);
    expect(response.headers.get("allow")).toBe("POST");
  });
});

describe("ComplexityScorer.estimateVerificationCost", () => {
  it("matches the cost produced by scoreVerification for the same inputs", () => {
    const scorer = new ComplexityScorer();
    const input = {
      method: "manual",
      dataFields: 4,
      documentCount: 2,
      requiresBiometric: false,
      requiresManualReview: true,
    };
    const estimate = scorer.estimateVerificationCost(input);
    const scored = scorer.scoreVerification({
      credentialId: "cred-1",
      ...input,
      createdAt: Date.now(),
    });
    expect(scored).toMatchObject({
      totalScore: estimate.totalScore,
      estimatedCostCents: estimate.estimatedCostCents,
      estimatedDurationSeconds: estimate.estimatedDurationSeconds,
    });
  });

  it("does not record scores or trends", () => {
    const scorer = new ComplexityScorer();
    scorer.estimateVerificationCost({
      method: "document",
      dataFields: 3,
      documentCount: 1,
      requiresBiometric: false,
      requiresManualReview: false,
    });
    expect(scorer.getAllScores()).toHaveLength(0);
    expect(scorer.getTrends()).toHaveLength(0);
  });
});
