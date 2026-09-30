/**
 * Issue #1751: Multi-Region API Availability — Cross-Region Replication & Health Check
 *
 * Provides:
 *   1. CrossRegionReplicator — fans out write operations to peer regions.
 *   2. RegionHealthChecker   — periodically probes each region's /health endpoint
 *      and updates the RegionRegistry accordingly.
 *   3. RegionAwareRouter     — middleware helper that selects the best region for
 *      a request and can proxy to it or return routing metadata.
 */

import type { IncomingMessage, ServerResponse } from "node:http";
import { request as httpRequest } from "node:http";
import { request as httpsRequest } from "node:https";
import { URL } from "node:url";
import type { RegionConfig } from "./regionConfig.js";
import { regionRegistry } from "./regionConfig.js";
import { metrics } from "./metricsRegistry.js";

// ---------------------------------------------------------------------------
// Cross-Region Replication
// ---------------------------------------------------------------------------

export interface ReplicationResult {
  regionId: string;
  success: boolean;
  statusCode?: number;
  durationMs: number;
  error?: string;
}

export interface ReplicationOptions {
  /** HTTP method.  Default: "POST". */
  method?: string;
  /** Request path (e.g. "/api/v1/credentials/issue"). */
  path: string;
  /** JSON-serialisable request body. */
  body: unknown;
  /** Additional headers to forward. */
  headers?: Record<string, string>;
  /** Per-region request timeout (ms).  Default: 10_000. */
  timeoutMs?: number;
}

/**
 * Fans out a write operation to all active peer regions in parallel.
 * Errors from individual peers are caught and returned as failed results
 * so a single peer failure does not abort the entire fan-out.
 */
export class CrossRegionReplicator {
  /**
   * @param localRegionId - ID of the region this instance is running in.
   *   Peer regions are all active regions EXCEPT the local one.
   */
  constructor(
    private readonly localRegionId: string = process.env["REGION_ID"] ?? "local"
  ) {}

  /**
   * Replicate a write to all currently active peer regions in parallel.
   *
   * @returns Array of per-region results (one per peer, never throws).
   */
  async replicateToAll(opts: ReplicationOptions): Promise<ReplicationResult[]> {
    const peers = regionRegistry
      .getActiveRegions()
      .filter((r) => r.id !== this.localRegionId);

    if (peers.length === 0) return [];

    const tasks = peers.map((peer) => this.replicateToPeer(peer, opts));
    const results = await Promise.allSettled(tasks);

    return results.map((r, i) => {
      if (r.status === "fulfilled") return r.value;
      return {
        regionId: peers[i]!.id,
        success: false,
        durationMs: 0,
        error: r.reason instanceof Error ? r.reason.message : String(r.reason),
      };
    });
  }

  /**
   * Replicate a write to a single peer region.
   */
  async replicateToPeer(
    peer: RegionConfig,
    opts: ReplicationOptions
  ): Promise<ReplicationResult> {
    const start = Date.now();
    const timeoutMs = opts.timeoutMs ?? 10_000;
    const method = opts.method ?? "POST";

    try {
      const statusCode = await this._sendRequest(
        peer.apiBaseUrl,
        opts.path,
        method,
        opts.body,
        opts.headers ?? {},
        timeoutMs
      );
      const durationMs = Date.now() - start;
      const success = statusCode >= 200 && statusCode < 300;

      metrics.incLabeledCounter(
        success
          ? "qc_cross_region_replication_success_total"
          : "qc_cross_region_replication_failure_total",
        "region",
        peer.id
      );

      return { regionId: peer.id, success, statusCode, durationMs };
    } catch (err) {
      const durationMs = Date.now() - start;
      metrics.incLabeledCounter(
        "qc_cross_region_replication_error_total",
        "region",
        peer.id
      );
      return {
        regionId: peer.id,
        success: false,
        durationMs,
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  // ---------------------------------------------------------------------------
  // Private HTTP helper
  // ---------------------------------------------------------------------------

  private _sendRequest(
    baseUrl: string,
    path: string,
    method: string,
    body: unknown,
    headers: Record<string, string>,
    timeoutMs: number
  ): Promise<number> {
    return new Promise((resolve, reject) => {
      const url = new URL(path, baseUrl);
      const payload = JSON.stringify(body);

      const reqHeaders: Record<string, string> = {
        "content-type": "application/json",
        "content-length": Buffer.byteLength(payload).toString(),
        "x-replication-source": this.localRegionId,
        ...headers,
      };

      const lib = url.protocol === "https:" ? httpsRequest : httpRequest;
      const req = lib(
        {
          hostname: url.hostname,
          port: url.port || (url.protocol === "https:" ? 443 : 80),
          path: url.pathname + url.search,
          method,
          headers: reqHeaders,
          timeout: timeoutMs,
        },
        (res) => {
          // Drain the response to free the socket.
          res.resume();
          res.on("end", () => resolve(res.statusCode ?? 0));
        }
      );

      req.on("timeout", () => {
        req.destroy(new Error(`Replication request to ${baseUrl}${path} timed out after ${timeoutMs}ms`));
      });
      req.on("error", reject);
      req.write(payload);
      req.end();
    });
  }
}

// ---------------------------------------------------------------------------
// Region Health Checker
// ---------------------------------------------------------------------------

/**
 * Periodically probes each registered region's health endpoint and updates
 * the RegionRegistry with the results.
 */
export class RegionHealthChecker {
  private timer: ReturnType<typeof setInterval> | null = null;
  private running = false;

  constructor(
    private readonly intervalMs = 30_000,
    private readonly localRegionId: string = process.env["REGION_ID"] ?? "local"
  ) {}

  start(): void {
    if (this.running) return;
    this.running = true;
    // Run an immediate check, then on the interval.
    void this._checkAll();
    this.timer = setInterval(() => void this._checkAll(), this.intervalMs);
    console.info(
      `[regionHealthChecker] Started (interval=${this.intervalMs}ms).`
    );
  }

  stop(): void {
    if (!this.running) return;
    this.running = false;
    if (this.timer !== null) {
      clearInterval(this.timer);
      this.timer = null;
    }
    console.info("[regionHealthChecker] Stopped.");
  }

  /** Run a single health check pass on all regions. */
  async checkAllNow(): Promise<void> {
    return this._checkAll();
  }

  // ---------------------------------------------------------------------------
  // Private
  // ---------------------------------------------------------------------------

  private async _checkAll(): Promise<void> {
    const allRegions = regionRegistry.getAllRegions();
    // Check all regions including local to keep health data fresh.
    await Promise.allSettled(
      allRegions.map((r) => this._checkOne(r))
    );
  }

  private async _checkOne(
    region: RegionConfig & { health: unknown }
  ): Promise<void> {
    const checkPath = region.healthCheckPath ?? "/health";
    const timeoutMs = region.healthCheckTimeoutMs ?? 5000;
    const start = Date.now();

    try {
      const statusCode = await this._probe(region.apiBaseUrl, checkPath, timeoutMs);
      const latencyMs = Date.now() - start;

      if (statusCode >= 200 && statusCode < 300) {
        regionRegistry.recordSuccess(region.id, latencyMs);
      } else {
        regionRegistry.recordFailure(
          region.id,
          `Health check returned HTTP ${statusCode}`
        );
      }
    } catch (err) {
      regionRegistry.recordFailure(
        region.id,
        err instanceof Error ? err.message : String(err)
      );
    }
  }

  private _probe(baseUrl: string, path: string, timeoutMs: number): Promise<number> {
    return new Promise((resolve, reject) => {
      try {
        const url = new URL(path, baseUrl);
        const lib = url.protocol === "https:" ? httpsRequest : httpRequest;
        const req = lib(
          {
            hostname: url.hostname,
            port: url.port || (url.protocol === "https:" ? 443 : 80),
            path: url.pathname,
            method: "GET",
            timeout: timeoutMs,
          },
          (res) => {
            res.resume();
            res.on("end", () => resolve(res.statusCode ?? 0));
          }
        );
        req.on("timeout", () => {
          req.destroy(new Error(`Health probe to ${baseUrl}${path} timed out`));
        });
        req.on("error", reject);
        req.end();
      } catch (err) {
        reject(err);
      }
    });
  }
}

// ---------------------------------------------------------------------------
// Region-Aware Router Middleware
// ---------------------------------------------------------------------------

/**
 * Extracts region routing metadata from a request and attaches it to the
 * response headers.  Also provides a `proxyToRegion` helper for transparent
 * cross-region forwarding.
 */
export class RegionAwareRouter {
  private readonly replicator: CrossRegionReplicator;

  constructor(
    private readonly localRegionId: string = process.env["REGION_ID"] ?? "local"
  ) {
    this.replicator = new CrossRegionReplicator(localRegionId);
  }

  /**
   * Middleware: attach region-routing headers to every response.
   *
   * Sets:
   *   X-Served-By-Region: <regionId>
   *   X-Region-Strategy:  <strategy>
   *   X-Failover-Available: true|false
   */
  attachRegionHeaders(_req: IncomingMessage, res: ServerResponse): void {
    const localConfig = regionRegistry.getRegion(this.localRegionId);
    res.setHeader("x-served-by-region", this.localRegionId);

    const summary = regionRegistry.healthSummary();
    const activeCount = summary.filter((r) => r.status === "active").length;
    res.setHeader("x-active-regions", String(activeCount));

    const failover = localConfig
      ? regionRegistry.resolveFailover(this.localRegionId)
      : undefined;
    res.setHeader("x-failover-available", failover ? "true" : "false");
    if (failover) {
      res.setHeader("x-failover-region", failover.id);
    }
  }

  /**
   * Determine whether this request should be forwarded to another region.
   *
   * Returns a redirect base URL when the local region is unhealthy and a
   * healthy failover region exists; otherwise returns `null`.
   */
  shouldRedirect(): string | null {
    const localHealth = regionRegistry.getHealth(this.localRegionId);
    if (
      !localHealth ||
      localHealth.status === "active" ||
      localHealth.status === "degraded"
    ) {
      // Still serving — don't redirect.
      return null;
    }

    const failover = regionRegistry.resolveFailover(this.localRegionId);
    if (!failover) return null;

    console.warn(
      `[regionAwareRouter] Local region "${this.localRegionId}" is ${localHealth.status}; ` +
        `redirecting to "${failover.id}" (${failover.apiBaseUrl}).`
    );
    metrics.incLabeledCounter("qc_region_failovers_total", "from", this.localRegionId);
    return failover.apiBaseUrl;
  }

  /**
   * Fan out a write to all peer regions asynchronously (fire-and-forget with
   * result logging).
   */
  replicateWrite(opts: ReplicationOptions): void {
    this.replicator.replicateToAll(opts).then((results) => {
      for (const r of results) {
        if (!r.success) {
          console.warn(
            `[regionAwareRouter] Replication to "${r.regionId}" failed: ${r.error ?? `HTTP ${r.statusCode}`}`
          );
        }
      }
    }).catch((err) => {
      console.error("[regionAwareRouter] Unexpected replication error:", err);
    });
  }
}

// ---------------------------------------------------------------------------
// Singletons
// ---------------------------------------------------------------------------

export const crossRegionReplicator = new CrossRegionReplicator();
export const regionHealthChecker = new RegionHealthChecker();
export const regionAwareRouter = new RegionAwareRouter();
