import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import { afterEach, describe, expect, it } from "vitest";
import { issueToken } from "../src/auth/tokens.js";
import { LocalApiRateLimiter } from "../src/auth/apiRateLimiter.js";
import {
  handleApiV1Request,
  handleTierLimitedGraphqlRequest,
} from "../src/http/apiV1Routes.js";
import { CredentialStore } from "../src/credentials/credentialStore.js";
import type { EventStore } from "../src/bridge/eventStore.js";

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

async function startServer(
  limiter: LocalApiRateLimiter
): Promise<{ baseUrl: string; token: string; otherToken: string }> {
  const server = createServer((req, res) => {
    const context = { authSecret: AUTH_SECRET, apiRateLimiter: limiter };
    if (req.url?.startsWith("/graphql")) {
      handleTierLimitedGraphqlRequest(req, res, context, {
        credentials: new CredentialStore(),
        events: { getRecentEvents: () => [] } as unknown as EventStore,
      });
    } else {
      handleApiV1Request(
        req,
        res,
        new URL(req.url ?? "/", "http://localhost"),
        context
      );
    }
  });
  servers.push(server);
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address() as AddressInfo;
  return {
    baseUrl: `http://127.0.0.1:${address.port}`,
    token: issueToken(AUTH_SECRET, "api-user", 60, undefined, "pro").token,
    otherToken: issueToken(AUTH_SECRET, "another-user", 60).token,
  };
}

describe("API v1 routes", () => {
  it("requires tokens and reports/enforces tier quota usage", async () => {
    const server = await startServer(
      new LocalApiRateLimiter({
        windowMs: 60_000,
        tiers: { free: 1, pro: 2, enterprise: 10 },
      })
    );
    const unauthorized = await fetch(`${server.baseUrl}/api/v1/limits`);
    expect(unauthorized.status).toBe(401);

    const first = await fetch(`${server.baseUrl}/api/v1/limits`, {
      headers: { authorization: `Bearer ${server.token}` },
    });
    expect(first.status).toBe(200);
    expect(await first.json()).toMatchObject({
      tier: "pro",
      limit: 2,
      used: 1,
      remaining: 1,
      windowMs: 60_000,
    });
    const second = await fetch(`${server.baseUrl}/api/v1/limits`, {
      headers: { authorization: `Bearer ${server.token}` },
    });
    expect(second.status).toBe(200);
    const limited = await fetch(`${server.baseUrl}/api/v1/limits`, {
      headers: { authorization: `Bearer ${server.token}` },
    });
    expect(limited.status).toBe(429);
    expect(limited.headers.get("retry-after")).not.toBeNull();
    expect(await limited.json()).toMatchObject({ used: 3, remaining: 0 });
    const graphql = await fetch(`${server.baseUrl}/graphql`, {
      method: "POST",
      headers: {
        authorization: `Bearer ${server.token}`,
        "content-type": "application/json",
      },
      body: JSON.stringify({ query: "{ credentials { id } }" }),
    });
    expect(graphql.status).toBe(429);

    const otherUser = await fetch(`${server.baseUrl}/api/v1/limits`, {
      headers: { authorization: `Bearer ${server.otherToken}` },
    });
    expect(otherUser.status).toBe(200);
  });

  it("accepts JSON batch imports and restricts progress polling to the creator", async () => {
    const server = await startServer(
      new LocalApiRateLimiter({
        windowMs: 60_000,
        tiers: { free: 100, pro: 100, enterprise: 100 },
      })
    );
    const response = await fetch(
      `${server.baseUrl}/api/v1/credentials/import/batch`,
      {
        method: "POST",
        headers: {
          authorization: `Bearer ${server.token}`,
          "content-type": "application/json",
        },
        body: JSON.stringify([
          {
            holderId: `holder-${Date.now()}`,
            type: "identity",
            issuer: "integration-test",
            expiresAt: Math.floor(Date.now() / 1_000) + 60,
          },
        ]),
      }
    );
    expect(response.status).toBe(202);
    const created = (await response.json()) as { importId: string };
    expect(response.headers.get("location")).toBe(
      `/api/v1/credentials/import/${created.importId}`
    );

    await new Promise<void>((resolve) => setTimeout(resolve, 10));
    const progress = await fetch(
      `${server.baseUrl}/api/v1/credentials/import/${created.importId}`,
      { headers: { authorization: `Bearer ${server.token}` } }
    );
    expect(progress.status).toBe(200);
    expect(await progress.json()).toMatchObject({
      status: "completed",
      total: 1,
      imported: 1,
      failed: 0,
      progress: 100,
    });

    const privateProgress = await fetch(
      `${server.baseUrl}/api/v1/credentials/import/${created.importId}`,
      { headers: { authorization: `Bearer ${server.otherToken}` } }
    );
    expect(privateProgress.status).toBe(404);
  });
});
