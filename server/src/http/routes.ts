import type { IncomingMessage, ServerResponse } from "node:http";
import { issueToken } from "../auth/tokens.js";
import { loadApiKeyStore, type ApiKeyStore } from "../auth/apiKeyStore.js";
import { defaultAuthRateLimiter, type AuthRateLimiter } from "../auth/rateLimiter.js";
import { metrics } from "./metricsRegistry.js";
import { expenseStore, isExpenseCategory } from "../expenses/expenseStore.js";
import { loanCartStore } from "../cart/loanCartStore.js";
import type { RevocationStore } from "../auth/jtiRevocationStore.js";
import type { SorobanRpcClient } from "../soroban/rpcClient.js";
import type { RecurringPaymentStore } from "../recurring/recurringPaymentStore.js";
import { credentialStore } from "../credentials/credentialStore.js";
import {
  generateProof,
  exportProofJson,
  importProofJson,
} from "../credentials/proofGenerator.js";
import { identityVerificationService } from "../credentials/identityVerificationService.js";
import { analyticsService } from "../credentials/analyticsService.js";

export interface RouteContext {
  authSecret: string;
  tokenTtlSeconds: number;
  webhookSecret?: string; // Optional: secret for receiving webhooks
  /** Issue #1227 — undefined only in tests/callers that don't wire cost allocation. */
  costAllocator?: CostAllocator;
  /** Issue #1229 — undefined only in tests/callers that don't wire partition detection. */
  partitionGuard?: PartitionGuard;
  /** Issue #1231 — deployed version string, surfaced on /health for canary monitoring. */
  serviceVersion?: string;
  /** Issue #1292 — JTI denylist store; undefined falls back to no revocation check. */
  revocationStore?: RevocationStore;
  /** Issue #1290 — provisioned API key store. */
  apiKeyStore?: ApiKeyStore;
  /** Issue #1290 — rate limiter for auth endpoint. */
  authRateLimiter?: AuthRateLimiter;
  /** Issue #1362 — Soroban RPC client for on-chain recurring payment execution. */
  rpcClient?: SorobanRpcClient;
  /** Issue #1362 — Persistent recurring payment store (Local or Redis-backed). */
  paymentStore?: RecurringPaymentStore;
}

/**
 * Gate for mutating endpoints (issue #1229): while `ctx.partitionGuard` reports this
 * instance partitioned, the write is queued for replay instead of applied immediately,
 * and the caller gets a 202 rather than the normal success response. Returns true if
 * the request was queued (caller must not also apply the write synchronously).
 */
function queueIfPartitioned(
  ctx: RouteContext,
  res: ServerResponse,
  perform: () => void,
  responseBody: Record<string, unknown> = {}
): boolean {
  if (!ctx.partitionGuard?.isPartitioned()) return false;
  ctx.partitionGuard.enqueue(perform);
  metrics.incCounter("qc_partition_writes_queued_total");
  res.writeHead(202, { "content-type": "application/json" });
  res.end(
    JSON.stringify({ ...responseBody, queued: true, reason: "network partition detected; write queued for replay on recovery" })
  );
  return true;
}

interface TokenRequestBody {
  apiKey?: string;
  borrower?: string;
}

interface ExpenseRequestBody {
  category?: string;
  amount?: number;
  description?: string;
  declaredPurpose?: string;
}

interface RecurringPaymentRequestBody {
  amount?: number;
  frequencySeconds?: number;
  startDate?: number;
}

interface CartRequestBody {
  borrower?: string;
  amount?: number;
  tenureSeconds?: number;
}

/** Minimal router for the handful of REST endpoints this service exposes — not
 * pulling in Express for three routes. */
export function handleHttpRequest(
  req: IncomingMessage,
  res: ServerResponse,
  ctx: RouteContext
): void {
  const url = new URL(req.url ?? "", "http://internal");

  // Health check
  if (req.method === "GET" && url.pathname === "/health") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ status: "ok", version: ctx.serviceVersion ?? "unknown" }));
    return;
  }

  // Metrics endpoint
  if (req.method === "GET" && url.pathname === "/metrics") {
    res.writeHead(200, { "content-type": "text/plain; version=0.0.4" });
    res.end(metrics.toPrometheusText());
    return;
  }

  const expensesMatch = url.pathname.match(/^\/loans\/([^/]+)\/expenses$/);
  if (expensesMatch) {
    const loanId = decodeURIComponent(expensesMatch[1] as string);

    if (req.method === "POST") {
      readJsonBody<ExpenseRequestBody>(req)
        .then((body) => {
          if (!isExpenseCategory(body.category)) {
            res.writeHead(400, { "content-type": "application/json" });
            res.end(JSON.stringify({ error: "category must be one of business, education, healthcare, other" }));
            return;
          }
          if (typeof body.amount !== "number" || !Number.isFinite(body.amount) || body.amount <= 0) {
            res.writeHead(400, { "content-type": "application/json" });
            res.end(JSON.stringify({ error: "amount must be a positive number" }));
            return;
          }
          const category = body.category;
          const amount = body.amount;
          const description = body.description ?? "";
          const declaredPurpose = body.declaredPurpose;

          if (
            queueIfPartitioned(
              ctx,
              res,
              () => {
                if (declaredPurpose) expenseStore.setDeclaredPurpose(loanId, declaredPurpose);
                expenseStore.addExpense(loanId, category, amount, description);
                metrics.incCounter("qc_expenses_recorded_total");
              },
              { loanId, category, amount }
            )
          ) {
            return;
          }

          if (declaredPurpose) expenseStore.setDeclaredPurpose(loanId, declaredPurpose);
          const expense = expenseStore.addExpense(loanId, category, amount, description);
          metrics.incCounter("qc_expenses_recorded_total");
          res.writeHead(201, { "content-type": "application/json" });
          res.end(JSON.stringify(expense));
        })
        .catch(() => {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "invalid request body" }));
        });
      return;
    }

    if (req.method === "GET") {
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(expenseStore.listExpenses(loanId)));
      return;
    }
  }

  const expenseBreakdownMatch = url.pathname.match(/^\/loans\/([^/]+)\/expense-breakdown$/);
  if (expenseBreakdownMatch && req.method === "GET") {
    const loanId = decodeURIComponent(expenseBreakdownMatch[1] as string);
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(expenseStore.breakdown(loanId)));
    return;
  }

  const recurringPaymentMatch = url.pathname.match(/^\/loans\/([^/]+)\/recurring-payment$/);
  if (recurringPaymentMatch) {
    const loanId = decodeURIComponent(recurringPaymentMatch[1] as string);
    const store = ctx.paymentStore;

    if (req.method === "POST") {
      readJsonBody<RecurringPaymentRequestBody>(req)
        .then(async (body) => {
          if (typeof body.amount !== "number" || !Number.isFinite(body.amount) || body.amount <= 0) {
            res.writeHead(400, { "content-type": "application/json" });
            res.end(JSON.stringify({ error: "amount must be a positive number" }));
            return;
          }
          if (typeof body.frequencySeconds !== "number" || body.frequencySeconds <= 0) {
            res.writeHead(400, { "content-type": "application/json" });
            res.end(JSON.stringify({ error: "frequencySeconds must be a positive number" }));
            return;
          }
          const amount = body.amount;
          const frequencySeconds = body.frequencySeconds;
          const startDate = typeof body.startDate === "number" ? body.startDate : Date.now();

          if (
            queueIfPartitioned(
              ctx,
              res,
              () => {
                void store?.setup(loanId, amount, frequencySeconds, startDate).then(() => {
                  metrics.incCounter("qc_recurring_payments_setup_total");
                });
              },
              { loanId, amount, frequencySeconds, startDate }
            )
          ) {
            return;
          }

          if (!store) {
            res.writeHead(503, { "content-type": "application/json" });
            res.end(JSON.stringify({ error: "payment store not configured" }));
            return;
          }

          const schedule = await store.setup(loanId, amount, frequencySeconds, startDate);
          metrics.incCounter("qc_recurring_payments_setup_total");
          res.writeHead(201, { "content-type": "application/json" });
          res.end(JSON.stringify(schedule));
        })
        .catch(() => {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "invalid request body" }));
        });
      return;
    }

    if (req.method === "GET") {
      if (!store) {
        res.writeHead(503, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "payment store not configured" }));
        return;
      }
      void store.get(loanId).then((schedule) => {
        if (!schedule) {
          res.writeHead(404, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "no recurring payment schedule for this loan" }));
          return;
        }
        void store.successRateBps(loanId).then((rateBps) => {
          res.writeHead(200, { "content-type": "application/json" });
          res.end(JSON.stringify({ ...schedule, successRateBps: rateBps }));
        });
      });
      return;
    }

    // Early termination (issue #1168).
    if (req.method === "DELETE") {
      if (
        queueIfPartitioned(
          ctx,
          res,
          () => {
            void store?.terminate(loanId).then((terminated) => {
              if (terminated) {
                metrics.incCounter("qc_recurring_payments_terminated_total");
              }
            });
          },
          { loanId }
        )
      ) {
        return;
      }

      if (!store) {
        res.writeHead(503, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "payment store not configured" }));
        return;
      }

      void store.terminate(loanId).then((terminated) => {
        if (!terminated) {
          res.writeHead(404, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "no recurring payment schedule for this loan" }));
          return;
        }
        metrics.incCounter("qc_recurring_payments_terminated_total");
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify({ loanId, active: false }));
      });
      return;
    }
  }

  const recurringExecuteMatch = url.pathname.match(/^\/loans\/([^/]+)\/recurring-payment\/execute$/);
  if (recurringExecuteMatch && req.method === "POST") {
    const loanId = decodeURIComponent(recurringExecuteMatch[1] as string);

    // On-chain fund movement is inherently unsafe to defer blindly during a partition
    // (the whole point of a partition is we can't trust reaching the chain right now) —
    // so this endpoint queues the *retry attempt* itself rather than any assumed
    // outcome, and the replay re-runs the real retry-with-backoff path on recovery.
    if (
      queueIfPartitioned(
        ctx,
        res,
        () => {
          void executeRecurringPayment(loanId, ctx).then((result) => {
            metrics.incCounter(result.ok ? "qc_recurring_payments_success_total" : "qc_recurring_payments_failed_total");
          });
        },
        { loanId }
      )
    ) {
      return;
    }

    executeRecurringPayment(loanId, ctx)
      .then((result) => {
        metrics.incCounter(result.ok ? "qc_recurring_payments_success_total" : "qc_recurring_payments_failed_total");
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify(result));
      })
      .catch(() => {
        res.writeHead(500, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "recurring payment execution failed unexpectedly" }));
      });
    return;
  }

  // Loan request cart (batch loan requests, issue: cart system).
  if (url.pathname === "/cart") {
    if (req.method === "POST") {
      readJsonBody<CartRequestBody>(req)
        .then((body) => {
          if (!body.borrower) {
            res.writeHead(400, { "content-type": "application/json" });
            res.end(JSON.stringify({ error: "borrower required" }));
            return;
          }
          if (typeof body.amount !== "number" || !Number.isFinite(body.amount) || body.amount <= 0) {
            res.writeHead(400, { "content-type": "application/json" });
            res.end(JSON.stringify({ error: "amount must be a positive number" }));
            return;
          }
          if (typeof body.tenureSeconds !== "number" || body.tenureSeconds <= 0) {
            res.writeHead(400, { "content-type": "application/json" });
            res.end(JSON.stringify({ error: "tenureSeconds must be a positive number" }));
            return;
          }
          const cart = loanCartStore.addItem(body.borrower, body.amount, body.tenureSeconds);
          metrics.incCounter("qc_cart_items_added_total");
          res.writeHead(201, { "content-type": "application/json" });
          res.end(JSON.stringify(cart));
        })
        .catch(() => {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "invalid request body" }));
        });
      return;
    }

    if (req.method === "GET") {
      const borrower = url.searchParams.get("borrower");
      if (!borrower) {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "borrower query param required" }));
        return;
      }
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(loanCartStore.getCart(borrower)));
      return;
    }
  }

  if (url.pathname === "/cart/submit" && req.method === "POST") {
    readJsonBody<CartRequestBody>(req)
      .then((body) => {
        if (!body.borrower) {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "borrower required" }));
          return;
        }
        const results = loanCartStore.submitBatch(body.borrower);
        metrics.incCounter("qc_cart_batches_submitted_total");
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify({ borrower: body.borrower, results }));
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  if (url.pathname === "/cart/abandon" && req.method === "POST") {
    readJsonBody<CartRequestBody>(req)
      .then((body) => {
        if (!body.borrower) {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "borrower required" }));
          return;
        }
        const abandoned = loanCartStore.abandon(body.borrower);
        if (abandoned) {
          metrics.incCounter("qc_cart_abandoned_total");
        }
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify({ borrower: body.borrower, abandoned }));
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  if (url.pathname === "/cart/stats" && req.method === "GET") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(loanCartStore.getStats()));
    return;
  }

  if (req.method === "POST" && url.pathname === "/api/auth/token") {
    // Issue #1290: validate the submitted API key against the provisioned-keys store.
    // Unrecognised keys → 401.  Repeated failures per IP are rate-limited.
    // Issue #1374: the rate limiter check/record calls are async (Redis-backed in
    // multi-instance deployments — see auth/rateLimiter.ts), so this handler runs
    // as an async IIFE rather than the previous synchronous isBlocked() check.
    const keyStore = ctx.apiKeyStore ?? loadApiKeyStore();
    const rateLimiter = ctx.authRateLimiter ?? defaultAuthRateLimiter;
    const sourceIp =
      (req.headers["x-forwarded-for"] as string | undefined)?.split(",")[0]?.trim() ??
      req.socket.remoteAddress ??
      "unknown";

    void (async () => {
      if (await rateLimiter.isBlocked(sourceIp)) {
        res.writeHead(429, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "too many failed attempts — try again later" }));
        return;
      }

      let body: TokenRequestBody;
      try {
        body = await readJsonBody<TokenRequestBody>(req);
      } catch {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
        return;
      }

      if (!body.apiKey) {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "apiKey required" }));
        return;
      }

      if (!keyStore.isValid(body.apiKey)) {
        const blocked = await rateLimiter.recordFailure(sourceIp);
        metrics.incCounter("qc_auth_failures_total");
        if (blocked) {
          res.writeHead(429, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "too many failed attempts — try again later" }));
        } else {
          res.writeHead(401, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "invalid API key" }));
        }
        return;
      }

      const issued = issueToken(ctx.authSecret, body.apiKey, ctx.tokenTtlSeconds, body.borrower);
      metrics.incCounter("qc_auth_issued_total");
      res.writeHead(200, { "content-type": "application/json" });
      res.end(JSON.stringify(issued));
    })();
    return;
  }

  // Cost allocation reports (issue #1227)
  if (req.method === "GET" && url.pathname === "/costs/report") {
    if (!ctx.costAllocator) {
      res.writeHead(503, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "cost allocation is not configured on this instance" }));
      return;
    }
    const month = url.searchParams.get("month");
    const report = month ? ctx.costAllocator.monthlyReport(month) : ctx.costAllocator.currentReport();
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(report));
    return;
  }

  if (req.method === "GET" && url.pathname === "/costs/report/monthly") {
    if (!ctx.costAllocator) {
      res.writeHead(503, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "cost allocation is not configured on this instance" }));
      return;
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(ctx.costAllocator.generateMonthlyReports()));
    return;
  }

  // Partition status (issue #1229)
  if (req.method === "GET" && url.pathname === "/status/partition") {
    if (!ctx.partitionGuard) {
      res.writeHead(503, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "partition detection is not configured on this instance" }));
      return;
    }
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(ctx.partitionGuard.status()));
    return;
  }

  // ── Issue #1590, #1591: Credential verification proof export and identity verification ──

  // GET /credentials/:credentialId/proof - Get exportable proof
  const proofExportMatch = url.pathname.match(
    /^\/credentials\/([^/]+)\/proof$/
  );
  if (proofExportMatch && req.method === "GET") {
    const credentialId = decodeURIComponent(proofExportMatch[1] as string);

    const credential = credentialStore.getCredential(credentialId);
    if (!credential) {
      res.writeHead(404, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "credential not found" }));
      return;
    }

    const proof = generateProof(
      ctx.authSecret,
      credential.id,
      credential.holderId,
      credential.type,
      credential.issuer,
      credential.expiresAt,
      credential.metadata
    );

    metrics.incCounter("qc_proofs_generated_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(proof));
    return;
  }

  // POST /credentials/:credentialId/proof/export - Export proof in JSON format
  const proofExportJsonMatch = url.pathname.match(
    /^\/credentials\/([^/]+)\/proof\/export$/
  );
  if (proofExportJsonMatch && req.method === "POST") {
    const credentialId = decodeURIComponent(proofExportJsonMatch[1] as string);

    const credential = credentialStore.getCredential(credentialId);
    if (!credential) {
      res.writeHead(404, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "credential not found" }));
      return;
    }

    const proof = generateProof(
      ctx.authSecret,
      credential.id,
      credential.holderId,
      credential.type,
      credential.issuer,
      credential.expiresAt,
      credential.metadata
    );

    const exportedProof = exportProofJson(proof);

    metrics.incCounter("qc_proofs_exported_total");
    res.writeHead(200, {
      "content-type": "application/json",
      "content-disposition": `attachment; filename="proof-${credentialId}.json"`,
    });
    res.end(exportedProof);
    return;
  }

  // POST /credentials/:credentialId/proof/validate - Validate an exported proof
  const proofValidateMatch = url.pathname.match(
    /^\/credentials\/([^/]+)\/proof\/validate$/
  );
  if (proofValidateMatch && req.method === "POST") {
    readJsonBody<{ proofJson?: string }>(req)
      .then((body) => {
        if (!body.proofJson) {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "proofJson required" }));
          return;
        }

        const result = importProofJson(body.proofJson, ctx.authSecret);

        metrics.incCounter("qc_proofs_validated_total");
        res.writeHead(result.valid ? 200 : 400, {
          "content-type": "application/json",
        });
        res.end(
          JSON.stringify({
            valid: result.valid,
            reason: result.reason,
            proof: result.payload,
          })
        );
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  // POST /credentials/verify-identity - Start identity verification process
  if (req.method === "POST" && url.pathname === "/credentials/verify-identity") {
    readJsonBody<{
      credentialId?: string;
      holderId?: string;
      documentType?: string;
      metadata?: Record<string, unknown>;
    }>(req)
      .then((body) => {
        if (!body.credentialId || !body.holderId || !body.documentType) {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(
            JSON.stringify({
              error: "credentialId, holderId, and documentType required",
            })
          );
          return;
        }

        const credential = credentialStore.getCredential(body.credentialId);
        if (!credential) {
          res.writeHead(404, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "credential not found" }));
          return;
        }

        if (credential.holderId !== body.holderId) {
          res.writeHead(403, { "content-type": "application/json" });
          res.end(
            JSON.stringify({
              error: "holder ID does not match credential",
            })
          );
          return;
        }

        const verification = credentialStore.recordVerification(
          body.credentialId,
          body.holderId,
          body.documentType,
          credential.expiresAt
        );

        metrics.incCounter("qc_identity_verifications_initiated_total");
        res.writeHead(201, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            credentialId: body.credentialId,
            status: "pending",
            message: "identity verification initiated",
            verificationId: body.credentialId,
          })
        );
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  // GET /credentials/:credentialId/verification-status - Get verification status
  const verificationStatusMatch = url.pathname.match(
    /^\/credentials\/([^/]+)\/verification-status$/
  );
  if (verificationStatusMatch && req.method === "GET") {
    const credentialId = decodeURIComponent(verificationStatusMatch[1] as string);

    const verification = credentialStore.getVerification(credentialId);
    if (!verification) {
      res.writeHead(404, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "verification not found" }));
      return;
    }

    metrics.incCounter("qc_verification_status_checks_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(verification));
    return;
  }

  // GET /credentials/:holderId/verify-all - Bulk verification check
  const verifyAllMatch = url.pathname.match(/^\/credentials\/([^/]+)\/verify-all$/);
  if (verifyAllMatch && req.method === "GET") {
    const holderId = decodeURIComponent(verifyAllMatch[1] as string);

    const stats = credentialStore.getVerificationStats(holderId);
    const needsReVerification = credentialStore.getCredentialsNeedingReVerification(
      holderId
    );

    metrics.incCounter("qc_verification_stats_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(
      JSON.stringify({
        holderId,
        stats,
        credentialsNeedingReVerification: needsReVerification.map((c) => ({
          id: c.id,
          type: c.type,
          nextVerificationRequired: credentialStore
            .getVerification(c.id)
            ?.nextVerificationRequired,
        })),
      })
    );
    return;
  }

  // ── Issue #1591: Document Verification & Verification Challenges ──

  // POST /credentials/:credentialId/document-verification - Verify a document
  const documentVerifyMatch = url.pathname.match(
    /^\/credentials\/([^/]+)\/document-verification$/
  );
  if (documentVerifyMatch && req.method === "POST") {
    const credentialId = decodeURIComponent(documentVerifyMatch[1] as string);

    readJsonBody<{
      documentType?: string;
      documentHash?: string;
      expiresAt?: number;
      metadata?: Record<string, unknown>;
    }>(req)
      .then((body) => {
        const validDocumentTypes = ["passport", "driver_license", "national_id", "utility_bill", "bank_statement"];

        if (!body.documentType || !validDocumentTypes.includes(body.documentType)) {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(
            JSON.stringify({
              error: `documentType must be one of: ${validDocumentTypes.join(", ")}`,
            })
          );
          return;
        }

        if (!body.documentHash || typeof body.documentHash !== "string") {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "documentHash (string) required" }));
          return;
        }

        const credential = credentialStore.getCredential(credentialId);
        if (!credential) {
          res.writeHead(404, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "credential not found" }));
          return;
        }

        const expiresAt = body.expiresAt || credential.expiresAt;
        const verification = identityVerificationService.verifyDocument(
          credentialId,
          body.documentType as "passport" | "driver_license" | "national_id" | "utility_bill" | "bank_statement",
          body.documentHash,
          expiresAt,
          body.metadata
        );

        metrics.incCounter("qc_documents_verified_total");
        res.writeHead(201, { "content-type": "application/json" });
        res.end(JSON.stringify(verification));
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  // GET /credentials/:credentialId/documents - Get document verifications
  const documentsMatch = url.pathname.match(/^\/credentials\/([^/]+)\/documents$/);
  if (documentsMatch && req.method === "GET") {
    const credentialId = decodeURIComponent(documentsMatch[1] as string);

    const documents = identityVerificationService.getDocumentVerifications(
      credentialId
    );
    const isValid = identityVerificationService.isDocumentValid(credentialId);

    metrics.incCounter("qc_documents_queried_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ credentialId, documents, isValid }));
    return;
  }

  // POST /credentials/:credentialId/challenges - Create a verification challenge
  const challengeCreateMatch = url.pathname.match(
    /^\/credentials\/([^/]+)\/challenges$/
  );
  if (challengeCreateMatch && req.method === "POST") {
    const credentialId = decodeURIComponent(challengeCreateMatch[1] as string);

    readJsonBody<{
      holderId?: string;
      challengeType?: string;
      metadata?: Record<string, unknown>;
    }>(req)
      .then((body) => {
        if (!body.holderId || !body.challengeType) {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(
            JSON.stringify({
              error: "holderId and challengeType required",
            })
          );
          return;
        }

        const credential = credentialStore.getCredential(credentialId);
        if (!credential) {
          res.writeHead(404, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "credential not found" }));
          return;
        }

        const challenge = identityVerificationService.createChallenge(
          credentialId,
          body.holderId,
          body.challengeType as "face_match" | "document_liveness" | "manual_review",
          body.metadata
        );

        metrics.incCounter("qc_verification_challenges_created_total");
        res.writeHead(201, { "content-type": "application/json" });
        res.end(JSON.stringify(challenge));
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  // POST /credentials/:credentialId/challenges/:challengeId/complete - Complete a challenge
  const challengeCompleteMatch = url.pathname.match(
    /^\/credentials\/([^/]+)\/challenges\/([^/]+)\/complete$/
  );
  if (challengeCompleteMatch && req.method === "POST") {
    const challengeId = decodeURIComponent(challengeCompleteMatch[2] as string);

    readJsonBody<{ passed?: boolean; metadata?: Record<string, unknown> }>(req)
      .then((body) => {
        const passed = body.passed === true;
        const challenge = identityVerificationService.completeChallenge(
          challengeId,
          passed,
          body.metadata
        );

        if (!challenge) {
          res.writeHead(404, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "challenge not found" }));
          return;
        }

        metrics.incCounter(
          passed
            ? "qc_verification_challenges_passed_total"
            : "qc_verification_challenges_failed_total"
        );
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify(challenge));
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  // GET /credentials/:credentialId/verification-report - Get detailed verification report
  const reportMatch = url.pathname.match(
    /^\/credentials\/([^/]+)\/verification-report$/
  );
  if (reportMatch && req.method === "GET") {
    const credentialId = decodeURIComponent(reportMatch[1] as string);

    const report = identityVerificationService.getVerificationReport(
      credentialId
    );

    metrics.incCounter("qc_verification_reports_generated_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(report));
    return;
  }

  // ── Issue #1592: Credential Holder Analytics Dashboard ──

  // GET /analytics/credentials/:holderId - Get analytics dashboard for a holder
  const analyticsDashboardMatch = url.pathname.match(
    /^\/analytics\/credentials\/([^/]+)$/
  );
  if (analyticsDashboardMatch && req.method === "GET") {
    const holderId = decodeURIComponent(analyticsDashboardMatch[1] as string);

    const dashboardData = analyticsService.generateDashboardData(holderId);

    metrics.incCounter("qc_analytics_dashboard_generated_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(dashboardData));
    return;
  }

  // GET /analytics/credentials/:holderId/stats - Get credential statistics
  const analyticsStatsMatch = url.pathname.match(
    /^\/analytics\/credentials\/([^/]+)\/stats$/
  );
  if (analyticsStatsMatch && req.method === "GET") {
    const holderId = decodeURIComponent(analyticsStatsMatch[1] as string);

    const stats = {
      credentialStats: analyticsService.getCredentialStats(holderId),
      verificationStats: analyticsService.getVerificationStats(holderId),
      topTypes: analyticsService.getTopCredentialTypes(holderId),
      healthScore: analyticsService.calculateHealthScore(holderId),
    };

    metrics.incCounter("qc_analytics_stats_retrieved_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(stats));
    return;
  }

  // GET /analytics/credentials/:holderId/trends - Get trend data
  const analyticsTrendsMatch = url.pathname.match(
    /^\/analytics\/credentials\/([^/]+)\/trends$/
  );
  if (analyticsTrendsMatch && req.method === "GET") {
    const holderId = decodeURIComponent(analyticsTrendsMatch[1] as string);
    const days = parseInt(url.searchParams.get("days") || "30");

    const trends = analyticsService.getTrendData(holderId, days);

    metrics.incCounter("qc_analytics_trends_retrieved_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ holderId, days, trends }));
    return;
  }

  // POST /analytics/credentials/:holderId/record-trend - Record trend data
  const analyticsRecordMatch = url.pathname.match(
    /^\/analytics\/credentials\/([^/]+)\/record-trend$/
  );
  if (analyticsRecordMatch && req.method === "POST") {
    const holderId = decodeURIComponent(analyticsRecordMatch[1] as string);

    readJsonBody<{
      credentials?: number;
      verified?: number;
      pending?: number;
      avgVerificationScore?: number;
    }>(req)
      .then((body) => {
        if (
          typeof body.credentials !== "number" ||
          typeof body.verified !== "number" ||
          typeof body.pending !== "number"
        ) {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(
            JSON.stringify({
              error: "credentials, verified, pending (numbers) required",
            })
          );
          return;
        }

        analyticsService.recordTrend(holderId, {
          credentials: body.credentials,
          verified: body.verified,
          pending: body.pending,
          avgVerificationScore: body.avgVerificationScore || 0,
        });

        metrics.incCounter("qc_analytics_trend_recorded_total");
        res.writeHead(201, { "content-type": "application/json" });
        res.end(
          JSON.stringify({
            holderId,
            message: "trend recorded successfully",
          })
        );
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  // GET /analytics/metrics - Get system-wide analytics metrics
  if (req.method === "GET" && url.pathname === "/analytics/metrics") {
    const metrics_data = analyticsService.getUsageMetrics();

    metrics.incCounter("qc_analytics_metrics_retrieved_total");
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify(metrics_data));
    return;
  }

  // Webhook endpoints
  if (
    url.pathname.startsWith("/api/webhooks") ||
    url.pathname === "/webhook" ||
    url.pathname === "/webhooks/subscribe"
  ) {
    const webhookCtx: WebhookRoutesContext = {
      webhookSecret: ctx.webhookSecret,
      authSecret: ctx.authSecret,
      apiKeyStore: ctx.apiKeyStore,
    };
    handleWebhookRequest(req, res, webhookCtx);
    return;
  }

  // ── Issue #1292: Token revocation admin endpoints ─────────────────────────

  // POST /auth/revoke — revoke a specific JTI
  if (req.method === "POST" && url.pathname === "/auth/revoke") {
    if (!ctx.revocationStore) {
      res.writeHead(503, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "token revocation is not configured on this instance" }));
      return;
    }
    readJsonBody<{ jti?: string; expiresAt?: number }>(req)
      .then(async (body) => {
        if (!body.jti || typeof body.jti !== "string") {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "jti (string) required" }));
          return;
        }
        const expiresAt =
          typeof body.expiresAt === "number" ? body.expiresAt : Date.now() + ctx.tokenTtlSeconds * 1000;
        await ctx.revocationStore!.revoke(body.jti, expiresAt);
        metrics.incCounter("qc_tokens_revoked_total");
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify({ revoked: true, jti: body.jti }));
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  // POST /auth/revoke-subject — revoke all tokens for a subject (apiKey / borrower)
  if (req.method === "POST" && url.pathname === "/auth/revoke-subject") {
    if (!ctx.revocationStore) {
      res.writeHead(503, { "content-type": "application/json" });
      res.end(JSON.stringify({ error: "token revocation is not configured on this instance" }));
      return;
    }
    readJsonBody<{ subject?: string }>(req)
      .then(async (body) => {
        if (!body.subject || typeof body.subject !== "string") {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "subject (string) required" }));
          return;
        }
        await ctx.revocationStore!.revokeAllForSubject(body.subject);
        metrics.incCounter("qc_tokens_revoked_total");
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify({ revoked: true, subject: body.subject }));
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  // Not found
  res.writeHead(404, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: "not found" }));
}

export function readJsonBody<T>(req: IncomingMessage): Promise<T> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => {
      try {
        resolve(chunks.length > 0 ? JSON.parse(Buffer.concat(chunks).toString("utf8")) : {} as T);
      } catch (e) {
        reject(e);
      }
    });
    req.on("error", reject);
  });
}

/**
 * Executes a due recurring payment with retry-with-backoff and borrower
 * notification on exhaustion, per issue #1168.
 * Issue #1362: Now with persistent store and on-chain execution.
 */
function executeRecurringPayment(loanId: string, ctx: RouteContext) {
  const store = ctx.paymentStore;
  if (!store) {
    return Promise.resolve({ ok: false, retriesUsed: 0, notifiedBorrower: false });
  }
  return store.executeWithRetry(
    loanId,
    () => submitOnChainRecurringPayment(loanId, ctx),
    (id, schedule) => notifyBorrowerOfMissedPayment(id, schedule)
  );
}

/**
 * Issue #1362: Invoke execute_recurring_payment on the Soroban contract.
 * Returns whether the on-chain submission succeeded (contract executed without reverting).
 * The RPC client emits structured logs for audit trails.
 */
async function submitOnChainRecurringPayment(loanId: string, ctx: RouteContext): Promise<boolean> {
  const rpc = ctx.rpcClient;
  if (!rpc) {
    console.warn(`[quorum-credit] No RPC client configured for loan ${loanId}`);
    metrics.incCounter("qc_recurring_payments_rpc_unconfigured_total");
    return false;
  }

  try {
    // Emit an audit record for the attempt.
    metrics.incCounter("qc_recurring_payments_chain_attempts_total");

    // Invoke the contract via Soroban RPC with the borrower address.
    // The borrower address is derived from loanId (which is synthesized from event log).
    // For now, this is a placeholder since full chain wiring (issue #1322/#1356) is not yet available.
    // Once chain client is available, this will:
    // 1. Build a transaction with the RPC call
    // 2. Sign it with the keeper's keypair
    // 3. Submit to the network
    // 4. Poll for confirmation
    const result = await rpc.executeRecurringPayment(loanId);

    if (result.ok) {
      metrics.incCounter("qc_recurring_payments_chain_success_total");
      console.log(
        `[quorum-credit] execute_recurring_payment succeeded for loan=${loanId} tx=${result.txHash}`
      );
      return true;
    } else {
      metrics.incCounter("qc_recurring_payments_chain_failed_total");
      console.warn(
        `[quorum-credit] execute_recurring_payment failed for loan=${loanId}: ${result.error}`
      );
      return false;
    }
  } catch (err) {
    const error = err instanceof Error ? err.message : String(err);
    console.error(`[quorum-credit] submitOnChainRecurringPayment crashed for loan=${loanId}: ${error}`);
    metrics.incCounter("qc_recurring_payments_chain_errors_total");
    return false;
  }
}

function notifyBorrowerOfMissedPayment(
  loanId: string,
  schedule?: { failureCount?: number; retryCount?: number }
): void {
  const failCount = schedule?.failureCount ?? 0;
  const retryCount = schedule?.retryCount ?? 0;
  console.warn(
    `[quorum-credit] recurring payment for loan ${loanId} failed after retries (failures=${failCount}, last_retry_count=${retryCount}); notifying borrower`
  );
  metrics.incCounter("qc_recurring_payments_notifications_total");
}
