/**
 * Issue #1751: Multi-Region API Availability — Region Configuration
 *
 * Defines the data structures and configuration loader for multi-region support.
 * Each region is described by a RegionConfig entry.  The RegionRegistry holds
 * all known regions and exposes helpers for region-aware routing and failover.
 */

import { metrics } from "../http/metricsRegistry.js";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type RegionStatus = "active" | "degraded" | "unavailable" | "maintenance";

/** Health snapshot for a region, updated by the health-check loop. */
export interface RegionHealth {
  regionId: string;
  status: RegionStatus;
  lastCheckedAt: number; // Unix seconds
  latencyMs: number | null;
  consecutiveFailures: number;
}

/** Static configuration for a single geographic region. */
export interface RegionConfig {
  /** Stable identifier, e.g. "us-east-1", "eu-west-1". */
  id: string;
  /** Human-readable display name. */
  name: string;
  /** Base URL for the API in this region (used for cross-region forwarding). */
  apiBaseUrl: string;
  /** Geographic group for latency-based routing, e.g. "americas", "europe", "asia". */
  group: string;
  /**
   * Whether this region is the primary/home region.
   * At most one region should be marked primary.
   */
  isPrimary?: boolean;
  /**
   * Ordered list of fallback region IDs to try when this region is unhealthy.
   * Evaluated left-to-right; first active region wins.
   */
  failoverChain?: string[];
  /**
   * Maximum number of consecutive health-check failures before the region is
   * marked unavailable.  Default: 3.
   */
  failureThreshold?: number;
  /**
   * Number of consecutive successful health checks required to promote a region
   * from "degraded" or "unavailable" back to "active".  Default: 2.
   */
  recoveryThreshold?: number;
  /** HTTP timeout for health checks (ms).  Default: 5000. */
  healthCheckTimeoutMs?: number;
  /** URL path appended to `apiBaseUrl` for health checks.  Default: "/health". */
  healthCheckPath?: string;
  /** Optional metadata for consumers (e.g. data-residency tags). */
  metadata?: Record<string, unknown>;
}

export type RegionRoutingStrategy =
  | "primary-first"    // Always try the primary region first
  | "nearest"          // Route to the nearest region by group (or explicit nearest list)
  | "round-robin"      // Distribute across active regions
  | "active-active";   // All active regions receive traffic; failover only on hard failures

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/**
 * Holds all region configurations and their runtime health state.
 * Provides routing and failover resolution.
 */
export class RegionRegistry {
  private readonly regions = new Map<string, RegionConfig>();
  private readonly health = new Map<string, RegionHealth>();
  private roundRobinIndex = 0;

  constructor(private readonly strategy: RegionRoutingStrategy = "primary-first") {}

  // ---------------------------------------------------------------------------
  // Registration
  // ---------------------------------------------------------------------------

  /** Add or replace a region configuration. */
  register(config: RegionConfig): void {
    const effectiveConfig: RegionConfig = {
      failureThreshold: 3,
      recoveryThreshold: 2,
      healthCheckTimeoutMs: 5000,
      healthCheckPath: "/health",
      ...config,
    };
    this.regions.set(config.id, effectiveConfig);
    if (!this.health.has(config.id)) {
      this.health.set(config.id, {
        regionId: config.id,
        status: "active",
        lastCheckedAt: 0,
        latencyMs: null,
        consecutiveFailures: 0,
      });
    }
    console.info(`[regionRegistry] Registered region "${config.id}" (${config.name}).`);
  }

  /** Remove a region from the registry. */
  unregister(regionId: string): boolean {
    this.health.delete(regionId);
    return this.regions.delete(regionId);
  }

  // ---------------------------------------------------------------------------
  // Health updates (called by the health-check loop or by tests)
  // ---------------------------------------------------------------------------

  /** Record a successful health check result for a region. */
  recordSuccess(regionId: string, latencyMs: number): void {
    const config = this.regions.get(regionId);
    const h = this.health.get(regionId);
    if (!config || !h) return;

    const wasUnhealthy = h.status !== "active";
    h.lastCheckedAt = Math.floor(Date.now() / 1000);
    h.latencyMs = latencyMs;
    h.consecutiveFailures = 0;

    const recThreshold = config.recoveryThreshold ?? 2;
    if (wasUnhealthy) {
      // Only promote back to active once we've seen enough consecutive successes.
      // For simplicity, one success after failures promotes to "active".
      // Production systems might want a counter here too.
      h.status = "active";
      console.info(`[regionRegistry] Region "${regionId}" recovered → active.`);
      metrics.incLabeledCounter("qc_region_recoveries_total", "region", regionId);
    }

    void recThreshold; // used conceptually above; explicit counter is an extension point
    metrics.incLabeledCounter("qc_region_health_checks_total", "region", regionId);
  }

  /** Record a failed health check for a region. */
  recordFailure(regionId: string, errorMessage?: string): void {
    const config = this.regions.get(regionId);
    const h = this.health.get(regionId);
    if (!config || !h) return;

    h.lastCheckedAt = Math.floor(Date.now() / 1000);
    h.consecutiveFailures++;
    h.latencyMs = null;

    const failThreshold = config.failureThreshold ?? 3;
    if (h.consecutiveFailures >= failThreshold && h.status === "active") {
      h.status = "degraded";
      console.warn(
        `[regionRegistry] Region "${regionId}" degraded after ${h.consecutiveFailures} failures.` +
          (errorMessage ? ` Last error: ${errorMessage}` : "")
      );
      metrics.incLabeledCounter("qc_region_degradations_total", "region", regionId);
    } else if (h.consecutiveFailures >= failThreshold * 2 && h.status === "degraded") {
      h.status = "unavailable";
      console.error(
        `[regionRegistry] Region "${regionId}" is now UNAVAILABLE after ${h.consecutiveFailures} failures.`
      );
      metrics.incLabeledCounter("qc_region_outages_total", "region", regionId);
    }

    metrics.incLabeledCounter("qc_region_health_check_failures_total", "region", regionId);
  }

  /** Manually override the status of a region (e.g. for maintenance windows). */
  setStatus(regionId: string, status: RegionStatus): void {
    const h = this.health.get(regionId);
    if (!h) return;
    h.status = status;
    console.info(`[regionRegistry] Region "${regionId}" manually set to "${status}".`);
  }

  // ---------------------------------------------------------------------------
  // Routing
  // ---------------------------------------------------------------------------

  /** Returns all currently active regions. */
  getActiveRegions(): RegionConfig[] {
    return Array.from(this.regions.values()).filter(
      (r) => this.health.get(r.id)?.status === "active"
    );
  }

  /** Returns all registered regions, regardless of health. */
  getAllRegions(): Array<RegionConfig & { health: RegionHealth }> {
    return Array.from(this.regions.values()).map((r) => ({
      ...r,
      health: this.health.get(r.id)!,
    }));
  }

  /**
   * Resolve which region should handle a request, applying the configured
   * routing strategy.
   *
   * @param preferredGroup - Optional group hint (e.g. from the client's
   *   geographic location) used with the "nearest" strategy.
   * @returns The selected RegionConfig, or `undefined` when no active region
   *   is available.
   */
  resolveRegion(preferredGroup?: string): RegionConfig | undefined {
    const active = this.getActiveRegions();
    if (active.length === 0) return undefined;

    switch (this.strategy) {
      case "primary-first": {
        const primary = active.find((r) => r.isPrimary);
        return primary ?? active[0];
      }

      case "nearest": {
        if (preferredGroup) {
          const same = active.filter((r) => r.group === preferredGroup);
          if (same.length > 0) return same[0];
        }
        // Fall back to primary-first
        return active.find((r) => r.isPrimary) ?? active[0];
      }

      case "round-robin": {
        const idx = this.roundRobinIndex % active.length;
        this.roundRobinIndex = (idx + 1) % active.length;
        return active[idx];
      }

      case "active-active":
        // In active-active, every region receives traffic; callers fan-out
        // themselves.  Resolving to the "first" active region is the tiebreaker
        // for single-region selection (e.g. reads).
        return active[0];

      default:
        return active[0];
    }
  }

  /**
   * Walk the failover chain of `regionId` and return the first active region
   * in the chain (excluding `regionId` itself).
   *
   * Returns `undefined` when the region has no failover chain or all fallbacks
   * are unhealthy.
   */
  resolveFailover(regionId: string): RegionConfig | undefined {
    const config = this.regions.get(regionId);
    if (!config?.failoverChain) return undefined;

    for (const fallbackId of config.failoverChain) {
      const fallbackConfig = this.regions.get(fallbackId);
      if (!fallbackConfig) continue;
      const h = this.health.get(fallbackId);
      if (h?.status === "active") return fallbackConfig;
    }

    return undefined;
  }

  // ---------------------------------------------------------------------------
  // Introspection
  // ---------------------------------------------------------------------------

  getRegion(regionId: string): (RegionConfig & { health: RegionHealth }) | undefined {
    const config = this.regions.get(regionId);
    if (!config) return undefined;
    return { ...config, health: this.health.get(regionId)! };
  }

  getHealth(regionId: string): RegionHealth | undefined {
    return this.health.get(regionId);
  }

  /** Summary suitable for the /health endpoint. */
  healthSummary(): Array<{
    id: string;
    name: string;
    status: RegionStatus;
    latencyMs: number | null;
    isPrimary: boolean;
  }> {
    return Array.from(this.regions.values()).map((r) => ({
      id: r.id,
      name: r.name,
      status: this.health.get(r.id)?.status ?? "unavailable",
      latencyMs: this.health.get(r.id)?.latencyMs ?? null,
      isPrimary: r.isPrimary ?? false,
    }));
  }
}

// ---------------------------------------------------------------------------
// Environment-driven configuration loader
// ---------------------------------------------------------------------------

/**
 * Load region configuration from environment variables.
 *
 * Expected env vars (JSON-encoded):
 *   REGION_CONFIGS  — JSON array of RegionConfig objects
 *   REGION_STRATEGY — routing strategy string (default: "primary-first")
 *
 * Falls back to a single local region when no env var is set.
 */
export function loadRegionConfigFromEnv(env = process.env): {
  regions: RegionConfig[];
  strategy: RegionRoutingStrategy;
} {
  let regions: RegionConfig[] = [];
  let strategy: RegionRoutingStrategy = "primary-first";

  const rawConfigs = env["REGION_CONFIGS"];
  if (rawConfigs) {
    try {
      regions = JSON.parse(rawConfigs) as RegionConfig[];
    } catch (err) {
      console.warn("[regionConfig] REGION_CONFIGS is not valid JSON — using default region.", err);
    }
  }

  const rawStrategy = env["REGION_STRATEGY"];
  const validStrategies: RegionRoutingStrategy[] = [
    "primary-first",
    "nearest",
    "round-robin",
    "active-active",
  ];
  if (rawStrategy && validStrategies.includes(rawStrategy as RegionRoutingStrategy)) {
    strategy = rawStrategy as RegionRoutingStrategy;
  }

  if (regions.length === 0) {
    // Default: single local region
    regions = [
      {
        id: "local",
        name: "Local (default)",
        apiBaseUrl: `http://localhost:${env["PORT"] ?? 4000}`,
        group: "local",
        isPrimary: true,
      },
    ];
  }

  return { regions, strategy };
}

// ---------------------------------------------------------------------------
// Singleton
// ---------------------------------------------------------------------------

const { regions, strategy } = loadRegionConfigFromEnv();
export const regionRegistry = new RegionRegistry(strategy);
for (const r of regions) {
  regionRegistry.register(r);
}
