import type { IncomingMessage, ServerResponse } from "node:http";
import { issueToken } from "../auth/tokens.js";
import { metrics } from "./metricsRegistry.js";

export interface RouteContext {
  authSecret: string;
  tokenTtlSeconds: number;
}

interface TokenRequestBody {
  apiKey?: string;
  borrower?: string;
}

/** Minimal router for the handful of REST endpoints this service exposes — not
 * pulling in Express for three routes. */
export function handleHttpRequest(
  req: IncomingMessage,
  res: ServerResponse,
  ctx: RouteContext
): void {
  const url = new URL(req.url ?? "", "http://internal");

  if (req.method === "GET" && url.pathname === "/health") {
    res.writeHead(200, { "content-type": "application/json" });
    res.end(JSON.stringify({ status: "ok" }));
    return;
  }

  if (req.method === "GET" && url.pathname === "/metrics") {
    res.writeHead(200, { "content-type": "text/plain; version=0.0.4" });
    res.end(metrics.toPrometheusText());
    return;
  }

  if (req.method === "POST" && url.pathname === "/api/auth/token") {
    readJsonBody(req)
      .then((body: TokenRequestBody) => {
        // NOTE: this issues a token to anyone who asks with any apiKey string — real
        // deployments must swap in a genuine credential check (e.g. verifying apiKey
        // against a provisioned-keys store) before going to production. Wiring that
        // check is intentionally left as a single, obvious seam (this block) rather
        // than left implicit, since this repo has no existing API-key store to
        // integrate against.
        if (!body.apiKey) {
          res.writeHead(400, { "content-type": "application/json" });
          res.end(JSON.stringify({ error: "apiKey required" }));
          return;
        }
        const issued = issueToken(ctx.authSecret, body.apiKey, ctx.tokenTtlSeconds, body.borrower);
        res.writeHead(200, { "content-type": "application/json" });
        res.end(JSON.stringify(issued));
      })
      .catch(() => {
        res.writeHead(400, { "content-type": "application/json" });
        res.end(JSON.stringify({ error: "invalid request body" }));
      });
    return;
  }

  res.writeHead(404, { "content-type": "application/json" });
  res.end(JSON.stringify({ error: "not found" }));
}

function readJsonBody(req: IncomingMessage): Promise<TokenRequestBody> {
  return new Promise((resolve, reject) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk) => chunks.push(chunk));
    req.on("end", () => {
      try {
        resolve(chunks.length > 0 ? JSON.parse(Buffer.concat(chunks).toString("utf8")) : {});
      } catch (e) {
        reject(e);
      }
    });
    req.on("error", reject);
  });
}
