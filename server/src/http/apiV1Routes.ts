import type { IncomingMessage, ServerResponse } from "node:http";
import { credentialStore, type Credential } from "../credentials/credentialStore.js";
import { metadataSearchIndex } from "../credentials/metadataSearchIndex.js";
import { credentialRecommender } from "../recommendations/credentialRecommender.js";
import {
  MAX_NOTE_LENGTH,
  attestationLedger,
  disputeService,
  isDisputeError,
  type AttestationVerdict,
  type DisputeOutcome,
} from "../disputes/attestorDisputes.js";
import { parseFieldsParam, projectCredential } from "../credentials/fieldProjection.js";
import { contractStateMonitor } from "../monitoring/contractStateMonitor.js";
import {
  OnboardingError,
  onboardingStore,
  progressOf,
} from "../onboarding/onboardingStore.js";
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
import {
  ComplexityScorer,
  type VerificationCostInput,
} from "../verification/complexityScorer.js";
import { metrics } from "./metricsRegistry.js";
import { readRequestBody, RequestBodyTooLargeError } from "./requestBody.js";
import { handleGraphqlHttpRequest, type GraphqlContext } from "../graphql/api.js";
import type { TokenPayload } from "../auth/tokens.js";
import {
  attestorFeedbackStore,
  FeedbackValidationError,
} from "../attestors/feedbackStore.js";

const MAX_IMPORT_BYTES = 2 * 1024 * 1024;
const MAX_IMPORT_ROWS = 500;
// Issue #1783: cost-estimate bodies are a handful of scalar fields.
const MAX_COST_ESTIMATE_BYTES = 4 * 1024;
const MAX_COST_ESTIMATE_FIELD_COUNT = 100;
const VERIFICATION_METHODS = ["automated", "document", "manual", "biometric"] as const;
const MAX_FEEDBACK_BYTES = 16 * 1024;
const defaultLimiter = new LocalApiRateLimiter();
// Estimation is stateless, so one shared scorer is safe across requests.
const costEstimator = new ComplexityScorer();

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

    if (url.pathname === "/api/v1/verify/estimate-cost") {
      if (req.method !== "POST") {
        res.setHeader("allow", "POST");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      await handleVerificationCostEstimate(req, res);
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

    // Issue #1775: full-text search over credential metadata, plus index stats and maintenance.
    if (url.pathname === "/api/v1/credentials/metadata/search") {
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      handleMetadataSearch(url, res);
      return;
    }

    if (url.pathname === "/api/v1/credentials/metadata/index/stats") {
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      sendJson(res, 200, metadataSearchIndex.stats());
      return;
    }

    if (url.pathname === "/api/v1/credentials/metadata/index/maintenance") {
      if (req.method !== "POST") {
        res.setHeader("allow", "POST");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      sendJson(res, 200, metadataSearchIndex.runMaintenance());
      return;
    }

    // Issue #1778: recommendations are always for the authenticated holder.
    if (url.pathname === "/api/v1/credentials/recommendations") {
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      const limit = parseOptionalInt(url.searchParams.get("limit"));
      sendJson(res, 200, credentialRecommender.recommend(authorized.payload.sub, limit));
      return;
    }

    if (url.pathname === "/api/v1/credentials/recommendations/feedback") {
      if (req.method !== "POST") {
        res.setHeader("allow", "POST");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      await handleRecommendationFeedback(req, res, authorized.payload.sub);
      return;
    }

    if (url.pathname === "/api/v1/credentials/recommendations/effectiveness") {
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      sendJson(res, 200, credentialRecommender.effectiveness());
      return;
    }

    // Issue #1777: attestor verdicts, conflict-gated disputes, and dispute resolution.
    if (url.pathname === "/api/v1/disputes") {
      if (req.method === "POST") {
        await handleOpenDispute(req, res, authorized.payload.sub);
        return;
      }
      if (req.method === "GET") {
        const status = url.searchParams.get("status");
        if (status !== null && status !== "open" && status !== "resolved") {
          sendJson(res, 400, { error: "status must be open or resolved" });
          return;
        }
        const disputes = disputeService.list({
          credentialId: url.searchParams.get("credentialId") ?? undefined,
          status: (status ?? undefined) as "open" | "resolved" | undefined,
        });
        sendJson(res, 200, { disputes });
        return;
      }
      res.setHeader("allow", "GET, POST");
      sendJson(res, 405, { error: "method not allowed" });
      return;
    }

    const disputeMatch = url.pathname.match(/^\/api\/v1\/disputes\/([^/]+)(\/resolve)?$/);
    if (disputeMatch) {
      let disputeId: string;
      try {
        disputeId = decodeURIComponent(disputeMatch[1]!);
      } catch {
        sendJson(res, 400, { error: "invalid dispute ID" });
        return;
      }
      if (disputeMatch[2]) {
        if (req.method !== "POST") {
          res.setHeader("allow", "POST");
          sendJson(res, 405, { error: "method not allowed" });
          return;
        }
        await handleResolveDispute(req, res, disputeId, authorized.payload.sub);
        return;
      }
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      const dispute = disputeService.get(disputeId);
      if (!dispute) {
        sendJson(res, 404, { error: "dispute not found" });
        return;
      }
      sendJson(res, 200, dispute);
      return;
    }

    const attestationMatch = url.pathname.match(/^\/api\/v1\/credentials\/([^/]+)\/attestations$/);
    if (attestationMatch) {
      if (req.method !== "POST") {
        res.setHeader("allow", "POST");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      let credentialId: string;
      try {
        credentialId = decodeURIComponent(attestationMatch[1]!);
      } catch {
        sendJson(res, 400, { error: "invalid credential ID" });
        return;
      }
      await handleRecordAttestation(req, res, credentialId, authorized.payload.sub);
      return;
    }

    // Issue #1779: progressive disclosure — return only the fields requested via ?fields=
    const credentialMatch = url.pathname.match(/^\/api\/v1\/credentials\/([^/]+)$/);
    if (credentialMatch && req.method === "GET") {
      let credentialId: string;
      try {
        credentialId = decodeURIComponent(credentialMatch[1]!);
      } catch {
        sendJson(res, 400, { error: "invalid credential ID" });
        return;
      }
      let paths: string[] | undefined;
      try {
        paths = parseFieldsParam(url.searchParams.get("fields"));
      } catch (error) {
        const message = error instanceof Error ? error.message : "invalid fields parameter";
        sendJson(res, 400, { error: message });
        return;
      }
      const credential = credentialStore.getCredential(credentialId);
      // Holder-only access; other callers get a 404 so existence is not disclosed.
      if (!credential || credential.holderId !== authorized.payload.sub) {
        sendJson(res, 404, { error: "credential not found" });
        return;
      }
      sendJson(res, 200, projectCredential(credential, paths));
      return;
    }

    // Issue #1782: read-only view of contract state monitoring. Observations are
    // ingested in-process only, so there is deliberately no write route here.
    if (url.pathname === "/api/v1/state/snapshot") {
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      sendJson(res, 200, { values: contractStateMonitor.snapshot() });
      return;
    }

    if (url.pathname === "/api/v1/state/alerts") {
      if (req.method !== "GET") {
        res.setHeader("allow", "GET");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      const rawLimit = url.searchParams.get("limit");
      const limit = rawLimit === null ? 25 : Number(rawLimit);
      if (!Number.isInteger(limit) || limit < 1 || limit > 100) {
        sendJson(res, 400, { error: "limit must be an integer between 1 and 100" });
        return;
      }
      sendJson(res, 200, { alerts: contractStateMonitor.getAlerts(limit) });
      return;
    }

    if (url.pathname.startsWith("/api/v1/onboarding")) {
      await handleOnboardingRoute(req, res, url, authorized.payload.sub);
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

    const feedbackMatch = url.pathname.match(/^\/api\/v1\/attestors\/([^/]+)\/feedback$/);
    if (feedbackMatch) {
      if (req.method !== "POST") {
        res.setHeader("allow", "POST");
        sendJson(res, 405, { error: "method not allowed" });
        return;
      }
      let attestorId: string;
      try {
        attestorId = decodeURIComponent(feedbackMatch[1]!);
      } catch {
        sendJson(res, 400, { error: "invalid attestor ID" });
        return;
      }
      await handleAttestorFeedback(req, res, attestorId, authorized.payload.sub);
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

async function handleVerificationCostEstimate(
  req: IncomingMessage,
  res: ServerResponse
): Promise<void> {
  let parsed: unknown;
  try {
    const contentType = (req.headers["content-type"] ?? "").split(";")[0]?.trim().toLowerCase();
    if (contentType !== "application/json") {
      sendJson(res, 415, { error: "Content-Type must be application/json" });
      return;
    }
    const body = await readRequestBody(req, MAX_COST_ESTIMATE_BYTES);
    parsed = JSON.parse(body.toString("utf8"));
  } catch (error) {
    if (error instanceof RequestBodyTooLargeError) {
      sendJson(res, 413, { error: error.message });
    } else {
      sendJson(res, 400, { error: "request body must be valid JSON" });
    }
    return;
  }

  const input = parseVerificationCostInput(parsed);
  if (!input.ok) {
    sendJson(res, 400, { error: input.error });
    return;
  }

  metrics.incCounter("qc_verification_cost_estimates_total");
  sendJson(res, 200, {
    input: input.value,
    estimate: costEstimator.estimateVerificationCost(input.value),
    currency: "USD",
    costUnit: "cents",
  });
}

/**
 * Issue #1783: validate and normalise a cost-estimate body. Fails closed on any
 * unexpected type so the scorer only ever sees well-formed numeric inputs.
 */
export function parseVerificationCostInput(
  body: unknown
): { ok: true; value: VerificationCostInput } | { ok: false; error: string } {
  if (typeof body !== "object" || body === null || Array.isArray(body)) {
    return { ok: false, error: "request body must be a JSON object" };
  }
  const fields = body as Record<string, unknown>;

  const method = fields.method;
  if (
    typeof method !== "string" ||
    !(VERIFICATION_METHODS as readonly string[]).includes(method)
  ) {
    return {
      ok: false,
      error: `method must be one of: ${VERIFICATION_METHODS.join(", ")}`,
    };
  }

  const dataFields = countField(fields.dataFields, "dataFields");
  if (typeof dataFields !== "number") return { ok: false, error: dataFields.error };
  const documentCount = countField(fields.documentCount, "documentCount");
  if (typeof documentCount !== "number") return { ok: false, error: documentCount.error };

  const requiresBiometric = booleanField(fields.requiresBiometric, "requiresBiometric");
  if (typeof requiresBiometric !== "boolean") return { ok: false, error: requiresBiometric.error };
  const requiresManualReview = booleanField(fields.requiresManualReview, "requiresManualReview");
  if (typeof requiresManualReview !== "boolean") {
    return { ok: false, error: requiresManualReview.error };
  }

  return {
    ok: true,
    value: {
      method,
      dataFields,
      documentCount,
      requiresBiometric,
      requiresManualReview,
    },
  };
}

// Optional count fields default to 0 when omitted; a present value must be a
// non-negative integer within the cap. Returns the number or an error object.
function countField(value: unknown, name: string): number | { error: string } {
  if (value === undefined) return 0;
  if (
    typeof value !== "number" ||
    !Number.isInteger(value) ||
    value < 0 ||
    value > MAX_COST_ESTIMATE_FIELD_COUNT
  ) {
    return {
      error: `${name} must be an integer between 0 and ${MAX_COST_ESTIMATE_FIELD_COUNT}`,
    };
  }
  return value;
}

function booleanField(value: unknown, name: string): boolean | { error: string } {
  if (value === undefined) return false;
  if (typeof value !== "boolean") return { error: `${name} must be a boolean` };
  return value;
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

const MAX_JSON_BODY_BYTES = 16 * 1024;

async function readJsonObject(
  req: IncomingMessage,
  res: ServerResponse
): Promise<Record<string, unknown> | undefined> {
  try {
    const body = await readRequestBody(req, MAX_JSON_BODY_BYTES);
    const parsed: unknown = JSON.parse(body.toString("utf8") || "null");
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      sendJson(res, 400, { error: "request body must be a JSON object" });
      return undefined;
    }
    return parsed as Record<string, unknown>;
  } catch (error) {
    sendJson(res, error instanceof RequestBodyTooLargeError ? 413 : 400, {
      error: error instanceof RequestBodyTooLargeError ? error.message : "request body must be valid JSON",
    });
    return undefined;
  }
}

async function handleRecommendationFeedback(
  req: IncomingMessage,
  res: ServerResponse,
  holderId: string
): Promise<void> {
  const body = await readJsonObject(req, res);
  if (!body) return;
  if (typeof body.itemId !== "string" || body.itemId.length === 0) {
    sendJson(res, 400, { error: "itemId is required" });
    return;
  }
  if (body.outcome !== "accepted" && body.outcome !== "dismissed") {
    sendJson(res, 400, { error: "outcome must be accepted or dismissed" });
    return;
  }
  const result = credentialRecommender.recordFeedback(holderId, body.itemId, body.outcome);
  if ("error" in result) {
    sendJson(res, result.status, { error: result.error });
    return;
  }
  sendJson(res, 201, result);
}

async function handleRecordAttestation(
  req: IncomingMessage,
  res: ServerResponse,
  credentialId: string,
  attestor: string
): Promise<void> {
  const body = await readJsonObject(req, res);
  if (!body) return;
  if (!credentialStore.getCredential(credentialId)) {
    sendJson(res, 404, { error: "credential not found" });
    return;
  }
  const note = typeof body.note === "string" ? body.note.slice(0, MAX_NOTE_LENGTH) : undefined;
  const result = attestationLedger.record(credentialId, attestor, body.verdict as AttestationVerdict, note);
  if (isDisputeError(result)) {
    sendJson(res, result.status, { error: result.error });
    return;
  }
  sendJson(res, 201, result);
}

async function handleOpenDispute(req: IncomingMessage, res: ServerResponse, raisedBy: string): Promise<void> {
  const body = await readJsonObject(req, res);
  if (!body) return;
  if (typeof body.credentialId !== "string" || body.credentialId.length === 0) {
    sendJson(res, 400, { error: "credentialId is required" });
    return;
  }
  if (!Array.isArray(body.attestors) || !body.attestors.every((a) => typeof a === "string" && a.length > 0)) {
    sendJson(res, 400, { error: "attestors must be an array of attestor IDs" });
    return;
  }
  const result = disputeService.open({
    credentialId: body.credentialId,
    raisedBy,
    attestors: body.attestors as string[],
    reason: body.reason as string,
  });
  if (isDisputeError(result)) {
    sendJson(res, result.status, { error: result.error });
    return;
  }
  metrics.incCounter("qc_disputes_opened_total");
  res.setHeader("location", `/api/v1/disputes/${result.id}`);
  sendJson(res, 201, result);
}

async function handleResolveDispute(
  req: IncomingMessage,
  res: ServerResponse,
  disputeId: string,
  resolver: string
): Promise<void> {
  const body = await readJsonObject(req, res);
  if (!body) return;
  const result = disputeService.resolve(disputeId, {
    resolver,
    outcome: body.outcome as DisputeOutcome,
    note: typeof body.note === "string" ? body.note : undefined,
  });
  if (isDisputeError(result)) {
    sendJson(res, result.status, { error: result.error });
    return;
  }
  metrics.incCounter("qc_disputes_resolved_total");
  sendJson(res, 200, result);
}

const CREDENTIAL_STATUSES: ReadonlyArray<Credential["status"]> = ["active", "revoked", "expired", "suspended"];
const CREDENTIAL_TYPES: ReadonlyArray<Credential["type"]> = ["identity", "education", "professional", "financial"];

function parseOptionalInt(value: string | null): number | undefined {
  if (value === null) return undefined;
  const parsed = Number.parseInt(value, 10);
  return Number.isFinite(parsed) ? parsed : undefined;
}

function handleMetadataSearch(url: URL, res: ServerResponse): void {
  const q = url.searchParams.get("q") ?? "";
  const status = url.searchParams.get("status");
  const type = url.searchParams.get("type");
  if (status !== null && !CREDENTIAL_STATUSES.includes(status as Credential["status"])) {
    sendJson(res, 400, { error: "status must be one of active, revoked, expired, suspended" });
    return;
  }
  if (type !== null && !CREDENTIAL_TYPES.includes(type as Credential["type"])) {
    sendJson(res, 400, { error: "type must be one of identity, education, professional, financial" });
    return;
  }
  const result = metadataSearchIndex.search(q, {
    status: (status ?? undefined) as Credential["status"] | undefined,
    type: (type ?? undefined) as Credential["type"] | undefined,
    limit: parseOptionalInt(url.searchParams.get("limit")),
    offset: parseOptionalInt(url.searchParams.get("offset")),
  });
  if ("error" in result) {
    sendJson(res, 400, { error: result.error });
    return;
  }
  sendJson(res, 200, result);
}

// Issue #1780: credential holder onboarding
//   POST /api/v1/onboarding/start
//   GET  /api/v1/onboarding/:flowId
//   POST /api/v1/onboarding/:flowId/steps/:step
//   POST /api/v1/onboarding/:flowId/complete
const ONBOARDING_ROUTE = /^\/api\/v1\/onboarding(?:\/([^/]+)(?:\/(steps\/([^/]+)|complete))?)?$/;

async function handleOnboardingRoute(
  req: IncomingMessage,
  res: ServerResponse,
  url: URL,
  holderId: string
): Promise<void> {
  const match = url.pathname.match(ONBOARDING_ROUTE);
  if (!match) {
    sendJson(res, 404, { error: "not found" });
    return;
  }
  const [, rawFlowId, tail, rawStep] = match;

  if (!rawFlowId) {
    sendJson(res, 404, { error: "not found" });
    return;
  }

  if (rawFlowId === "start" && !tail) {
    if (req.method !== "POST") {
      res.setHeader("allow", "POST");
      sendJson(res, 405, { error: "method not allowed" });
      return;
    }
    const flow = onboardingStore.start(holderId);
    sendJson(res, 200, { ...flow, progress: progressOf(flow) });
    return;
  }

  let flowId: string;
  let step: string | undefined;
  try {
    flowId = decodeURIComponent(rawFlowId);
    step = rawStep ? decodeURIComponent(rawStep) : undefined;
  } catch {
    sendJson(res, 400, { error: "invalid onboarding ID" });
    return;
  }

  if (!tail) {
    if (req.method !== "GET") {
      res.setHeader("allow", "GET");
      sendJson(res, 405, { error: "method not allowed" });
      return;
    }
    const flow = onboardingStore.getFlow(flowId, holderId);
    if (!flow) {
      sendJson(res, 404, { error: "onboarding flow not found" });
      return;
    }
    sendJson(res, 200, { ...flow, progress: progressOf(flow) });
    return;
  }

  if (tail === "complete") {
    if (req.method !== "POST") {
      res.setHeader("allow", "POST");
      sendJson(res, 405, { error: "method not allowed" });
      return;
    }
    await runOnboardingTransition(res, holderId, flowId, (id, holder) =>
      onboardingStore.complete(id, holder)
    );
    return;
  }

  if (req.method !== "POST") {
    res.setHeader("allow", "POST");
    sendJson(res, 405, { error: "method not allowed" });
    return;
  }
  await runOnboardingTransition(res, holderId, flowId, (id, holder) =>
    onboardingStore.completeStep(id, holder, step ?? "")
  );
}

async function runOnboardingTransition(
  res: ServerResponse,
  holderId: string,
  flowId: string,
  transition: (flowId: string, holderId: string) => ReturnType<typeof onboardingStore.complete>
): Promise<void> {
  try {
    const flow = transition(flowId, holderId);
    if (!flow) {
      sendJson(res, 404, { error: "onboarding flow not found" });
      return;
    }
    sendJson(res, 200, { ...flow, progress: progressOf(flow) });
  } catch (error) {
    if (error instanceof OnboardingError) {
      const status = error.code === "unknown_step" ? 400 : 409;
      sendJson(res, status, { error: error.message, code: error.code, missingSteps: error.missingSteps });
      return;
    }
    throw error;
  }
}

async function handleAttestorFeedback(
  req: IncomingMessage,
  res: ServerResponse,
  attestorId: string,
  rater: string
): Promise<void> {
  let parsed: unknown;
  try {
    const body = await readRequestBody(req, MAX_FEEDBACK_BYTES);
    parsed = JSON.parse(body.toString("utf8"));
  } catch (error) {
    if (error instanceof RequestBodyTooLargeError) {
      sendJson(res, 413, { error: "feedback body is too large" });
    } else {
      sendJson(res, 400, { error: "request body must be valid JSON" });
    }
    return;
  }
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    sendJson(res, 400, { error: "request body must be a JSON object" });
    return;
  }
  const { score, comment } = parsed as { score?: unknown; comment?: unknown };
  try {
    const summary = attestorFeedbackStore.submit(attestorId, rater, score, comment);
    metrics.incCounter("qc_attestor_feedback_total");
    sendJson(res, 201, summary);
  } catch (error) {
    if (error instanceof FeedbackValidationError) {
      sendJson(res, 400, { error: error.message });
      return;
    }
    throw error;
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
