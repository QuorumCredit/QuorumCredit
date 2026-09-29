/**
 * Issue #1765: Service Health Dashboard
 *
 * Aggregates health checks across all registered services, tracks health
 * metrics over time, and provides alerting for unhealthy services.
 */

export type HealthStatus = "healthy" | "degraded" | "unhealthy" | "unknown";

export interface HealthCheckConfig {
  /** Human-readable service name. */
  name: string;
  /** Optional interval hint (milliseconds) — informational only, callers manage polling. */
  intervalMs?: number;
  /** Number of consecutive failures before status becomes "unhealthy". */
  unhealthyThreshold?: number;
  /** Number of consecutive successes after unhealthy to become "healthy" again. */
  recoveryThreshold?: number;
}

export interface HealthCheckResult {
  service: string;
  status: HealthStatus;
  responseTimeMs?: number;
  message?: string;
  checkedAt: number;
}

export interface ServiceHealth {
  service: string;
  status: HealthStatus;
  consecutiveFailures: number;
  consecutiveSuccesses: number;
  lastCheckedAt: number | undefined;
  lastHealthyAt: number | undefined;
  lastUnhealthyAt: number | undefined;
  /** Recent history — up to last 10 checks. */
  history: HealthCheckResult[];
}

export interface HealthDashboardSnapshot {
  services: ServiceHealth[];
  overallStatus: HealthStatus;
  totalServices: number;
  healthyCount: number;
  degradedCount: number;
  unhealthyCount: number;
  unknownCount: number;
  alerts: HealthAlert[];
  snapshotAt: number;
}

export interface HealthAlert {
  service: string;
  status: HealthStatus;
  message: string;
  triggeredAt: number;
}

const HISTORY_LIMIT = 10;
const DEFAULT_UNHEALTHY_THRESHOLD = 3;
const DEFAULT_RECOVERY_THRESHOLD = 2;

interface ServiceEntry {
  config: Required<HealthCheckConfig>;
  status: HealthStatus;
  consecutiveFailures: number;
  consecutiveSuccesses: number;
  lastCheckedAt: number | undefined;
  lastHealthyAt: number | undefined;
  lastUnhealthyAt: number | undefined;
  history: HealthCheckResult[];
}

/**
 * Central health dashboard: registers services, records check results,
 * tracks status transitions, and exposes a dashboard snapshot.
 */
export class ServiceHealthDashboard {
  private readonly services = new Map<string, ServiceEntry>();
  private readonly activeAlerts = new Map<string, HealthAlert>();

  /** Register a service for health tracking. */
  register(config: HealthCheckConfig): void {
    if (!this.services.has(config.name)) {
      this.services.set(config.name, {
        config: {
          name: config.name,
          intervalMs: config.intervalMs ?? 60_000,
          unhealthyThreshold: config.unhealthyThreshold ?? DEFAULT_UNHEALTHY_THRESHOLD,
          recoveryThreshold: config.recoveryThreshold ?? DEFAULT_RECOVERY_THRESHOLD,
        },
        status: "unknown",
        consecutiveFailures: 0,
        consecutiveSuccesses: 0,
        lastCheckedAt: undefined,
        lastHealthyAt: undefined,
        lastUnhealthyAt: undefined,
        history: [],
      });
    }
  }

  /**
   * Record the outcome of a health check for a service.
   * Automatically transitions state and fires/clears alerts.
   */
  recordResult(result: HealthCheckResult): void {
    if (!this.services.has(result.service)) {
      this.register({ name: result.service });
    }
    const entry = this.services.get(result.service)!;
    entry.lastCheckedAt = result.checkedAt;

    // Maintain rolling history
    entry.history.push(result);
    if (entry.history.length > HISTORY_LIMIT) entry.history.shift();

    const isSuccess = result.status === "healthy";

    if (isSuccess) {
      entry.consecutiveFailures = 0;
      entry.consecutiveSuccesses += 1;
      entry.lastHealthyAt = result.checkedAt;

      if (
        entry.status !== "healthy" &&
        entry.consecutiveSuccesses >= entry.config.recoveryThreshold
      ) {
        entry.status = "healthy";
        this.clearAlert(result.service);
      } else if (entry.status === "unknown") {
        entry.status = "healthy";
        this.clearAlert(result.service);
      }
    } else {
      entry.consecutiveSuccesses = 0;
      entry.consecutiveFailures += 1;

      if (result.status === "unhealthy" || result.status === "degraded") {
        entry.lastUnhealthyAt = result.checkedAt;
      }

      const shouldDeclareUnhealthy =
        entry.consecutiveFailures >= entry.config.unhealthyThreshold;

      if (shouldDeclareUnhealthy && entry.status !== "unhealthy") {
        entry.status = "unhealthy";
        this.raiseAlert({
          service: result.service,
          status: "unhealthy",
          message:
            result.message ??
            `${result.service} has failed ${entry.consecutiveFailures} consecutive health checks`,
          triggeredAt: result.checkedAt,
        });
      } else if (!shouldDeclareUnhealthy && entry.status === "healthy") {
        entry.status = "degraded";
      }
    }
  }

  /** Get health information for a single service. */
  getServiceHealth(service: string): ServiceHealth | undefined {
    const entry = this.services.get(service);
    if (!entry) return undefined;
    return this.toServiceHealth(service, entry);
  }

  /** Full dashboard snapshot. */
  getSnapshot(): HealthDashboardSnapshot {
    const serviceHealthList: ServiceHealth[] = [];
    let healthyCount = 0;
    let degradedCount = 0;
    let unhealthyCount = 0;
    let unknownCount = 0;

    for (const [name, entry] of this.services.entries()) {
      const sh = this.toServiceHealth(name, entry);
      serviceHealthList.push(sh);
      switch (sh.status) {
        case "healthy": healthyCount += 1; break;
        case "degraded": degradedCount += 1; break;
        case "unhealthy": unhealthyCount += 1; break;
        default: unknownCount += 1;
      }
    }

    const overallStatus: HealthStatus =
      unhealthyCount > 0
        ? "unhealthy"
        : degradedCount > 0
          ? "degraded"
          : unknownCount === serviceHealthList.length
            ? "unknown"
            : "healthy";

    return {
      services: serviceHealthList,
      overallStatus,
      totalServices: serviceHealthList.length,
      healthyCount,
      degradedCount,
      unhealthyCount,
      unknownCount,
      alerts: Array.from(this.activeAlerts.values()),
      snapshotAt: Date.now(),
    };
  }

  /** Active alerts only. */
  getAlerts(): HealthAlert[] {
    return Array.from(this.activeAlerts.values());
  }

  private raiseAlert(alert: HealthAlert): void {
    this.activeAlerts.set(alert.service, alert);
  }

  private clearAlert(service: string): void {
    this.activeAlerts.delete(service);
  }

  private toServiceHealth(name: string, entry: ServiceEntry): ServiceHealth {
    return {
      service: name,
      status: entry.status,
      consecutiveFailures: entry.consecutiveFailures,
      consecutiveSuccesses: entry.consecutiveSuccesses,
      lastCheckedAt: entry.lastCheckedAt,
      lastHealthyAt: entry.lastHealthyAt,
      lastUnhealthyAt: entry.lastUnhealthyAt,
      history: [...entry.history],
    };
  }
}

export const serviceHealthDashboard = new ServiceHealthDashboard();
