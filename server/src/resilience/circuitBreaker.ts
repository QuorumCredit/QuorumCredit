/**
 * Issue #1762: Service Circuit Breaker for External APIs
 *
 * Implements the circuit breaker pattern to prevent cascading failures when
 * external APIs become unavailable. Tracks state per named service, supports
 * automatic recovery, and exposes state for monitoring.
 */

export type CircuitState = "closed" | "open" | "half-open";

export interface CircuitBreakerConfig {
  /** Number of consecutive failures before opening the circuit. */
  failureThreshold: number;
  /** Milliseconds to wait in open state before moving to half-open. */
  recoveryTimeoutMs: number;
  /** Number of successful calls in half-open state needed to close the circuit. */
  successThreshold: number;
}

export interface CircuitBreakerStatus {
  service: string;
  state: CircuitState;
  consecutiveFailures: number;
  consecutiveSuccesses: number;
  totalFailures: number;
  totalSuccesses: number;
  openedAt: number | undefined;
  lastFailureAt: number | undefined;
  lastSuccessAt: number | undefined;
}

export interface CircuitBreakerSnapshot {
  services: CircuitBreakerStatus[];
  totalOpen: number;
  totalHalfOpen: number;
  totalClosed: number;
}

const DEFAULT_CONFIG: CircuitBreakerConfig = {
  failureThreshold: 5,
  recoveryTimeoutMs: 30_000,
  successThreshold: 2,
};

interface CircuitEntry {
  config: CircuitBreakerConfig;
  state: CircuitState;
  consecutiveFailures: number;
  consecutiveSuccesses: number;
  totalFailures: number;
  totalSuccesses: number;
  openedAt: number | undefined;
  lastFailureAt: number | undefined;
  lastSuccessAt: number | undefined;
}

/**
 * Circuit breaker registry: manages per-service circuit breaker state,
 * automatic recovery via half-open probing, and fallback execution.
 */
export class CircuitBreakerService {
  private readonly circuits = new Map<string, CircuitEntry>();

  /** Register a named service with optional custom configuration. */
  register(service: string, config: Partial<CircuitBreakerConfig> = {}): void {
    if (!this.circuits.has(service)) {
      this.circuits.set(service, {
        config: { ...DEFAULT_CONFIG, ...config },
        state: "closed",
        consecutiveFailures: 0,
        consecutiveSuccesses: 0,
        totalFailures: 0,
        totalSuccesses: 0,
        openedAt: undefined,
        lastFailureAt: undefined,
        lastSuccessAt: undefined,
      });
    }
  }

  /** Returns true when the circuit is open and calls should be blocked. */
  isOpen(service: string): boolean {
    return this.getState(service) === "open";
  }

  /**
   * Execute a call guarded by the circuit breaker.
   * Throws if the circuit is open (fast-fail). On success/failure the
   * internal counters and state transition are applied automatically.
   * An optional fallback is called when the circuit is open.
   */
  async execute<T>(
    service: string,
    call: () => Promise<T>,
    fallback?: () => T | Promise<T>
  ): Promise<T> {
    const state = this.getState(service);

    if (state === "open") {
      if (fallback) return fallback();
      throw new Error(`Circuit open for service: ${service}`);
    }

    try {
      const result = await call();
      this.recordSuccess(service);
      return result;
    } catch (err) {
      this.recordFailure(service);
      if (fallback) return fallback();
      throw err;
    }
  }

  /** Manually record a successful call for a service. */
  recordSuccess(service: string): void {
    const entry = this.getOrRegister(service);
    entry.consecutiveFailures = 0;
    entry.consecutiveSuccesses += 1;
    entry.totalSuccesses += 1;
    entry.lastSuccessAt = Date.now();

    if (entry.state === "half-open") {
      if (entry.consecutiveSuccesses >= entry.config.successThreshold) {
        entry.state = "closed";
        entry.openedAt = undefined;
      }
    }
  }

  /** Manually record a failed call for a service. */
  recordFailure(service: string): void {
    const entry = this.getOrRegister(service);
    entry.consecutiveSuccesses = 0;
    entry.consecutiveFailures += 1;
    entry.totalFailures += 1;
    entry.lastFailureAt = Date.now();

    if (entry.state === "closed") {
      if (entry.consecutiveFailures >= entry.config.failureThreshold) {
        entry.state = "open";
        entry.openedAt = Date.now();
      }
    } else if (entry.state === "half-open") {
      // Failure in half-open — reopen the circuit.
      entry.state = "open";
      entry.openedAt = Date.now();
    }
  }

  /** Get the current state, advancing open→half-open if recovery timeout has elapsed. */
  getState(service: string): CircuitState {
    const entry = this.circuits.get(service);
    if (!entry) return "closed";

    if (
      entry.state === "open" &&
      entry.openedAt !== undefined &&
      Date.now() - entry.openedAt >= entry.config.recoveryTimeoutMs
    ) {
      entry.state = "half-open";
      entry.consecutiveSuccesses = 0;
    }

    return entry.state;
  }

  /** Status for a single service. */
  getStatus(service: string): CircuitBreakerStatus {
    const state = this.getState(service);
    const entry = this.circuits.get(service);
    return {
      service,
      state,
      consecutiveFailures: entry?.consecutiveFailures ?? 0,
      consecutiveSuccesses: entry?.consecutiveSuccesses ?? 0,
      totalFailures: entry?.totalFailures ?? 0,
      totalSuccesses: entry?.totalSuccesses ?? 0,
      openedAt: entry?.openedAt,
      lastFailureAt: entry?.lastFailureAt,
      lastSuccessAt: entry?.lastSuccessAt,
    };
  }

  /** Snapshot of all registered circuits. */
  getSnapshot(): CircuitBreakerSnapshot {
    const services: CircuitBreakerStatus[] = [];
    let totalOpen = 0;
    let totalHalfOpen = 0;
    let totalClosed = 0;

    for (const service of this.circuits.keys()) {
      const status = this.getStatus(service);
      services.push(status);
      if (status.state === "open") totalOpen += 1;
      else if (status.state === "half-open") totalHalfOpen += 1;
      else totalClosed += 1;
    }

    return { services, totalOpen, totalHalfOpen, totalClosed };
  }

  /** Manually reset a circuit to closed state. */
  reset(service: string): void {
    const entry = this.circuits.get(service);
    if (!entry) return;
    entry.state = "closed";
    entry.consecutiveFailures = 0;
    entry.consecutiveSuccesses = 0;
    entry.openedAt = undefined;
  }

  private getOrRegister(service: string): CircuitEntry {
    if (!this.circuits.has(service)) this.register(service);
    return this.circuits.get(service)!;
  }
}

export const circuitBreakerService = new CircuitBreakerService();
