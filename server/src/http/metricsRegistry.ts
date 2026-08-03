/** Tiny counter/gauge registry for this service's own operational metrics — kept
 * dependency-free (hand-rolled Prometheus text exposition) rather than pulling in a
 * client library for a handful of numbers.
 *
 * Counters tracked:
 *   qc_broadcast_messages_dropped_total   — queue-overflow drops
 *   qc_ws_rate_limited_total              — inbound messages throttled (issue rate-limit)
 *   qc_ws_force_disconnected_rate_limit_total — connections force-closed for rate excess
 *   qc_ws_idle_closed_total               — connections closed by heartbeat idle timeout
 *
 * Gauges tracked:
 *   qc_broadcast_loan_connections         — current live loan socket connections
 *   qc_broadcast_metrics_connections      — current live metrics WS connections
 */
export class MetricsRegistry {
  private readonly counters = new Map<string, number>();
  private readonly gauges = new Map<string, number>();

  incCounter(name: string, by = 1): void {
    this.counters.set(name, (this.counters.get(name) ?? 0) + by);
  }

  setGauge(name: string, value: number): void {
    this.gauges.set(name, value);
  }

  toPrometheusText(): string {
    const lines: string[] = [];
    for (const [name, value] of this.counters) {
      lines.push(`# TYPE ${name} counter`, `${name} ${value}`);
    }
    for (const [name, value] of this.gauges) {
      lines.push(`# TYPE ${name} gauge`, `${name} ${value}`);
    }
    return lines.join("\n") + "\n";
  }
}

export const metrics = new MetricsRegistry();
