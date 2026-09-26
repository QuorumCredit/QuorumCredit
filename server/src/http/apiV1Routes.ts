import type { IncomingMessage, ServerResponse } from "node:http";
import { credentialStore } from "../credentials/credentialStore.js";
import {
  credentialImportStore,
  parseCredentialCsv,
  parseCredentialJson,
} from "../batch/credentialImport.js";
import {
  LocalApiRateLimiter,
  type ApiRateLimitResult,
  type ApiRateLimiter,
  type ApiTier,
} from "../auth/apiRateLimiter.js";
import type { RevocationStore } from "../auth/jtiRevocationStore.js";
import { verifyToken } from "../auth/tokens.js";
import { retryExecutor } from "../resilience/retry.js";
import { metrics } from "./metricsRegistry.js";
import { readRequestBody, RequestBodyTooLargeError } from "./requestBody.js";
import { handleGraphqlHttpRequest, type GraphqlContext } from "../graphql/api.js";
import type { TokenPayload } from "../auth/tokens.js";

const MAX_IMPORT_BYTES = 2 * 1024 * 1024;
const MAX_IMPORT_ROWS = 500;
const defaultLimiter = new LocalApiRateLimiter();

export interface ApiV1Context {
  authSecret: string;
  revocationStore?: RevocationStore;
  apiRateLimiter?: ApiRateLimiter;
}

interface AuthorizedRequest {
  payload: TokenPayload;
  rate: ApiRateLimitResult;
}

export function handleApiV1Request(
  req: IncomingMessage,
  res: ServerResponse,
  url: URL,
  context: ApiV1Context
): void {
  void (async () => {
    const authorized = await authorizeApiRequest(req, res, context);
    if (!authorized) return;

    if (url.pathname === "/api/v1/limits") {
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      sendJson(res, 200, limitsBody(authorized.rate));
      return;
    }

    if (url.pathname === "/api/v1/retry-stats") {
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      sendJson(res, 200, { operations: retryExecutor.getStatistics() });
      return;
    }

    if (url.pathname === "/api/v1/credentials/import/batch") {
      if (req.method !== "POST") {
        res.setHeader("allow", "POST");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      await handleCredentialImport(req, res, authorized.payload.sub);
      return;
    }

    const importMatch = url.pathname.match(/^\/api\/v1\/credentials\/import\/([^/]+)$/);
    if (importMatch && req.method === "GET") {
      let importId: string;
      try {
        importId = decodeURIComponent(importMatch[1]!);
      } catch {
        sendJson(res, 400, { error: "invalid import ID" });
        return;
      }
      const batch = credentialImportStore.getBatch(importId, authorized.payload.sub);
      if (!batch) {
        sendJson(res, 404, { error: "import not found" });
        return;
      }
      sendJson(res, 200, batch);
      return;
    }

    sendJson(res, 404, { error: "not found" });
  })().catch((error: unknown) => {
    console.error("[quorum-credit] API v1 request failed", error);
    if (!res.headersSent) sendJson(res, 500, { error: "internal server error" });
    else res.end();
  });
}

export function handleTierLimitedGraphqlRequest(
  req: IncomingMessage,
  res: ServerResponse,
  context: ApiV1Context,
  graphqlContext: GraphqlContext
): void {
  void authorizeApiRequest(req, res, context)
    .then((authorized) => {
      if (authorized) handleGraphqlHttpRequest(req, res, graphqlContext);
    })
    .catch((error: unknown) => {
      console.error("[quorum-credit] GraphQL API request failed", error);
      if (!res.headersSent) sendJson(res, 500, { errors: [{ message: "internal server error" }] });
      else res.end();
    });
}

async function authorizeApiRequest(
  req: IncomingMessage,
  res: ServerResponse,
  context: ApiV1Context
): Promise<AuthorizedRequest | undefined> {
  const token = bearerToken(req);
  if (!token) {
    sendJson(res, 401, { error: "bearer token required" });
    return undefined;
  }
  const verified = context.revocationStore
    ? await verifyToken(context.authSecret, token, context.revocationStore)
    : verifyToken(context.authSecret, token);
  if (!verified.valid || !verified.payload.sub) {
    sendJson(res, 401, { error: "invalid or expired bearer token" });
    return undefined;
  }

  const tier = isApiTier(verified.payload.tier) ? verified.payload.tier : "free";
  const rate = await (context.apiRateLimiter ?? defaultLimiter).consume(
    verified.payload.sub,
    tier
  );
  applyRateLimitHeaders(res, rate);
  metrics.incLabeledCounter("qc_api_requests_total", "tier", tier);
  if (!rate.allowed) {
    res.setHeader(
      "retry-after",
      String(Math.max(1, Math.ceil((rate.resetAt - Date.now()) / 1_000)))
    );
    sendJson(res, 429, { error: "API rate limit exceeded", ...limitsBody(rate) });
    return undefined;
  }
  return { payload: verified.payload, rate };
}

async function handleCredentialImport(
  req: IncomingMessage,
  res: ServerResponse,
  owner: string
): Promise<void> {
  let rows;
  try {
    const body = await readRequestBody(req, MAX_IMPORT_BYTES);
    const contentType = (req.headers["content-type"] ?? "").split(";")[0]?.trim().toLowerCase();
    if (contentType === "application/json") {
      const parsed: unknown = JSON.parse(body.toString("utf8"));
      rows = parseCredentialJson(parsed);
    } else if (contentType === "text/csv" || contentType === "application/csv") {
      rows = parseCredentialCsv(body.toString("utf8"));
    } else {
      sendJson(res, 415, { error: "Content-Type must be application/json or text/csv" });
      return;
    }
  } catch (error) {
    const message = error instanceof Error ? error.message : "invalid import body";
    sendJson(res, error instanceof RequestBodyTooLargeError ? 413 : 400, { error: message });
    return;
  }

  if (rows.length === 0) {
    sendJson(res, 400, { error: "import must contain at least one credential" });
    return;
  }
  if (rows.length > MAX_IMPORT_ROWS) {
    sendJson(res, 413, { error: `import cannot exceed ${MAX_IMPORT_ROWS} rows` });
    return;
  }

  try {
    const batch = credentialImportStore.startImport(rows, credentialStore, owner);
    metrics.incCounter("qc_credential_import_batches_total");
    res.setHeader("location", `/api/v1/credentials/import/${batch.importId}`);
    sendJson(res, 202, batch);
  } catch (error) {
    console.error("[quorum-credit] unable to start credential import", error);
    sendJson(res, 503, { error: "credential import tracking capacity is unavailable" });
  }
}

function bearerToken(req: IncomingMessage): string | undefined {
  const header = req.headers.authorization;
  if (typeof header !== "string") return undefined;
  const match = header.match(/^Bearer\s+(\S+)$/i);
  return match?.[1];
}

function applyRateLimitHeaders(
  res: ServerResponse,
  rate: { limit: number; remaining: number; resetAt: number }
): void {
  res.setHeader("x-ratelimit-limit", String(rate.limit));
  res.setHeader("x-ratelimit-remaining", String(rate.remaining));
  res.setHeader("x-ratelimit-reset", String(Math.ceil(rate.resetAt / 1_000)));
}

function limitsBody(rate: {
  tier: ApiTier;
  limit: number;
  used: number;
  remaining: number;
  resetAt: number;
  windowMs: number;
}) {
  return {
    tier: rate.tier,
    limit: rate.limit,
    used: rate.used,
    remaining: rate.remaining,
    resetAt: rate.resetAt,
    windowMs: rate.windowMs,
  };
}

function isApiTier(value: unknown): value is ApiTier {
  return value === "free" || value === "pro" || value === "enterprise";
}

function sendJson(res: ServerResponse, status: number, body: unknown): void {
  res.writeHead(status, { "content-type": "application/json" });
  res.end(JSON.stringify(body));
}
