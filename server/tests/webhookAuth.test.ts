import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { createServer, type Server } from "node:http";
import type { AddressInfo } from "node:net";
import { createHash } from "node:crypto";
import { handleWebhookRequest, type WebhookRoutesContext } from "../src/http/webhookRoutes.js";
import { webhookRegistry } from "../src/webhooks/signature.js";
import { issueToken } from "../src/auth/tokens.js";
import { buildApiKeyStore } from "../src/auth/apiKeyStore.js";

function sha256(val: string): string {
  return createHash("sha256").update(val, "utf8").digest("hex");
}

describe("webhook route auth and owner scoping (#2149)", () => {
  let server: Server | undefined;
  let baseUrl = "";

  const authSecret = "test-auth-secret-for-webhook-scoping-32chars";

  const keyClientA = "client-a-api-key";
  const keyClientB = "client-b-api-key";
  const keyAdmin = "admin-api-key";
  const keyLegacy = "test-apikey";

  const hashClientA = sha256(keyClientA);
  const hashClientB = sha256(keyClientB);
  const hashAdmin = sha256(keyAdmin);
  const hashLegacy = sha256(keyLegacy);

  const hashList = [hashClientA, hashClientB, hashAdmin, hashLegacy].join(",");
  const apiKeyStore = buildApiKeyStore(
    hashList,
    {},
    { [hashAdmin]: "admin" }
  );

  const ctx: WebhookRoutesContext = {
    authSecret,
    apiKeyStore,
  };

  beforeEach(async () => {
    // Clear in-memory registrations between tests
    await webhookRegistry.close();

    server = createServer((req, res) => {
      handleWebhookRequest(req, res, ctx);
    });

    await new Promise<void>((resolve) => {
      server!.listen(0, "127.0.0.1", () => resolve());
    });

    const addr = server.address() as AddressInfo;
    baseUrl = `http://127.0.0.1:${addr.port}`;
  });

  afterEach(async () => {
    if (server) {
      await new Promise<void>((resolve) => server!.close(() => resolve()));
      server = undefined;
    }
    await webhookRegistry.close();
  });

  // ── Authentication Checks ───────────────────────────────────────────────────

  it("rejects register without auth with 401", async () => {
    const res = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url: "https://example.com/hook", events: ["loan_issued"] }),
    });
    expect(res.status).toBe(401);
    const body = await res.json();
    expect(body.error).toContain("missing credentials");
  });

  it("rejects register with invalid API key with 401", async () => {
    const res = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-api-key": "bad-key",
      },
      body: JSON.stringify({ url: "https://example.com/hook", events: ["loan_issued"] }),
    });
    expect(res.status).toBe(401);
    const body = await res.json();
    expect(body.error).toBe("invalid credentials");
  });

  it("accepts register with valid bearer token and scopes to owner", async () => {
    const token = issueToken(authSecret, "user-alice", 60);
    const res = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${token.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hook", events: ["loan_issued"] }),
    });
    expect(res.status).toBe(201);
    const body = await res.json();
    expect(body.id).toBeDefined();
    expect(body.owner).toBe("user-alice");
    expect(body.secret).toBeDefined(); // returned only once
  });

  it("accepts register with valid API key and scopes to owner", async () => {
    const res = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-api-key": keyLegacy,
      },
      body: JSON.stringify({ url: "https://example.com/hook", events: ["loan_issued"] }),
    });
    expect(res.status).toBe(201);
    const body = await res.json();
    expect(body.id).toBeDefined();
    expect(body.owner).toBe(keyLegacy);
    expect(body.secret).toBeDefined();
  });

  // ── Scoping & Access Control: Isolation between clients ─────────────────────

  it("does not allow one client to read (GET) another client's webhook", async () => {
    const tokenA = issueToken(authSecret, "client-a", 60);
    const tokenB = issueToken(authSecret, "client-b", 60);

    // Client A registers a webhook
    const regRes = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA", events: ["loan_issued"] }),
    });
    expect(regRes.status).toBe(201);
    const webhookA = await regRes.json();

    // Client A can read their own webhook (secret is omitted)
    const getResA = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      headers: { authorization: `Bearer ${tokenA.token}` },
    });
    expect(getResA.status).toBe(200);
    const gotA = await getResA.json();
    expect(gotA.id).toBe(webhookA.id);
    expect(gotA.owner).toBe("client-a");
    expect(gotA.secret).toBeUndefined();

    // Client B attempts to read Client A's webhook -> 403 Forbidden
    const getResB = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      headers: { authorization: `Bearer ${tokenB.token}` },
    });
    expect(getResB.status).toBe(403);
    const errB = await getResB.json();
    expect(errB.error).toContain("forbidden");
  });

  it("does not allow one client to update (PUT) another client's webhook", async () => {
    const tokenA = issueToken(authSecret, "client-a", 60);
    const tokenB = issueToken(authSecret, "client-b", 60);

    const regRes = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA", events: ["loan_issued"] }),
    });
    const webhookA = await regRes.json();

    // Client B attempts to update Client A's webhook -> 403 Forbidden
    const updateResB = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      method: "PUT",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenB.token}`,
      },
      body: JSON.stringify({ url: "https://attacker.com/hijack" }),
    });
    expect(updateResB.status).toBe(403);

    // Verify webhook was NOT changed
    const verify = await webhookRegistry.getWebhook(webhookA.id);
    expect(verify?.url).toBe("https://example.com/hookA");

    // Client A updates their own webhook -> 200 OK
    const updateResA = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      method: "PUT",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA-updated" }),
    });
    expect(updateResA.status).toBe(200);
    const updated = await updateResA.json();
    expect(updated.url).toBe("https://example.com/hookA-updated");
    expect(updated.secret).toBeUndefined();
  });

  it("does not allow one client to delete (DELETE) another client's webhook", async () => {
    const tokenA = issueToken(authSecret, "client-a", 60);
    const tokenB = issueToken(authSecret, "client-b", 60);

    const regRes = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA", events: ["loan_issued"] }),
    });
    const webhookA = await regRes.json();

    // Client B attempts to delete Client A's webhook -> 403 Forbidden
    const deleteResB = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      method: "DELETE",
      headers: { authorization: `Bearer ${tokenB.token}` },
    });
    expect(deleteResB.status).toBe(403);

    // Verify webhook still exists
    expect(await webhookRegistry.getWebhook(webhookA.id)).toBeDefined();

    // Client A deletes their own webhook -> 200 OK
    const deleteResA = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      method: "DELETE",
      headers: { authorization: `Bearer ${tokenA.token}` },
    });
    expect(deleteResA.status).toBe(200);
    expect(await webhookRegistry.getWebhook(webhookA.id)).toBeUndefined();
  });

  it("does not allow one client to view deliveries or stats of another client's webhook", async () => {
    const tokenA = issueToken(authSecret, "client-a", 60);
    const tokenB = issueToken(authSecret, "client-b", 60);

    const regRes = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA", events: ["loan_issued"] }),
    });
    const webhookA = await regRes.json();

    // Client B tries /deliveries -> 403
    const delResB = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}/deliveries`, {
      headers: { authorization: `Bearer ${tokenB.token}` },
    });
    expect(delResB.status).toBe(403);

    // Client A reads deliveries -> 200
    const delResA = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}/deliveries`, {
      headers: { authorization: `Bearer ${tokenA.token}` },
    });
    expect(delResA.status).toBe(200);

    // Client B tries /stats -> 403
    const statsResB = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}/stats`, {
      headers: { authorization: `Bearer ${tokenB.token}` },
    });
    expect(statsResB.status).toBe(403);

    // Client A reads stats -> 200
    const statsResA = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}/stats`, {
      headers: { authorization: `Bearer ${tokenA.token}` },
    });
    expect(statsResA.status).toBe(200);
  });

  it("does not allow one client to trigger test delivery on another's webhook", async () => {
    const tokenA = issueToken(authSecret, "client-a", 60);
    const tokenB = issueToken(authSecret, "client-b", 60);

    const regRes = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA", events: ["loan_issued"] }),
    });
    const webhookA = await regRes.json();

    const testResB = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}/test`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenB.token}`,
      },
      body: JSON.stringify({ event: "loan_issued", data: { test: true } }),
    });
    expect(testResB.status).toBe(403);
  });

  // ── List Endpoint Scoping ───────────────────────────────────────────────────

  it("restricts list responses to the caller's own webhooks", async () => {
    const tokenA = issueToken(authSecret, "client-a", 60);
    const tokenB = issueToken(authSecret, "client-b", 60);

    // Register 2 for A
    await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA1", events: ["loan_issued"] }),
    });
    await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA2", events: ["payment_received"] }),
    });

    // Register 1 for B
    await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenB.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookB1", events: ["loan_issued"] }),
    });

    // Client A listing webhooks sees only A's 2 webhooks
    const listA = await fetch(`${baseUrl}/api/webhooks`, {
      headers: { authorization: `Bearer ${tokenA.token}` },
    });
    expect(listA.status).toBe(200);
    const webhooksA = await listA.json();
    expect(webhooksA).toHaveLength(2);
    expect(webhooksA.every((w: any) => w.owner === "client-a")).toBe(true);
    expect(webhooksA.every((w: any) => w.secret === undefined)).toBe(true);

    // Client B listing webhooks sees only B's 1 webhook
    const listB = await fetch(`${baseUrl}/api/webhooks`, {
      headers: { authorization: `Bearer ${tokenB.token}` },
    });
    expect(listB.status).toBe(200);
    const webhooksB = await listB.json();
    expect(webhooksB).toHaveLength(1);
    expect(webhooksB[0].owner).toBe("client-b");
    expect(webhooksB[0].secret).toBeUndefined();
  });

  // ── Admin Role Access ───────────────────────────────────────────────────────

  it("allows admin role to list all webhooks and manage any webhook", async () => {
    const tokenA = issueToken(authSecret, "client-a", 60);
    const tokenB = issueToken(authSecret, "client-b", 60);
    const tokenAdmin = issueToken(authSecret, "admin-user", 60, undefined, undefined, "admin");

    const regA = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA", events: ["loan_issued"] }),
    });
    const webhookA = await regA.json();

    await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenB.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookB", events: ["payment_received"] }),
    });

    // Admin lists all webhooks
    const listAdmin = await fetch(`${baseUrl}/api/webhooks`, {
      headers: { authorization: `Bearer ${tokenAdmin.token}` },
    });
    expect(listAdmin.status).toBe(200);
    const allWebhooks = await listAdmin.json();
    expect(allWebhooks).toHaveLength(2);
    expect(allWebhooks.every((w: any) => w.secret === undefined)).toBe(true);

    // Admin can GET Client A's webhook
    const getAdmin = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      headers: { authorization: `Bearer ${tokenAdmin.token}` },
    });
    expect(getAdmin.status).toBe(200);

    // Admin can UPDATE Client A's webhook
    const updateAdmin = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      method: "PUT",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenAdmin.token}`,
      },
      body: JSON.stringify({ enabled: false }),
    });
    expect(updateAdmin.status).toBe(200);

    // Admin can DELETE Client A's webhook
    const deleteAdmin = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      method: "DELETE",
      headers: { authorization: `Bearer ${tokenAdmin.token}` },
    });
    expect(deleteAdmin.status).toBe(200);
  });

  it("supports admin access via admin API key", async () => {
    const tokenA = issueToken(authSecret, "client-a", 60);
    const regA = await fetch(`${baseUrl}/api/webhooks/register`, {
      method: "POST",
      headers: {
        "content-type": "application/json",
        authorization: `Bearer ${tokenA.token}`,
      },
      body: JSON.stringify({ url: "https://example.com/hookA", events: ["loan_issued"] }),
    });
    const webhookA = await regA.json();

    // Admin API key can list all
    const listAdmin = await fetch(`${baseUrl}/api/webhooks`, {
      headers: { "x-api-key": keyAdmin },
    });
    expect(listAdmin.status).toBe(200);
    const all = await listAdmin.json();
    expect(all).toHaveLength(1);

    // Admin API key can read Client A's webhook
    const getAdmin = await fetch(`${baseUrl}/api/webhooks/${webhookA.id}`, {
      headers: { "x-api-key": keyAdmin },
    });
    expect(getAdmin.status).toBe(200);
  });

  // ── Missing / Unknown Webhook ───────────────────────────────────────────────

  it("returns 404 for unknown webhook ID", async () => {
    const token = issueToken(authSecret, "client-a", 60);
    const res = await fetch(`${baseUrl}/api/webhooks/non_existent_id`, {
      headers: { authorization: `Bearer ${token.token}` },
    });
    expect(res.status).toBe(404);
  });
});
