/** Tiny counter/gauge registry for this service's own operational metrics — kept
 * dependency-free (hand-rolled Prometheus text exposition) rather than pulling in a
 * client library for a handful of numbers. */
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
