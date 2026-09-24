import { describe, it, expect, vi, beforeEach } from "vitest";
import { IncomingMessage, ServerResponse } from "node:http";
import { handleWebhookRequest } from "../src/http/webhookRoutes.js";
import {
  WebhookDeliveryService,
  buildWebhookDeliveryService,
  type WebhookRegistration,
} from "../src/webhooks/delivery.js";
import { LocalWebhookRegistry } from "../src/webhooks/signature.js";
import { verifyToken } from "../src/auth/tokens.js";
import { buildApiKeyStore } from "../src/auth/apiKeyStore.js";

function createReq(method = "POST", headers: Record<string, string | string[]> = {}, body?: unknown): IncomingMessage {
  const req = new IncomingMessage(null as any);
  req.method = method;
  req.headers = headers as any;
  if (body !== undefined) {
    req.on("data", (chunk: Buffer) => req.push(chunk));
    req.on("end", () => {
      req.push(null);
    });
    (req as any).rawBody = JSON.stringify(body);
  }
  return req;
}

function createRes(): ServerResponse {
  const res = new ServerResponse(null as any);
  return res;
}

function readBody(res: ServerResponse): Promise<unknown> {
  return new Promise((resolve) => {
    const chunks: Buffer[] = [];
    res.on("data", (chunk: Buffer) => chunks.push(chunk));
    res.on("end", () => {
      try {
        resolve(chunks.length > 0 ? JSON.parse(Buffer.concat(chunks).toString("utf8")) : null);
      } catch {
        resolve(null);
      }
    });
  });
}

describe("webhook route auth", () => {
  const registry = new LocalWebhookRegistry();
  const delivery = new WebhookDeliveryService({ maxRecordsPerWebhook: 10, sweepIntervalMs: 100_000 });
  const authSecret = "test-secret";
  const apiKeyStore = buildApiKeyStore("dGVzdC1hcGlrZXk=");

  const ctx = {
    webhookSecret: undefined,
    authSecret,
    apiKeyStore,
  } as any;

  it("rejects register without auth", async () => {
    const req = createReq("POST", {}, { url: "https://example.com/hook", events: ["loan_issued"] });
    const res = createRes();
    handleWebhookRequest(req, res, ctx);
    expect(res.statusCode).toBe(401);
  });

  it("accepts register with valid bearer token", async () => {
    const token = (await import("../src/auth/tokens.js")).issueToken(authSecret, "apiKey", 60);
    const req = createReq("POST", { authorization: `Bearer ${token.token}` }, { url: "https://example.com/hook", events: ["loan_issued"] });
    const res = createRes();
    handleWebhookRequest(req, res, ctx);
    expect(res.statusCode).toBe(201);
  });

  it("accepts register with valid API key", async () => {
    const req = createReq("POST", { "x-api-key": "test-apikey" }, { url: "https://example.com/hook", events: ["loan_issued"] });
    const res = createRes();
    handleWebhookRequest(req, res, ctx);
    expect(res.statusCode).toBe(201);
  });

  it("rejects register with invalid API key", async () => {
    const req = createReq("POST", { "x-api-key": "bad-key" }, { url: "https://example.com/hook", events: ["loan_issued"] });
    const res = createRes();
    handleWebhookRequest(req, res, ctx);
    expect(res.statusCode).toBe(401);
  });
});
