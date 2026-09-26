/**
 * Issue #1745: Deprecation warning headers for API endpoints.
 *
 * Deprecated endpoints are registered with a deprecation date, an optional sunset
 * date and a replacement. Responses from those endpoints carry:
 *   Deprecation: @<unix-seconds>            (draft-ietf-httpapi-deprecation-header)
 *   Sunset: <HTTP-date>                     (RFC 8594)
 *   Link: <replacement>; rel="successor-version", <docs>; rel="deprecation"
 *   Warning: 299 - "<message>"
 * Every hit is counted per endpoint and per client, exposed via metrics and
 * GET /api/v1/deprecations, and alerts are raised (throttled) when a deprecated
 * endpoint is used — escalating once the sunset date is near or past.
 */

import type { IncomingMessage, ServerResponse } from "node:http";
import { metrics } from "./metricsRegistry.js";

export interface DeprecatedEndpoint {
  /** Stable identifier used in metrics and the usage report. */
  id: string;
  method?: string;
  /** Exact path or a RegExp matched against the request pathname. */
  path: string | RegExp;
  deprecatedAt: Date;
  sunsetAt?: Date;
  replacement?: string;
  docsUrl?: string;
  message?: string;
}

export interface DeprecationUsage {
  id: string;
  hits: number;
  firstSeenAt?: number;
  lastSeenAt?: number;
  clients: Record<string, number>;
}

export type DeprecationAlertLevel = "info" | "warning" | "critical";

export interface DeprecationAlert {
  level: DeprecationAlertLevel;
  endpointId: string;
  client: string;
  hits: number;
  sunsetAt?: string;
  message: string;
}

export type DeprecationAlertHandler = (alert: DeprecationAlert) => void;

const SUNSET_WARNING_WINDOW_MS = 30 * 24 * 60 * 60 * 1000;
const MAX_TRACKED_CLIENTS = 1000;

export class DeprecationRegistry {
  private readonly endpoints: DeprecatedEndpoint[] = [];
  private readonly usage = new Map<string, DeprecationUsage>();
  private readonly lastAlertAt = new Map<string, number>();
  private readonly alertHandlers = new Set<DeprecationAlertHandler>();

  constructor(private readonly alertThrottleMs = 60 * 60 * 1000) {}

  register(endpoint: DeprecatedEndpoint): void {
    this.endpoints.push(endpoint);
    this.usage.set(endpoint.id, { id: endpoint.id, hits: 0, clients: {} });
  }

  onAlert(handler: DeprecationAlertHandler): () => void {
    this.alertHandlers.add(handler);
    return () => this.alertHandlers.delete(handler);
  }

  match(method: string | undefined, pathname: string): DeprecatedEndpoint | undefined {
    return this.endpoints.find(
      (e) =>
        (!e.method || e.method === method) &&
        (typeof e.path === "string" ? e.path === pathname : e.path.test(pathname))
    );
  }

  /** Builds the response headers for a deprecated endpoint. */
  headersFor(endpoint: DeprecatedEndpoint): Record<string, string> {
    const headers: Record<string, string> = {
      deprecation: `@${Math.floor(endpoint.deprecatedAt.getTime() / 1000)}`,
    };
    if (endpoint.sunsetAt) headers["sunset"] = endpoint.sunsetAt.toUTCString();

    const links: string[] = [];
    if (endpoint.replacement) links.push(`<${endpoint.replacement}>; rel="successor-version"`);
    if (endpoint.docsUrl) links.push(`<${endpoint.docsUrl}>; rel="deprecation"; type="text/html"`);
    if (links.length > 0) headers["link"] = links.join(", ");

    headers["warning"] = `299 - "${warningText(endpoint).replace(/"/g, "'")}"`;
    return headers;
  }

  /** Records a hit and raises a (throttled) alert. */
  track(endpoint: DeprecatedEndpoint, client: string): void {
    const now = Date.now();
    const usage = this.usage.get(endpoint.id) ?? { id: endpoint.id, hits: 0, clients: {} };
    usage.hits++;
    usage.firstSeenAt ??= now;
    usage.lastSeenAt = now;
    if (client in usage.clients || Object.keys(usage.clients).length < MAX_TRACKED_CLIENTS) {
      usage.clients[client] = (usage.clients[client] ?? 0) + 1;
    }
    this.usage.set(endpoint.id, usage);

    metrics.incLabeledCounter("qc_deprecated_endpoint_requests_total", "endpoint", endpoint.id);

    const alertKey = `${endpoint.id}\n${client}`;
    const last = this.lastAlertAt.get(alertKey) ?? 0;
    if (now - last < this.alertThrottleMs) return;
    this.lastAlertAt.set(alertKey, now);

    const alert: DeprecationAlert = {
      level: alertLevel(endpoint, now),
      endpointId: endpoint.id,
      client,
      hits: usage.hits,
      sunsetAt: endpoint.sunsetAt?.toISOString(),
      message: `deprecated endpoint ${endpoint.id} used by ${client}: ${warningText(endpoint)}`,
    };
    metrics.incLabeledCounter("qc_deprecation_alerts_total", "level", alert.level);

    if (this.alertHandlers.size === 0) {
      const log = alert.level === "info" ? console.info : console.warn;
      log(`[deprecation] ${alert.level}: ${alert.message}`);
    }
    for (const handler of this.alertHandlers) {
      try {
        handler(alert);
      } catch (err) {
        console.error("[deprecation] alert handler failed", err);
      }
    }
  }

  report(): Array<{
    id: string;
    method?: string;
    path: string;
    deprecatedAt: string;
    sunsetAt?: string;
    sunsetPassed: boolean;
    replacement?: string;
    usage: DeprecationUsage;
  }> {
    const now = Date.now();
    return this.endpoints.map((e) => ({
      id: e.id,
      method: e.method,
      path: typeof e.path === "string" ? e.path : e.path.source,
      deprecatedAt: e.deprecatedAt.toISOString(),
      sunsetAt: e.sunsetAt?.toISOString(),
      sunsetPassed: e.sunsetAt ? e.sunsetAt.getTime() <= now : false,
      replacement: e.replacement,
      usage: this.usage.get(e.id) ?? { id: e.id, hits: 0, clients: {} },
    }));
  }
}

function warningText(endpoint: DeprecatedEndpoint): string {
  if (endpoint.message) return endpoint.message;
  let text = `${endpoint.method ?? "*"} ${typeof endpoint.path === "string" ? endpoint.path : endpoint.id} is deprecated`;
  if (endpoint.sunsetAt) text += ` and will be removed after ${endpoint.sunsetAt.toISOString().slice(0, 10)}`;
  if (endpoint.replacement) text += `; use ${endpoint.replacement} instead`;
  return text;
}

function alertLevel(endpoint: DeprecatedEndpoint, now: number): DeprecationAlertLevel {
  if (!endpoint.sunsetAt) return "info";
  const remaining = endpoint.sunsetAt.getTime() - now;
  if (remaining <= 0) return "critical";
  if (remaining <= SUNSET_WARNING_WINDOW_MS) return "warning";
  return "info";
}

function clientIdentifier(req: IncomingMessage): string {
  const apiKey = req.headers["x-api-key"];
  if (typeof apiKey === "string" && apiKey) return `key:${apiKey.slice(0, 8)}`;
  const forwarded = req.headers["x-forwarded-for"];
  const ip = (typeof forwarded === "string" ? forwarded.split(",")[0]?.trim() : undefined) ??
    req.socket.remoteAddress ??
    "unknown";
  return `ip:${ip}`;
}

export const deprecationRegistry = new DeprecationRegistry();

// The bulk verify-all check is superseded by the analytics stats endpoint.
deprecationRegistry.register({
  id: "credentials-verify-all",
  method: "GET",
  path: /^\/credentials\/[^/]+\/verify-all$/,
  deprecatedAt: new Date("2026-09-26T00:00:00Z"),
  sunsetAt: new Date("2027-03-31T00:00:00Z"),
  replacement: "/analytics/credentials/{holderId}/stats",
});

/**
 * Applies deprecation headers and usage tracking for the current request. Headers are
 * set with setHeader() before the route handler runs so they merge with whatever the
 * handler passes to writeHead().
 */
export function applyDeprecationHeaders(
  req: IncomingMessage,
  res: ServerResponse,
  registry: DeprecationRegistry = deprecationRegistry
): void {
  const url = new URL(req.url ?? "", "http://internal");
  const endpoint = registry.match(req.method, url.pathname);
  if (!endpoint) return;
  for (const [name, value] of Object.entries(registry.headersFor(endpoint))) {
    res.setHeader(name, value);
  }
  registry.track(endpoint, clientIdentifier(req));
}
