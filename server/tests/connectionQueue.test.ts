import { describe, it, expect } from "vitest";
import { MetricsRegistry } from "../src/http/metricsRegistry.js";
import { ConnectionQueue } from "../src/ws/connectionQueue.js";

describe("ConnectionQueue backpressure metrics", () => {
  it("increments labeled counter on drop via onDrop callback", () => {
    const registry = new MetricsRegistry();
    const queue = new ConnectionQueue<number>(2);

    queue.push(1, () => registry.incLabeledCounter("qc_ws_queue_drops_total", "type", "loan"));
    queue.push(2, () => registry.incLabeledCounter("qc_ws_queue_drops_total", "type", "loan"));
    queue.push(3, () => registry.incLabeledCounter("qc_ws_queue_drops_total", "type", "loan"));

    expect(queue.size).toBe(2);
    expect(registry.toPrometheusText()).toContain('qc_ws_queue_drops_total{type="loan"} 1');
  });

  it("supports different labels for different connection types", () => {
    const registry = new MetricsRegistry();
    const loanQueue = new ConnectionQueue<number>(1);
    const metricsQueue = new ConnectionQueue<number>(1);

    loanQueue.push(1, () => registry.incLabeledCounter("qc_ws_queue_drops_total", "type", "loan"));
    loanQueue.push(2, () => registry.incLabeledCounter("qc_ws_queue_drops_total", "type", "loan"));

    metricsQueue.push(1, () => registry.incLabeledCounter("qc_ws_queue_drops_total", "type", "metrics"));
    metricsQueue.push(2, () => registry.incLabeledCounter("qc_ws_queue_drops_total", "type", "metrics"));

    const text = registry.toPrometheusText();
    expect(text).toContain('qc_ws_queue_drops_total{type="loan"} 1');
    expect(text).toContain('qc_ws_queue_drops_total{type="metrics"} 1');
  });

  it("does not increment when no onDrop callback is provided", () => {
    const registry = new MetricsRegistry();
    const queue = new ConnectionQueue<number>(1);

    queue.push(1);
    queue.push(2);

    const text = registry.toPrometheusText();
    expect(text).not.toContain("qc_ws_queue_drops_total");
  });
});
