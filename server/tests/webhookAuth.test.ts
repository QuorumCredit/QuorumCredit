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
import { LocalRevocationStore } from "../src/auth/jtiRevocationStore.js";
import { EventEmitter } from "node:events";

function createReq(method = "POST", headers: Record<string, string | string[]> = {}, body?: unknown): IncomingMessage {
  const req = new IncomingMessage(null as any);
  req.method = method;
  req.url = "/api/webhooks/register";
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
  const res = new EventEmitter() as any;
  res.statusCode = 200;
  res.writeHead = (statusCode: number, headers?: Record<string, string>) => {
    res.statusCode = statusCode;
    if (headers) res.headers = headers;
  };
  res.end = (body?: unknown) => {
    if (body !== undefined) {
      res.emit("data", Buffer.from(String(body)));
    }
    res.emit("end");
  };
  res.setHeader = () => {};
  res.getHeader = () => undefined;
  return res as ServerResponse;
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
  const revocationStore = new LocalRevocationStore();

  const ctx = {
    webhookSecret: undefined,
    authSecret,
    revocationStore,
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

  it("rejects register with a revoked bearer token", async () => {
    const issued = (await import("../src/auth/tokens.js")).issueToken(authSecret, "apiKey", 60);
    await revocationStore.revoke(issued.jti, issued.expiresAt);
    const req = createReq("POST", { authorization: `Bearer ${issued.token}` }, { url: "https://example.com/hook", events: ["loan_issued"] });
    const res = createRes();
    handleWebhookRequest(req, res, ctx);
    await new Promise<void>((resolve) => setImmediate(resolve));
    expect(res.statusCode).toBe(401);
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
